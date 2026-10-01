// Explorer actions shared by the tree (clicks, keys, context menus), the
// explorer commands and other panels (search "reveal in explorer").

import {
  ExternalLink,
  PanelRight,
  Pencil,
  Pin,
  PinOff,
  EyeOff,
  Eye,
  FolderSearch,
  Plus,
  Columns3,
  Archive,
  ArchiveRestore,
  Trash2,
  FolderInput,
  X,
  ChevronsDownUp,
  RefreshCw,
  FilePlus2,
  Fingerprint,
} from '@lucide/svelte';
import type { BoardEntry, BoardSnapshot, Parent } from '$lib/backend/types';
import { rpc } from '$lib/backend/rpc';
import { boards, openBoard, apply, newLaneId, type BoardModel } from '$lib/state/boards.svelte';
import { registry, boardEntry, sortedBoards, updateRegistry } from '$lib/state/registry.svelte';
import { ws, activePane, openBoardTab, openDocTab, persistWorkspace, type Tab } from '$lib/state/workspace.svelte';
import { ui, persistUi } from '$lib/state/ui.svelte';
import { selection } from '$lib/state/selection.svelte';
import type { MenuItem } from '$lib/state/menu.svelte';
import { inputBox } from '$lib/quickinput/qi.svelte';
import { confirm } from '$lib/state/dialogs.svelte';
import { toast } from '$lib/state/toasts.svelte';
import { settings } from '$lib/settings/store.svelte';
import { runCommand } from '$lib/commands/registry.svelte';
import { openCard } from '$lib/app/open';
import { pickFolder, reveal } from '$lib/app/helpers';
import { cardMenu } from '$lib/board/cardMenu';
import { createCard, renameCard, setArchived, trashCards } from '$lib/board/cardActions';
import { dnd, onExternalDrop, type DragSource, type DropTarget } from '$lib/board/dnd.svelte';
import { t } from '$lib/i18n/index.svelte';
import { ancestorKeys, boardKey, cardKey, laneKey, type Row } from './flatten';
import { explorer, setExpanded, setSectionOpen, sectionOf, collapseAll } from './explorerState.svelte';
import { reorderPinned } from './nav';

/** Rows currently rendered by the tree (set by Explorer.svelte). */
export const tree: { rows: Row[] } = { rows: [] };

export function rowByKey(key: string): Row | undefined {
  return tree.rows.find((r) => r.key === key);
}

export function focusedRow(): Row | undefined {
  return rowByKey(explorer.focusKey);
}

const isReadOnly = (boardId: string) => !!boards.get(boardId)?.header.readOnly || !!boardEntry(boardId)?.mirror;

// --- opening ---------------------------------------------------------------

/** Run `open` in a new pane to the right; the pane is dropped if nothing lands in it. */
async function inNewPane(open: (paneId: string) => Promise<Tab | null>) {
  const cur = activePane();
  const pane = { id: 'p' + Math.random().toString(36).slice(2, 9), tabs: [] as Tab[], active: null as string | null, mru: [] as string[] };
  ws.panes.splice(ws.panes.findIndex((p) => p.id === cur.id) + 1, 0, pane);
  await open(pane.id);
  const live = ws.panes.find((p) => p.id === pane.id);
  if (live && !live.tabs.length) {
    ws.panes = ws.panes.filter((p) => p.id !== pane.id);
    if (ws.activePane === pane.id) ws.activePane = cur.id;
  }
  persistWorkspace();
}

export async function openRow(row: Row, side = false) {
  const e = row.entry;
  if (row.type === 'section') {
    setSectionOpen(row.section!, !row.expanded);
    return;
  }
  if (row.type === 'board') {
    if (e?.missing) return;
    if (side) await inNewPane((p) => openBoardTab(row.boardId, p));
    else await openBoardTab(row.boardId);
    return;
  }
  if (row.type === 'lane') {
    await openBoardTab(row.boardId);
    selection.boardId = row.boardId;
    selection.ids = [];
    selection.lane = row.id;
    return;
  }
  if (row.type === 'card') {
    if (row.kind === 'files') {
      if (side) await inNewPane((p) => openDocTab(row.boardId, row.id, p));
      else await openDocTab(row.boardId, row.id);
    } else await openCard(row.boardId, row.id);
  }
}

// --- reveal ----------------------------------------------------------------

