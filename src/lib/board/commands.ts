import { Plus, FilePlus2, Columns3, ArrowUp, ArrowDown, ArrowLeft, ArrowRight, Indent, Outdent, Trash2, Archive, Copy, Link, Pencil, Tag, CalendarDays, Flag, UserPlus, ArrowRightLeft, Image, LayoutTemplate, ExternalLink, SquareDashedMousePointer } from '@lucide/svelte';
import type { Command } from '$lib/commands/registry.svelte';
import type { Parent } from '$lib/backend/types';
import { rpc } from '$lib/backend/rpc';
import { boards, apply, openBoard, type BoardModel } from '$lib/state/boards.svelte';
import { selection, select, selectMany, clearSelection } from '$lib/state/selection.svelte';
import { ui, openEditor } from '$lib/state/ui.svelte';
import { activeTab, openDocTab, openBoardTab } from '$lib/state/workspace.svelte';
import { registry } from '$lib/state/registry.svelte';
import { quickPick, pickOne, inputBox, steps, BACK, type QuickItem } from '$lib/quickinput/qi.svelte';
import { settings } from '$lib/settings/store.svelte';
import { tabBoard, activeCard, copyText } from '$lib/app/helpers';
import { openCard } from '$lib/app/open';
import { toast } from '$lib/state/toasts.svelte';
import { t, fmtDate } from '$lib/i18n/index.svelte';
import { boardUi, listKey, flash } from './boardUi.svelte';
import { createCard, trashCards, setArchived, duplicateCard, addTag, setFooterField, readCard } from './cardActions';
import { CARD_TEMPLATES } from './templates';

// --- helpers ---------------------------------------------------------------

function focusBoard(): BoardModel | null {
  return tabBoard();
}

function targets(): { b: BoardModel; ids: string[] } | null {
  const b = focusBoard();
  if (!b) {
    const c = activeCard();
    return c ? { b: c.board, ids: [c.id] } : null;
  }
  if (selection.boardId !== b.id || !selection.ids.length) {
    const c = activeCard();
    return c && c.board.id === b.id ? { b, ids: [c.id] } : null;
  }
  return { b, ids: selection.ids.filter((id) => b.node(id)) };
}

function visibleCards(): HTMLElement[] {
  return [...document.querySelectorAll<HTMLElement>('[data-board-scroll] [data-card]')].filter((el) => el.offsetParent !== null);
}

function scrollTo(id: string) {
  document.querySelector(`[data-board-scroll] [data-card="${id}"]`)?.scrollIntoView({ block: 'nearest', inline: 'nearest', behavior: 'smooth' });
}

/** Spatial navigation (works for columns, rows and wrapped layouts). */
function navigate(dir: 'up' | 'down' | 'left' | 'right', extend = false) {
  const b = focusBoard();
  if (!b) return;
  const cards = visibleCards();
  if (!cards.length) return;
  const curEl = selection.boardId === b.id && selection.focus ? cards.find((c) => c.dataset.card === selection.focus) : undefined;
  if (!curEl) {
    const first = cards[0].dataset.card!;
    select(b.id, first);
    scrollTo(first);
    return;
  }
  const r = curEl.getBoundingClientRect();
  const cx = r.left + r.width / 2;
  const cy = r.top + r.height / 2;
  let best: { id: string; score: number } | null = null;
  for (const el of cards) {
    if (el === curEl || el.contains(curEl) || curEl.contains(el)) continue;
    const o = el.getBoundingClientRect();
    const ox = o.left + o.width / 2;
    const oy = o.top + o.height / 2;
    let ok = false;
    let score = 0;
    if (dir === 'down') {
      ok = o.top >= r.top + 4 && oy > cy;
      score = oy - cy + Math.abs(ox - cx) * 3;
    } else if (dir === 'up') {
      ok = o.bottom <= r.bottom - 4 && oy < cy;
      score = cy - oy + Math.abs(ox - cx) * 3;
    } else if (dir === 'right') {
      ok = o.left >= r.right - 4;
      score = ox - cx + Math.abs(oy - cy) * 1.5;
    } else if (dir === 'left') {
      ok = o.right <= r.left + 4;
      score = cx - ox + Math.abs(oy - cy) * 1.5;
    }
    if (ok && (!best || score < best.score)) best = { id: el.dataset.card!, score };
  }
  if (!best) return;
  if (extend) select(b.id, best.id, 'add');
  else select(b.id, best.id);
  scrollTo(best.id);
}

