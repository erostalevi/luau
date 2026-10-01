// "Create card from clipboard": As is, or Let AI review it (local AI only).
// Rules live in ./clipboardCards.ts; the AI drafts come from the core
// (`ai.cardsFromText`, validated there). Every insertion is one undo step:
// one `batch` op on a board, one CodeMirror transaction in a document.

import { ClipboardPlus } from '@lucide/svelte';
import type { Command } from '$lib/commands/registry.svelte';
import { runCommand } from '$lib/commands/registry.svelte';
import { apply, boards, newCardId, type BoardModel } from '$lib/state/boards.svelte';
import { selection } from '$lib/state/selection.svelte';
import { activeTab } from '$lib/state/workspace.svelte';
import { ui } from '$lib/state/ui.svelte';
import { toast, dismiss } from '$lib/state/toasts.svelte';
import { confirm } from '$lib/state/dialogs.svelte';
import { pickOne } from '$lib/quickinput/qi.svelte';
import { activeEditor, type ActiveEditor } from '$lib/editor/active';
import { flash } from '$lib/board/boardUi.svelte';
import { readClipboardText, tabBoard } from '$lib/app/helpers';
import { ai } from '$lib/summaries/api';
import { t } from '$lib/i18n/index.svelte';
import {
  acceptAiCards,
  asIsCard,
  asIsSection,
  blockForInsertion,
  cleanText,
  createCardsOp,
  insertionTarget,
  previewText,
  tooLong,
  type InsertTarget,
} from './clipboardCards';

type Mode = 'asIs' | 'ai';

interface Resolved {
  board: BoardModel;
  target: InsertTarget;
  editor: ActiveEditor | null;
}

/** Board + target, captured before the quick pick moves the focus. */
function resolveTarget(): Resolved | null {
  const tab = activeTab();
  const editorCard = ui.editor.open && ui.editor.cardId ? ui.editor : null;
  const board = (editorCard && boards.get(editorCard.boardId)) || tabBoard();
  if (!board) return null;
  const e = activeEditor();
  const docEditor = tab?.kind === 'doc' && !!e && e.boardId === board.id && e.cardId === tab.cardId && (!editorCard || editorCard.cardId === tab.cardId);
  const own = selection.boardId === board.id;
  const target = insertionTarget({
    board,
    docEditor,
    focus: editorCard?.cardId || (tab?.kind === 'doc' ? (tab.cardId ?? null) : null) || (own ? selection.focus || null : null),
    lane: own ? selection.lane || null : null,
  });
  return target ? { board, target, editor: target.kind === 'doc' ? e : null } : null;
}

function insertIntoDoc(e: ActiveEditor, sections: string[]) {
  const view = e.view;
  const { from, to } = view.state.selection.main;
  const insert = blockForInsertion(sections, view.state.sliceDoc(0, from), view.state.sliceDoc(to));
  if (!insert) return;
  view.dispatch({ changes: { from, to, insert }, selection: { anchor: from + insert.length }, scrollIntoView: true, userEvent: 'input.paste' });
  view.focus();
}

async function createCards(r: Resolved, contents: string[]) {
  if (r.target.kind !== 'cards') return;
  const { parent, before } = r.target;
  const ids: string[] = [];
  for (let i = 0; i < contents.length; i++) ids.push(await newCardId());
  const op = createCardsOp(ids, contents, parent, r.board.childrenOf(parent), before);
  const label = contents.length > 1 ? t('clipboard.opMany', { count: contents.length }) : t('ops.createCard');
  const res = await apply(r.board.id, op, label);
  if (!res) return;
  ids.forEach((id) => flash(id));
  toast.success(t('clipboard.created', { count: contents.length }), {
    action: { label: t('common.undo'), run: () => void runCommand('edit.undo') },
  });
}

async function aiDrafts(text: string) {
  const tid = toast.info(t('clipboard.reviewing'), { timeout: 0 });
  try {
    const r = await ai.cardsFromText(text);
    const cards = acceptAiCards(r);
    if (!cards.length) throw new Error(t('clipboard.noCards'));
    return { cards, truncated: r.truncated };
  } catch (e) {
    toast.error(t('clipboard.aiFailed', { message: (e as Error).message }), {
      action: { label: t('clipboard.asIs'), run: () => void fromClipboard('asIs') },
    });
    return null;
  } finally {
    dismiss(tid);
  }
}

export async function fromClipboard(preset?: Mode) {
  const r = resolveTarget();
  if (!r) {
    toast.warn(t(tabBoard() || ui.editor.open ? 'clipboard.noLane' : 'clipboard.noBoard'));
    return;
  }
  if (r.board.header.readOnly) {
    toast.warn(t('clipboard.readOnly'));
    return;
  }
  const text = cleanText(await readClipboardText());
  if (!text.trim()) {
    toast.warn(t('clipboard.empty'));
    return;
  }
  if (tooLong(text)) {
    toast.warn(t('clipboard.tooLong'));
    return;
  }
  const mode =
    preset ??
    (await pickOne<Mode>(
      [
        { label: t('clipboard.asIs'), description: t('clipboard.asIsDesc'), value: 'asIs' },
        { label: t('clipboard.aiReview'), description: t('clipboard.aiReviewDesc'), value: 'ai' },
      ],
      { title: t('commands.card.newFromClipboard'), placeholder: t('clipboard.how') },
    ));
  if (mode !== 'asIs' && mode !== 'ai') return;

  if (mode === 'asIs') {
    if (r.target.kind === 'doc' && r.editor) insertIntoDoc(r.editor, [asIsSection(text) ?? '']);
    else await createCards(r, [asIsCard(text) ?? '']);
    return;
  }

  const drafts = await aiDrafts(text);
  if (!drafts) return;
  const { cards, truncated } = drafts;
  const intoDoc = r.target.kind === 'doc' && !!r.editor;
  const ok = await confirm({
    title: t(intoDoc ? 'clipboard.confirmDoc' : 'clipboard.confirmCards', { count: cards.length }),
    message: truncated ? t('clipboard.truncated') : t('clipboard.confirmHint'),
    preview: previewText(cards),
    confirmLabel: t(intoDoc ? 'clipboard.insert' : 'clipboard.create', { count: cards.length }),
  });
  if (!ok) return;
  if (intoDoc && r.editor)
    insertIntoDoc(
      r.editor,
      cards.map((c) => c.section),
    );
  else
    await createCards(
      r,
      cards.map((c) => c.markdown),
    );
}

export const commands: Command[] = [
  {
    id: 'card.newFromClipboard',
    title: 'commands.card.newFromClipboard',
    category: 'card',
    icon: ClipboardPlus,
    when: '!boardReadOnly',
    run: () => fromClipboard(),
  },
];