function showExplorer() {
  if (!ui.left.visible || ui.left.section !== 'explorer') {
    ui.left.visible = true;
    ui.left.section = 'explorer';
    persistUi();
  }
}

/** Expand the path to a card (loading its board) and focus it in the tree. */
export async function revealCard(boardId: string, cardId?: string) {
  const m = boards.get(boardId) ?? (await openBoard({ id: boardId }).catch(() => null));
  setSectionOpen(sectionOf(boardId), true);
  if (m && cardId && m.node(cardId)) {
    for (const k of ancestorKeys(boardId, cardId, m)) setExpanded(k, true);
    explorer.focusKey = cardKey(boardId, cardId);
  } else explorer.focusKey = boardKey(boardId);
  showExplorer();
  explorer.revealTick++;
}

// --- rename ----------------------------------------------------------------

export function canRename(row: Row): boolean {
  if (row.type === 'board') return !row.entry?.missing && !row.mirror && !boards.get(row.boardId)?.header.readOnly;
  if (row.type === 'lane' || row.type === 'card') return !isReadOnly(row.boardId);
  return false;
}

export function startRename(row: Row | undefined) {
  if (!row || !canRename(row)) return;
  explorer.focusKey = row.key;
  explorer.renaming = row.key;
}

export async function commitRename(row: Row, value: string) {
  explorer.renaming = '';
  const v = value.trim();
  if (!v || v === row.label) return;
  if (row.type === 'board') {
    const m = boards.get(row.boardId) ?? (await openBoard({ id: row.boardId }));
    m.header = { ...m.header, name: v };
    await apply(row.boardId, { op: 'updateBoard', patch: { name: v } }, t('commands.board.rename'));
  } else if (row.type === 'lane') {
    await apply(row.boardId, { op: 'updateLane', id: row.id, patch: { name: v } }, t('ops.renameLane'));
  } else if (row.type === 'card') {
    await renameCard(row.boardId, row.id, v);
  }
}

// --- create ----------------------------------------------------------------

/** Where a "new card" lands for a row: board → first lane (or root), lane, card → child. */
function createTarget(row: Row, m: BoardModel): Parent | null {
  if (row.type === 'board') {
    if (m.kind === 'files') return { kind: 'root' };
    const lane = m.lanes.find((l) => !l.archived);
    return lane ? { kind: 'lane', id: lane.id } : null;
  }
  if (row.type === 'lane') return { kind: 'lane', id: row.id };
  if (row.type === 'card') return { kind: 'card', id: row.id };
  return null;
}

export async function newCardIn(row: Row) {
  if (isReadOnly(row.boardId)) return;
  const m = boards.get(row.boardId) ?? (await openBoard({ id: row.boardId }));
  const parent = createTarget(row, m);
  if (!parent) {
    toast.info(t('explorer.noLaneYet'));
    return;
  }
  const files = m.kind === 'files';
  const title = await inputBox({ title: files ? t('explorer.newDocTitle') : t('explorer.newCardTitle'), placeholder: t('explorer.titlePlaceholder') });
  if (typeof title !== 'string') return;
  const id = await createCard(m.id, parent, null, `# ${title.trim()}\n`);
  if (!id) return;
  setExpanded(row.key, true);
  if (parent.kind === 'lane') setExpanded(laneKey(m.id, parent.id), true);
  setExpanded(boardKey(m.id), true);
  explorer.focusKey = cardKey(m.id, id);
  explorer.revealTick++;
  if (files) await openDocTab(m.id, id);
}

export async function newLaneIn(row: Row) {
  if (isReadOnly(row.boardId)) return;
  const m = boards.get(row.boardId) ?? (await openBoard({ id: row.boardId }));
  if (m.kind !== 'kanban') return;
  const name = await inputBox({ title: t('commands.lane.new'), placeholder: t('explorer.lanePlaceholder'), validate: (v) => (v.trim() ? null : t('validation.required')) });
  if (typeof name !== 'string') return;
  const id = await newLaneId(m.id);
  const r = await apply(m.id, { op: 'createLane', id, name: name.trim(), index: null }, t('ops.createLane'));
  if (!r) return;
  setExpanded(boardKey(m.id), true);
  explorer.focusKey = laneKey(m.id, id);
  explorer.revealTick++;
}

// --- archive / delete --------------------------------------------------------