/** Where ⌘N / "new card" should create, based on focus. */
function newCardTarget(b: BoardModel): { parent: Parent; before: string | null } | null {
  if (b.kind === 'files') return { parent: { kind: 'root' }, before: null };
  if (selection.boardId === b.id && selection.focus) {
    const n = b.node(selection.focus);
    if (n) {
      const sibs = b.childrenOf(n.parent);
      return { parent: n.parent, before: sibs[sibs.indexOf(n.id) + 1] ?? null };
    }
  }
  const lane = (selection.boardId === b.id && selection.lane && b.lane(selection.lane)) || b.lanes.find((l) => !l.archived);
  return lane ? { parent: { kind: 'lane', id: lane.id }, before: null } : null;
}

async function newCard() {
  // Editor open: create a sibling of the edited card and open it.
  if (ui.editor.open) {
    const b = boards.get(ui.editor.boardId);
    const n = b?.node(ui.editor.cardId);
    if (b && n) {
      const sibs = b.childrenOf(n.parent);
      const id = await createCard(b.id, n.parent, sibs[sibs.indexOf(n.id) + 1] ?? null, '# \n');
      if (id) openEditor(b.id, id);
      return;
    }
  }
  const tab = activeTab();
  if (tab?.kind === 'doc' && tab.boardId) {
    const b = boards.get(tab.boardId);
    const n = b?.node(tab.cardId!);
    const parent: Parent = n?.parent ?? { kind: 'root' };
    const sibs = b?.childrenOf(parent) ?? [];
    const id = await createCard(tab.boardId, parent, n ? (sibs[sibs.indexOf(n.id) + 1] ?? null) : null, '# \n');
    if (id) await openDocTab(tab.boardId, id);
    return;
  }
  const b = focusBoard();
  if (!b) {
    const pick = await pickOne(
      registry.data.boards.filter((x) => !x.hidden && !x.missing && !x.mirror).map((x) => ({ label: x.name, value: x.id })),
      { title: t('cards.newIn') },
    );
    if (typeof pick !== 'string') return;
    await openBoardTab(pick);
    queueMicrotask(() => void newCard());
    return;
  }
  if (b.header.readOnly) return;
  if (b.kind === 'files') {
    const id = await createCard(b.id, { kind: 'root' }, null, '# \n');
    if (id) await openDocTab(b.id, id);
    return;
  }
  const tg = newCardTarget(b);
  if (!tg) {
    boardUi.newLane = { boardId: b.id, index: null };
    return;
  }
  boardUi.quickAdd = { boardId: b.id, parent: tg.parent, before: tg.before, key: listKey(b.id, tg.parent) };
}

async function moveWithin(dir: -1 | 1) {
  const tg = targets();
  if (!tg || tg.ids.length !== 1) return;
  const { b, ids } = tg;
  const n = b.node(ids[0]);
  if (!n) return;
  const sibs = b.childrenOf(n.parent).filter((id) => ui.showArchived || !b.node(id)?.archived);
  const i = sibs.indexOf(n.id);
  const j = i + dir;
  if (j < 0 || j >= sibs.length) return;
  const before = dir < 0 ? sibs[j] : (sibs[j + 1] ?? null);
  await apply(b.id, { op: 'move', ids, to: n.parent, before }, t('ops.moveCard'));
  scrollTo(ids[0]);
}

