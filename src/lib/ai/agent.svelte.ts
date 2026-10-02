// "AI…" agent turns: ask the core for a plan, then apply it.
//
// - Board changes go out as one `batch` op per board, so each board gets a
//   single undo step; `rememberAiRun` makes ⌘Z undo the whole run at once
//   when it spans several boards.
// - Plans that leave the boards (Jira / Trello comments and transitions,
//   Slack messages), delete cards, or change more than 10 cards wait for the
//   user's confirmation (the core sets `confirm`).

import { rpc } from '$lib/backend/rpc';
import type { Op, Parent } from '$lib/backend/types';
import { boards, openBoard, apply, rememberAiRun } from '$lib/state/boards.svelte';
import { setTitle, appendToBody } from '$lib/markdown/meta';
import { withField } from '$lib/editor/propertyMenu';
import { activeBoard } from '$lib/app/helpers';
import { t } from '$lib/i18n/index.svelte';
import { assistant, pushTurn, currentRun, nextRun, type Turn } from './assistant.svelte';

export interface PlannedAction {
  type:
    | 'create_card'
    | 'move_card'
    | 'rename_card'
    | 'append_text'
    | 'set_property'
    | 'add_tag'
    | 'archive_card'
    | 'delete_card'
    | 'remote_comment'
    | 'remote_transition'
    | 'slack_message';
  board: string;
  boardName: string;
  card: string;
  cardTitle: string;
  lane: string;
  laneName: string;
  title: string;
  text: string;
  key: string;
  value: string;
  channel: string;
  because: string;
  risky: boolean;
}

export interface Plan {
  answer: string;
  actions: PlannedAction[];
  rejected: { type: string; reason: string; detail: string }[];
  confirm: boolean;
  provider: string;
  model: string;
}

export interface PlanState {
  plan: Plan;
  /** `proposed` (waiting for OK) → `applying` → `applied` | `failed`; or `cancelled`. */
  status: 'proposed' | 'applying' | 'applied' | 'failed' | 'cancelled';
  /** Per action: null = pending, '' = done, else the error. */
  results: (string | null)[];
  /** Actions the user unticked before applying. */
  skip: boolean[];
}

const LOCAL = new Set(['create_card', 'move_card', 'rename_card', 'append_text', 'set_property', 'add_tag', 'archive_card', 'delete_card']);

function history(): { role: string; text: string }[] {
  return assistant.turns
    .filter((x) => x.mode === 'agent' && x.text && !x.streaming)
    .slice(0, -1)
    .map((x) => ({ role: x.role === 'user' ? 'user' : 'assistant', text: x.text }));
}

export async function runAgentTurn(message: string) {
  const run = nextRun();
  const turn = pushTurn({ role: 'ai', text: '', mode: 'agent', streaming: true });
  assistant.busy = true;
  try {
    const plan = await rpc<Plan>('ai.agent', { message, history: history(), board: activeBoard()?.id ?? null });
    if (run !== currentRun()) return;
    turn.text = plan.answer || (plan.actions.length ? t('assistant.plan.ready') : t('assistant.plan.nothing'));
    turn.model = plan.model;
    if (plan.actions.length || plan.rejected.length) {
      turn.plan = { plan, status: 'proposed', results: plan.actions.map(() => null), skip: plan.actions.map(() => false) } satisfies PlanState;
      if (!plan.confirm && plan.actions.length) await applyPlan(turn);
    }
  } catch (e) {
    if (run === currentRun()) turn.error = (e as Error).message;
  } finally {
    if (run === currentRun()) {
      turn.streaming = false;
      assistant.busy = false;
    }
  }
}

export function cancelPlan(turn: Turn) {
  const st = turn.plan as PlanState | undefined;
  if (st?.status === 'proposed') st.status = 'cancelled';
}

async function laneId(board: string, name: string): Promise<string | null> {
  const b = boards.get(board) ?? (await openBoard({ id: board }));
  const n = name.trim().toLowerCase();
  return b.lanes.find((l) => !l.archived && l.name.trim().toLowerCase() === n)?.id ?? null;
}

/** Card content after the text edits in `acts` (pure). */
export function editContent(content: string, acts: PlannedAction[]): string {
  let c = content;
  for (const a of acts) {
    if (a.type === 'rename_card') c = setTitle(c, a.title);
    else if (a.type === 'set_property') c = withField(c, a.key, a.key === 'assignees' ? a.value.replace(/(^|,\s*)@?/g, '$1@') : a.value);
    else if (a.type === 'append_text' || a.type === 'add_tag') c = appendToBody(c, a.type === 'add_tag' ? `#${a.value}` : a.text);
  }
  return c;
}