export async function archiveRow(row: Row) {
  if (isReadOnly(row.boardId)) return;
  if (row.type === 'card') await setArchived(row.boardId, [row.id], !row.archived);
  else if (row.type === 'lane') {
    await apply(row.boardId, { op: 'setArchived', nodes: [], lanes: [[row.id, !row.archived]] }, row.archived ? t('ops.unarchive') : t('ops.archive'));
  }
}

export async function deleteRow(row: Row) {
  if (isReadOnly(row.boardId)) return;
  if (row.type === 'card') await trashCards(row.boardId, [row.id]);
  else if (row.type === 'lane') {
    const r = await apply(row.boardId, { op: 'trash', nodes: [], lanes: [row.id] }, t('ops.deleteLane'));
    if (r) toast.info(t('toasts.laneTrashed', { name: row.label }), { action: { label: t('common.undo'), run: () => void runCommand('edit.undo') } });
  } else if (row.type === 'board' && row.entry?.missing) await removeFromList(row.entry);
}

// --- boards: registry actions --------------------------------------------------

export async function togglePin(boardId: string) {
  await updateRegistry({ id: boardId, pinned: !boardEntry(boardId)?.pinned });
}

export async function toggleHidden(boardId: string) {
  await runCommand('board.hide', boardId);
}

export function toggleShowHidden() {
  const v = !settings.get<boolean>('explorer.showHidden');
  settings.set('explorer.showHidden', v);
  toast.info(t(v ? 'toasts.hiddenShown' : 'toasts.hiddenHidden'));
}

/** A missing board: point the registry at its new folder. */
export async function relocate(e: BoardEntry) {
  const path = await pickFolder(t('explorer.relocateTitle', { name: e.name }));
  if (!path) return;
  try {
    const snap = await rpc<BoardSnapshot>('board.open', { path });
    if (snap.header.id !== e.id) {
      toast.warn(t('explorer.relocateOther', { name: snap.header.name }));
      return;
    }
    toast.success(t('explorer.relocated', { name: e.name }));
  } catch (err) {
    toast.error(t('explorer.relocateFailed', { message: (err as Error).message }));
  }
}

export async function removeFromList(e: BoardEntry) {
  const ok = await confirm({ title: t('explorer.removeTitle', { name: e.name }), message: t('explorer.removeMsg'), confirmLabel: t('common.remove') });
  if (ok) await updateRegistry({ id: e.id, remove: true });
}

async function revealBoard(boardId: string) {
  const root = boards.get(boardId)?.header.root ?? boardEntry(boardId)?.path;
  if (root) await reveal(root);
}

// --- board reordering (drag within the pinned section) --------------------------

export const BOARD_DRAG = 'explorerBoard';

export function boardDragSource(e: BoardEntry): DragSource {
  return { kind: 'external', payload: { type: BOARD_DRAG, boardId: e.id }, label: e.name };
}

async function dropBoard(src: DragSource, tg: DropTarget) {
  const moving = (src.payload as { boardId?: string })?.boardId;
  const el = tg.el;
  if (!moving || tg.type !== 'tree' || el.dataset.tree !== 'board') return;
  const target = el.dataset.board!;
  const section = sectionOf(target);
  if (section !== sectionOf(moving) || moving === target) return;
  if (!boardEntry(target)?.pinned) {
    toast.info(t('explorer.reorderPinnedOnly'));
    return;
  }
  const r = el.getBoundingClientRect();
  const after = dnd.y > r.top + r.height / 2;
  const pinned = sortedBoards(section, true)
    .filter((b) => b.pinned)
    .map((b) => b.id);
  const key = section === 'boards' ? 'order' : 'mirrorOrder';
  const next = reorderPinned(registry.data[key], pinned.includes(moving) ? pinned : [...pinned, moving], moving, target, after);
  if (!boardEntry(moving)?.pinned) await updateRegistry({ id: moving, pinned: true });
  await updateRegistry({ [key]: next });
}

let dropRegistered = false;
export function registerExplorerDrop() {
  if (dropRegistered) return;
  dropRegistered = true;
  onExternalDrop(BOARD_DRAG, dropBoard);
}

// --- context menus -------------------------------------------------------------