async function moveLane(dir: -1 | 1) {
  const tg = targets();
  if (!tg) return;
  const { b, ids } = tg;
  const laneId = b.laneOf(ids[0]);
  const lanes = b.lanes.filter((l) => ui.showArchived || !l.archived);
  const li = lanes.findIndex((l) => l.id === laneId);
  const target = lanes[li + dir];
  if (!target) return;
  // Keep the vertical position of the top-level ancestor.
  let top = b.node(ids[0]);
  while (top && top.parent.kind === 'card') top = b.node(top.parent.id);
  const idx = top ? (b.lane(laneId!)?.order.indexOf(top.id) ?? 0) : 0;
  const before = target.order[idx] ?? null;
  await apply(b.id, { op: 'move', ids, to: { kind: 'lane', id: target.id }, before }, t('ops.moveCard'));
  scrollTo(ids[0]);
}

async function indent() {
  const tg = targets();
  if (!tg || tg.ids.length !== 1) return;
  const { b, ids } = tg;
  const n = b.node(ids[0]);
  if (!n) return;
  const sibs = b.childrenOf(n.parent);
  const prev = sibs[sibs.indexOf(n.id) - 1];
  if (!prev) return;
  await apply(b.id, { op: 'move', ids, to: { kind: 'card', id: prev }, before: null }, t('ops.indent'));
}

async function outdent() {
  const tg = targets();
  if (!tg || tg.ids.length !== 1) return;
  const { b, ids } = tg;
  const n = b.node(ids[0]);
  if (!n || n.parent.kind !== 'card') return;
  const parent = b.node(n.parent.id);
  if (!parent) return;
  const sibs = b.childrenOf(parent.parent);
  await apply(b.id, { op: 'move', ids, to: parent.parent, before: sibs[sibs.indexOf(parent.id) + 1] ?? null }, t('ops.outdent'));
}

async function moveTo() {
  const tg = targets();
  if (!tg) return;
  const { b: src, ids } = tg;
  const res = await steps<[string, Parent, 'top' | 'bottom']>([
    () =>
      pickOne(
        [
          { label: src.header.name, description: t('cards.thisBoard'), value: src.id },
          ...registry.data.boards.filter((x) => x.id !== src.id && !x.hidden && !x.missing && !x.mirror).map((x) => ({ label: x.name, value: x.id })),
        ],
        { title: t('cards.moveTo'), step: 1, totalSteps: 3 },
      ),
    async ([boardId]) => {
      const b = boards.get(boardId) ?? (await openBoard({ id: boardId }));
      const items: QuickItem<Parent>[] =
        b.kind === 'files'
          ? [{ label: t('cards.topLevel'), value: { kind: 'root' } }]
          : b.lanes.filter((l) => !l.archived).map((l) => ({ label: l.name, value: { kind: 'lane', id: l.id } as Parent, icon: Columns3 }));
      const groups = [...b.nodes.values()]
        .filter((n) => !ids.includes(n.id) && !ids.some((i) => b.isAncestor(i, n.id)))
        .slice(0, 400)
        .map((n) => ({ label: n.title || t('common.untitled'), description: t('cards.insideCard'), value: { kind: 'card', id: n.id } as Parent, icon: FilePlus2 }));
      return pickOne([...items, { kind: 'separator', label: t('cards.insideCard') }, ...groups], { title: t('cards.pickDestination'), step: 2, totalSteps: 3, matchOnDescription: false });
    },
    () =>
      pickOne(
        [
          { label: t('cards.atTop'), value: 'top' as const },
          { label: t('cards.atBottom'), value: 'bottom' as const },
        ],
        { title: t('cards.pickPosition'), step: 3, totalSteps: 3 },
      ),
  ]);
  if (!res) return;
  const [boardId, parent, pos] = res;
  const target = boards.get(boardId)!;
  const before = pos === 'top' ? (target.childrenOf(parent)[0] ?? null) : null;
  if (boardId === src.id) await apply(src.id, { op: 'move', ids, to: parent, before }, t('ops.moveCard'));
  else {
    await rpc('board.moveAcross', { from: src.id, ids, to: boardId, parent, before });
    toast.success(t('toasts.movedToBoard', { count: ids.length, board: target.header.name }));
  }
}