export async function applyPlan(turn: Turn) {
  const st = turn.plan as PlanState | undefined;
  if (!st || (st.status !== 'proposed' && st.status !== 'failed')) return;
  st.status = 'applying';
  const acts = st.plan.actions.map((a, i) => ({ a, i })).filter(({ i }) => !st.skip[i]);
  const label = t('assistant.undoLabel');
  const touchedBoards: string[] = [];
  let failed = false;

  // 1. Board changes: one batch per board.
  const byBoard = new Map<string, { a: PlannedAction; i: number }[]>();
  for (const x of acts) if (LOCAL.has(x.a.type)) byBoard.set(x.a.board, [...(byBoard.get(x.a.board) ?? []), x]);
  for (const [board, list] of byBoard) {
    try {
      await openBoard({ id: board });
      const ops: Op[] = [];
      const edits = new Map<string, PlannedAction[]>();
      for (const { a } of list) {
        if (a.type === 'create_card') {
          const id = await rpc<string>('board.newCardId');
          const lane = a.laneName ? await laneId(board, a.laneName) : null;
          const parent: Parent = lane ? { kind: 'lane', id: lane } : { kind: 'root' };
          ops.push({ op: 'createCard', id, parent, index: null, content: `# ${a.title}\n${a.text ? `\n${a.text}\n` : ''}` });
        } else if (a.type === 'move_card') {
          const lane = await laneId(board, a.laneName);
          if (!lane) throw new Error(t('assistant.plan.noLane', { lane: a.laneName }));
          ops.push({ op: 'move', ids: [a.card], to: { kind: 'lane', id: lane }, before: null });
        } else if (a.type === 'archive_card') ops.push({ op: 'setArchived', nodes: [[a.card, true]], lanes: [] });
        else if (a.type === 'delete_card') ops.push({ op: 'trash', nodes: [a.card], lanes: [] });
        else edits.set(a.card, [...(edits.get(a.card) ?? []), a]);
      }
      for (const [card, list] of edits) {
        const content = await rpc<string>('card.read', { board, id: card });
        const next = editContent(content, list);
        if (next !== content) ops.unshift({ op: 'writeCard', id: card, content: next });
      }
      const res = ops.length ? await apply(board, { op: 'batch', ops }, label) : { version: 0, created: [], trashed: [] };
      if (!res) throw new Error(t('assistant.plan.failed'));
      touchedBoards.push(board);
      for (const { i } of list) st.results[i] = '';
    } catch (e) {
      failed = true;
      for (const { i } of list) st.results[i] = (e as Error).message || t('assistant.plan.failed');
    }
  }
  if (touchedBoards.length) rememberAiRun(label, touchedBoards);

  // 2. Outside the boards (already confirmed by the user when present).
  for (const { a, i } of acts) {
    if (LOCAL.has(a.type)) continue;
    try {
      if (a.type === 'slack_message') await rpc('ai.slackSend', { channel: a.channel, text: a.text });
      else if (a.type === 'remote_comment') {
        const { runGated } = await import('$lib/integrations/gate');
        if ((await runGated({ kind: 'comment', board: a.board, card: a.card, body: a.text })) === null) throw new Error(t('assistant.plan.skipped'));
      } else if (a.type === 'remote_transition') {
        const remote = (boards.get(a.board) ?? (await openBoard({ id: a.board }))).remote.get(a.card);
        if (!remote) throw new Error(t('assistant.plan.notLinked'));
        const list = await rpc<{ id: string; name: string; to: string; toCategory: string }[]>('remote.transitions', {
          account: remote.account,
          key: remote.key,
        });
        const v = a.value.trim().toLowerCase();
        const tr = list.find((x) => x.to.toLowerCase() === v) ?? list.find((x) => x.name.toLowerCase() === v);
        if (!tr) throw new Error(t('assistant.plan.noTransition', { status: a.value }));
        const { transitionCard } = await import('$lib/integrations/actions');
        await transitionCard(a.board, a.card, tr as never);
      }
      st.results[i] = '';
    } catch (e) {
      failed = true;
      st.results[i] = (e as Error).message || t('assistant.plan.failed');
    }
  }
  for (const { i } of acts) st.results[i] ??= '';
  st.status = failed ? 'failed' : 'applied';
}