export function rowMenu(row: Row): MenuItem[] {
  if (row.type === 'section') {
    return [
      { label: row.expanded ? t('explorer.collapseSection') : t('explorer.expandSection'), run: () => setSectionOpen(row.section!, !row.expanded) },
      { label: t('commands.explorer.collapseAll'), icon: ChevronsDownUp, run: collapseAll },
      { separator: true },
      { label: t('explorer.showHidden'), icon: Eye, checked: settings.get<boolean>('explorer.showHidden'), run: toggleShowHidden },
      { label: t('commands.board.rescan'), icon: RefreshCw, command: 'board.rescan' },
      { label: t('commands.board.new'), icon: Plus, command: 'board.new' },
    ];
  }
  if (row.type === 'board') return boardMenu(row);
  if (row.type === 'lane') return laneMenu(row);
  if (row.type === 'card') return cardRowMenu(row);
  if (row.type === 'hint' && row.path) {
    return [
      { label: t('commands.board.makeCopyUnique'), icon: Fingerprint, run: () => void runCommand('board.makeCopyUnique', row.path) },
      { label: t('cards.reveal'), icon: FolderSearch, run: () => void reveal(row.path!) },
    ];
  }
  return [];
}

function boardMenu(row: Row): MenuItem[] {
  const e = row.entry!;
  if (e.missing) {
    return [
      { label: t('explorer.relocate'), icon: FolderInput, run: () => void relocate(e) },
      { label: t('explorer.removeFromList'), icon: X, danger: true, run: () => void removeFromList(e) },
    ];
  }
  const ro = isReadOnly(row.boardId);
  const files = row.kind === 'files';
  return [
    { label: t('explorer.open'), icon: ExternalLink, run: () => void openRow(row) },
    { label: t('explorer.openToSide'), icon: PanelRight, run: () => void openRow(row, true) },
    { separator: true },
    { label: files ? t('explorer.newDoc') : t('explorer.newCard'), icon: files ? FilePlus2 : Plus, run: () => void newCardIn(row), disabled: ro },
    ...(files ? [] : [{ label: t('commands.lane.new'), icon: Columns3, run: () => void newLaneIn(row), disabled: ro }]),
    { separator: true },
    { label: t('explorer.rename'), icon: Pencil, run: () => startRename(row), disabled: !canRename(row) },
    { label: e.pinned ? t('explorer.unpin') : t('explorer.pin'), icon: e.pinned ? PinOff : Pin, run: () => void togglePin(e.id) },
    { label: e.hidden ? t('explorer.unhide') : t('explorer.hide'), icon: e.hidden ? Eye : EyeOff, run: () => void toggleHidden(e.id) },
    { label: t('explorer.revealInFinder'), icon: FolderSearch, run: () => void revealBoard(e.id) },
  ];
}

function laneMenu(row: Row): MenuItem[] {
  const ro = isReadOnly(row.boardId);
  return [
    { label: t('explorer.openBoard'), icon: ExternalLink, run: () => void openRow(row) },
    { separator: true },
    { label: t('explorer.newCard'), icon: Plus, run: () => void newCardIn(row), disabled: ro },
    { label: t('explorer.rename'), icon: Pencil, run: () => startRename(row), disabled: ro },
    { separator: true },
    row.archived
      ? { label: t('cards.unarchive'), icon: ArchiveRestore, run: () => void archiveRow(row), disabled: ro }
      : { label: t('cards.archive'), icon: Archive, run: () => void archiveRow(row), disabled: ro },
    { label: t('explorer.deleteLane'), icon: Trash2, danger: true, run: () => void deleteRow(row), disabled: ro },
  ];
}

function cardRowMenu(row: Row): MenuItem[] {
  const ro = isReadOnly(row.boardId);
  const files = row.kind === 'files';
  const base = cardMenu(row.boardId, row.id).filter((it) => it.label !== t('cards.open') && it.label !== t('cards.rename') && it.command !== 'card.newSubcard');
  return [
    { label: t('explorer.open'), icon: ExternalLink, run: () => void openRow(row) },
    ...(files ? [{ label: t('explorer.openToSide'), icon: PanelRight, run: () => void openRow(row, true) }] : []),
    { label: t('explorer.rename'), icon: Pencil, run: () => startRename(row), disabled: ro },
    { label: files ? t('explorer.newSubdoc') : t('cards.newSubcard'), icon: FilePlus2, run: () => void newCardIn(row), disabled: ro },
    { separator: true },
    ...base,
  ];
}