async function pickTag(): Promise<string | undefined> {
  const tags = await rpc<[string, number][]>('search.tags', { boards: [] }).catch(() => []);
  const r = await quickPick(
    tags.map(([tag, n]) => ({ label: `#${tag}`, value: tag, description: String(n) })),
    {
      title: t('cards.addTag'),
      placeholder: t('cards.tagPlaceholder'),
      allowCustom: (text) => {
        const clean = text.replace(/^#/, '').trim().replace(/\s+/g, '-');
        return clean ? { label: t('cards.createTag', { tag: clean }), value: clean } : null;
      },
    },
  );
  return typeof r === 'string' ? r : undefined;
}

function isoDate(d: Date): string {
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
}

async function pickDate(title: string): Promise<string | null | undefined> {
  const today = new Date();
  const add = (n: number) => {
    const d = new Date(today);
    d.setDate(d.getDate() + n);
    return isoDate(d);
  };
  const nextMonday = add(((8 - today.getDay()) % 7) || 7);
  const items: QuickItem<string>[] = [
    { label: t('dates.today'), description: fmtDate(add(0), { weekday: 'short', month: 'short', day: 'numeric' }), value: add(0) },
    { label: t('dates.tomorrow'), description: fmtDate(add(1), { weekday: 'short', month: 'short', day: 'numeric' }), value: add(1) },
    { label: t('dates.nextMonday'), description: fmtDate(nextMonday, { weekday: 'short', month: 'short', day: 'numeric' }), value: nextMonday },
    { label: t('dates.inAWeek'), description: fmtDate(add(7), { weekday: 'short', month: 'short', day: 'numeric' }), value: add(7) },
    { label: t('dates.custom'), value: 'custom' },
    { label: t('dates.clear'), value: '' },
  ];
  const r = await pickOne(items, { title });
  if (r === undefined || r === BACK) return undefined;
  if (r !== 'custom') return r;
  const v = await inputBox({
    title,
    placeholder: 'YYYY-MM-DD',
    value: add(0),
    validate: (s) => (/^\d{4}-\d{2}-\d{2}$/.test(s) && !Number.isNaN(Date.parse(s)) ? null : t('validation.date')),
  });
  return typeof v === 'string' ? v : undefined;
}

async function forEachTarget(fn: (b: BoardModel, id: string) => Promise<unknown>) {
  const tg = targets();
  if (!tg) return;
  for (const id of tg.ids) await fn(tg.b, id);
}

const ON_BOARD = "tabKind == 'board'";


/** The user's own @name (setting `general.yourName`); asks once when unset. */
export async function myName(): Promise<string | null> {
  const cur = (settings.get<string>('general.yourName') ?? '').trim().replace(/^@/, '');
  if (cur) return cur;
  const v = await inputBox({ title: t('cards.askName'), placeholder: '@name', validate: (x) => (/^@?[\p{L}\p{N}._-]{1,40}$/u.test(x.trim()) ? null : t('validation.required')) });
  if (typeof v !== 'string' || !v.trim()) return null;
  const name = v.trim().replace(/^@/, '');
  settings.set('general.yourName', name);
  return name;
}

export const boardCardCommands: Command[] = [
  { id: 'card.new', title: 'commands.card.new', category: 'card', icon: Plus, run: newCard },
  {
    id: 'card.newSubcard',
    title: 'commands.card.newSubcard',
    category: 'card',
    icon: FilePlus2,
    run: () => {
      const c = activeCard();
      if (!c) return;
      const parent: Parent = { kind: 'card', id: c.id };
      if (c.board.kind === 'files') {
        void createCard(c.board.id, parent, null, '# \n').then((id) => {
          if (id) void openDocTab(c.board.id, id);
        });
        return;
      }
      boardUi.collapsed[c.id] = false;
      boardUi.quickAdd = { boardId: c.board.id, parent, before: null, key: listKey(c.board.id, parent) };
    },
  },
  {
    id: 'card.newFromTemplate',
    title: 'commands.card.newFromTemplate',
    category: 'card',
    icon: LayoutTemplate,
    run: async () => {
      const b = focusBoard() ?? activeCard()?.board;
      if (!b) return;
      const custom = await rpc<{ name: string; content: string }[]>('templates.list', { board: b.id }).catch(() => []);
      const tpl = await pickOne(
        [
          ...CARD_TEMPLATES.map((x) => ({ label: t(`templates.cards.${x.id}.name`), value: () => x.body() })),
          ...(custom.length ? [{ kind: 'separator' as const, label: t('templates.custom') }] : []),
          ...custom.map((c) => ({ label: c.name, value: () => c.content })),
        ],
        { title: t('commands.card.newFromTemplate') },
      );
      if (typeof tpl !== 'function') return;
      const tg = newCardTarget(b) ?? { parent: { kind: 'root' } as Parent, before: null };
      const id = await createCard(b.id, tg.parent, tg.before, tpl());
      if (id) await openCard(b.id, id);
    },
  },
  {
    id: 'lane.new',
    title: 'commands.lane.new',
    category: 'lane',
    icon: Columns3,
    when: "boardType == 'kanban'",
    run: () => {
      const b = focusBoard();
      if (!b || b.header.readOnly) return;
      const i = selection.lane ? b.lanes.findIndex((l) => l.id === selection.lane) : -1;
      boardUi.newLane = { boardId: b.id, index: i >= 0 ? i + 1 : null };
    },
  },
  { id: 'nav.up', title: 'commands.nav.up', category: 'navigation', icon: ArrowUp, hidden: true, run: () => navigate('up') },
  { id: 'nav.down', title: 'commands.nav.down', category: 'navigation', icon: ArrowDown, hidden: true, run: () => navigate('down') },
  { id: 'nav.left', title: 'commands.nav.left', category: 'navigation', icon: ArrowLeft, hidden: true, run: () => navigate('left') },
  { id: 'nav.right', title: 'commands.nav.right', category: 'navigation', icon: ArrowRight, hidden: true, run: () => navigate('right') },
  { id: 'nav.extendUp', title: 'commands.nav.extendUp', category: 'navigation', hidden: true, run: () => navigate('up', true) },
  { id: 'nav.extendDown', title: 'commands.nav.extendDown', category: 'navigation', hidden: true, run: () => navigate('down', true) },
  {
    id: 'nav.first',
    title: 'commands.nav.first',
    category: 'navigation',
    hidden: true,
    run: () => {
      const b = focusBoard();
      const c = visibleCards()[0];
      if (b && c) select(b.id, c.dataset.card!);
    },
  },
  {
    id: 'nav.last',
    title: 'commands.nav.last',
    category: 'navigation',
    hidden: true,
    run: () => {
      const b = focusBoard();
      const cs = visibleCards();
      if (b && cs.length) select(b.id, cs[cs.length - 1].dataset.card!);
    },
  },
  {
    id: 'board.selectAll',
    title: 'commands.board.selectAll',
    category: 'board',
    icon: SquareDashedMousePointer,
    when: ON_BOARD,
    run: () => {
      const b = focusBoard();
      if (!b) return;
      const laneId = selection.focus ? b.laneOf(selection.focus) : selection.lane;
      const ids = visibleCards()
        .filter((el) => el.dataset.group !== '1' && (!laneId || el.closest('[data-lane]')?.getAttribute('data-lane') === laneId))
        .map((el) => el.dataset.card!);
      selectMany(b.id, ids);
    },
  },
  { id: 'board.clearSelection', title: 'commands.board.clearSelection', category: 'board', hidden: true, run: () => clearSelection() },
  {
    id: 'card.open',
    title: 'commands.card.open',
    category: 'card',
    icon: ExternalLink,
    run: () => {
      const c = activeCard();
      if (c) void openCard(c.board.id, c.id);
    },
  },
  { id: 'card.peek', title: 'commands.card.peek', category: 'card', hidden: true, run: () => { const c = activeCard(); if (c) openEditor(c.board.id, c.id); } },
  { id: 'card.moveUp', title: 'commands.card.moveUp', category: 'card', icon: ArrowUp, run: () => moveWithin(-1) },
  { id: 'card.moveDown', title: 'commands.card.moveDown', category: 'card', icon: ArrowDown, run: () => moveWithin(1) },
  { id: 'card.moveLeft', title: 'commands.card.moveLeft', category: 'card', icon: ArrowLeft, when: "boardType == 'kanban'", run: () => moveLane(-1) },
  { id: 'card.moveRight', title: 'commands.card.moveRight', category: 'card', icon: ArrowRight, when: "boardType == 'kanban'", run: () => moveLane(1) },
  { id: 'card.indent', title: 'commands.card.indent', category: 'card', icon: Indent, run: indent },
  { id: 'card.outdent', title: 'commands.card.outdent', category: 'card', icon: Outdent, run: outdent },
  { id: 'card.moveTo', title: 'commands.card.moveTo', category: 'card', icon: ArrowRightLeft, run: moveTo },
  {
    id: 'card.delete',
    title: 'commands.card.delete',
    category: 'card',
    icon: Trash2,
    run: async () => {
      const tg = targets();
      if (tg && !tg.b.header.readOnly) await trashCards(tg.b.id, tg.ids);
    },
  },
  {
    id: 'card.archive',
    title: 'commands.card.archive',
    category: 'card',
    icon: Archive,
    run: async () => {
      const tg = targets();
      if (!tg || tg.b.header.readOnly) return;
      const allArchived = tg.ids.every((id) => tg.b.node(id)?.archived);
      await setArchived(tg.b.id, tg.ids, !allArchived);
    },
  },
  {
    id: 'card.duplicate',
    title: 'commands.card.duplicate',
    category: 'card',
    icon: Copy,
    run: async () => {
      const c = activeCard();
      if (!c) return;
      const id = await duplicateCard(c.board.id, c.id);
      if (id) {
        select(c.board.id, id);
        flash(id);
      }
    },
  },
  {
    id: 'card.rename',
    title: 'commands.card.rename',
    category: 'card',
    icon: Pencil,
    run: () => {
      const c = activeCard();
      if (c) boardUi.renamingCard = c.id;
    },
  },
  {
    id: 'card.copyLink',
    title: 'commands.card.copyLink',
    category: 'card',
    icon: Link,
    run: async () => {
      const tg = targets();
      if (!tg) return;
      await copyText(tg.ids.map((i) => `[[${i}]]`).join(' '));
      toast.info(t('toasts.linkCopied', { count: tg.ids.length }));
    },
  },
  {
    id: 'card.copyId',
    title: 'commands.card.copyId',
    category: 'card',
    run: async () => {
      const tg = targets();
      if (tg) await copyText(tg.ids.join(' '));
    },
  },
  {
    id: 'card.addTag',
    title: 'commands.card.addTag',
    category: 'card',
    icon: Tag,
    run: async () => {
      const tag = await pickTag();
      if (tag) await forEachTarget((b, id) => addTag(b.id, id, tag));
    },
  },
  {
    id: 'card.setDue',
    title: 'commands.card.setDue',
    category: 'card',
    icon: CalendarDays,
    run: async () => {
      const d = await pickDate(t('cards.setDue'));
      if (d !== undefined) await forEachTarget((b, id) => setFooterField(b.id, id, 'due', d ?? ''));
    },
  },
  {
    id: 'card.setPriority',
    title: 'commands.card.setPriority',
    category: 'card',
    icon: Flag,
    run: async () => {
      const p = await pickOne(
        ['urgent', 'high', 'medium', 'low', 'none'].map((x) => ({ label: t(`priority.${x}`), value: x })),
        { title: t('cards.setPriority') },
      );
      if (typeof p === 'string') await forEachTarget((b, id) => setFooterField(b.id, id, 'priority', p === 'none' ? '' : p));
    },
  },
  {
    id: 'card.assign',
    title: 'commands.card.assign',
    category: 'card',
    icon: UserPlus,
    run: async () => {
      const people = await rpc<string[]>('search.people').catch(() => []);
      const who = await quickPick(
        people.map((p) => ({ label: `@${p}`, value: p })),
        { title: t('cards.assign'), allowCustom: (s) => (s.trim() ? { label: t('cards.assignNew', { name: s.trim().replace(/^@/, '') }), value: s.trim().replace(/^@/, '') } : null) },
      );
      if (typeof who !== 'string') return;
      await forEachTarget(async (b, id) => {
        const n = b.node(id);
        const list = [...new Set([...(n?.footer.assignees ?? []), who])];
        await setFooterField(b.id, id, 'assignees', list.map((x) => `@${x}`).join(', '));
      });
    },
  },
  {
    id: 'card.assignSelf',
    title: 'commands.card.assignSelf',
    category: 'card',
    icon: UserPlus,
    run: async () => {
      const me = await myName();
      if (!me) return;
      await forEachTarget(async (b, id) => {
        const n = b.node(id);
        const list = [...new Set([...(n?.footer.assignees ?? []), me])];
        await setFooterField(b.id, id, 'assignees', list.map((x) => `@${x}`).join(', '));
      });
    },
  },
  {
    id: 'card.setCover',
    title: 'commands.card.setCover',
    category: 'card',
    icon: Image,
    run: async () => {
      const c = activeCard();
      if (!c) return;
      const n = c.board.node(c.id);
      const imgs = n?.attachments.filter((a) => a.kind === 'image') ?? [];
      const mode = await pickOne(
        [
          { label: t('cards.coverSet'), value: 'cover' as const },
          { label: t('cards.coverThumb'), value: 'thumb' as const },
          { label: t('cards.coverNone'), value: 'none' as const },
        ],
        { title: t('cards.setCover'), step: 1, totalSteps: 2 },
      );
      if (typeof mode !== 'string') return;
      if (mode === 'none') return void apply(c.board.id, { op: 'setCover', id: c.id, cover: null }, t('cards.setCover'));
      if (!imgs.length) return toast.info(t('cards.noImages'));
      const file = imgs.length === 1 ? imgs[0].file : await pickOne(imgs.map((a) => ({ label: a.display, value: a.file })), { title: t('cards.pickImage'), step: 2, totalSteps: 2 });
      if (typeof file === 'string') await apply(c.board.id, { op: 'setCover', id: c.id, cover: { file, mode } }, t('cards.setCover'));
    },
  },
  {
    id: 'card.openPrevious',
    title: 'commands.card.openPrevious',
    category: 'card',
    hidden: true,
    run: () => stepEditor(-1),
  },
  { id: 'card.openNext', title: 'commands.card.openNext', category: 'card', hidden: true, run: () => stepEditor(1) },
  {
    id: 'card.back',
    title: 'commands.card.back',
    category: 'card',
    hidden: true,
    run: () => {
      const e = ui.editor;
      const prev = e.back.pop();
      if (!prev) return;
      e.fwd.push({ boardId: e.boardId, cardId: e.cardId });
      openEditor(prev.boardId, prev.cardId, false);
    },
  },
  {
    id: 'card.forward',
    title: 'commands.card.forward',
    category: 'card',
    hidden: true,
    run: () => {
      const e = ui.editor;
      const next = e.fwd.pop();
      if (!next) return;
      e.back.push({ boardId: e.boardId, cardId: e.cardId });
      openEditor(next.boardId, next.cardId, false);
    },
  },
];

function stepEditor(dir: 1 | -1) {
  const e = ui.editor;
  const b = boards.get(e.boardId);
  const n = b?.node(e.cardId);
  if (!b || !n) return;
  const sibs = b.childrenOf(n.parent).filter((id) => ui.showArchived || !b.node(id)?.archived);
  const next = sibs[sibs.indexOf(n.id) + dir];
  if (next) {
    select(b.id, next);
    openEditor(b.id, next);
  }
}

export { readCard };
