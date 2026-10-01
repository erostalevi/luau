import { LayoutGrid, FileText, FolderOpen, Plus, Pencil, Rows3, Archive, FolderSearch, Pin, EyeOff, Repeat, Diamond, Space } from '@lucide/svelte';
import type { Command } from '../registry.svelte';
import { rpc } from '$lib/backend/rpc';
import type { BoardSnapshot, Op, Parent } from '$lib/backend/types';
import { quickPick, inputBox, pickOne, steps, BACK, type QuickItem } from '$lib/quickinput/qi.svelte';
import { openBoardTab } from '$lib/state/workspace.svelte';
import { boards, openBoard, apply, newCardId, newLaneId } from '$lib/state/boards.svelte';
import { registry, updateRegistry, boardEntry } from '$lib/state/registry.svelte';
import { settings } from '$lib/settings/store.svelte';
import { ui } from '$lib/state/ui.svelte';
import { toast } from '$lib/state/toasts.svelte';
import { confirm } from '$lib/state/dialogs.svelte';
import { tabBoard, pickFolder, reveal, joinPath, baseName, pathExists } from '$lib/app/helpers';
import { BOARD_TEMPLATES, templateLanes } from '$lib/board/templates';
import { t } from '$lib/i18n/index.svelte';

export async function openFolderAsBoard() {
  const path = await pickFolder(t('boards.openFolder'));
  if (!path) return;
  const info = await pathExists(path);
  if (info.isBoard) {
    try {
      const snap = await rpc<BoardSnapshot>('board.open', { path });
      await openBoardTab(snap.header.id);
    } catch (e) {
      // A copy of a board that is already open (same id): offer a new id.
      if (!String((e as Error).message ?? '').includes('duplicate_board_id')) throw e;
      const ok = await confirm({ title: t('boards.duplicateTitle'), message: t('boards.duplicateMessage', { name: baseName(path) }), confirmLabel: t('commands.board.makeCopyUnique') });
      if (!ok) return;
      await rpc('board.reassignId', { path });
      const snap = await rpc<BoardSnapshot>('board.open', { path });
      await openBoardTab(snap.header.id);
    }
    return;
  }
  const choice = await pickOne(
    [
      { label: t('boards.makeBoardHere'), description: t('boards.makeBoardHereDesc'), value: 'create' },
      { label: t('import.fromFolder'), description: t('import.fromFolderDesc'), value: 'import' },
    ],
    { title: t('boards.notABoard', { name: baseName(path) }) },
  );
  if (choice === 'create') await newBoardFlow(path);
  else if (choice === 'import') {
    const { runCommand } = await import('../registry.svelte');
    await runCommand('board.import', { path });
  }
}

export async function newBoardFlow(presetPath?: string) {
  const res = await steps<[string, string, string, string]>([
    () =>
      pickOne(
        [
          { label: t('boards.kanban'), description: t('boards.kanbanDesc'), value: 'kanban', icon: LayoutGrid },
          { label: t('boards.files'), description: t('boards.filesDesc'), value: 'files', icon: FileText },
        ],
        { title: t('boards.new'), step: 1, totalSteps: 4 },
      ),
    ([kind]) =>
      kind === 'files'
        ? Promise.resolve('notes')
        : pickOne(
            BOARD_TEMPLATES.filter((x) => x.kind === 'kanban').map((tpl) => ({
              label: t(`templates.boards.${tpl.id}`),
              description: templateLanes(tpl).join(' · ') || t('templates.noLanes'),
              value: tpl.id,
            })),
            { title: t('boards.pickTemplate'), step: 2, totalSteps: 4 },
          ),
    async () => {
      if (presetPath) return presetPath;
      const p = await pickFolder(t('boards.pickLocation'));
      return p ?? BACK;
    },
    ([, , folder]) =>
      inputBox({
        title: t('boards.name'),
        step: 4,
        totalSteps: 4,
        value: baseName(folder),
        prompt: t('boards.namePrompt'),
        validate: (v) => (v.trim() ? null : t('validation.required')),
      }),
  ]);
  if (!res) return;
  const [kind, tplId, folder, name] = res;
  const tpl = BOARD_TEMPLATES.find((x) => x.id === tplId)!;
  const info = await pathExists(folder);
  // Create a subfolder named after the board unless the folder is empty/new.
  const target = info.exists && !presetPath ? joinPath(folder, name.trim()) : folder;
  try {
    const snap = await rpc<BoardSnapshot>('board.create', {
      path: target,
      name: name.trim(),
      kind,
      lanes: templateLanes(tpl),
      git: settings.get<boolean>('files.gitInit'),
    });
    await openBoardTab(snap.header.id);
    toast.success(t('boards.created', { name: snap.header.name }));
  } catch (e) {
    const msg = (e as Error).message;
    toast.error(msg.includes('nested_board') ? t('errors.nestedBoard') : t('errors.createBoard', { message: msg }));
  }
}

async function boardPicker() {
  const items: QuickItem<() => unknown>[] = [
    { label: t('boards.new'), icon: Plus, value: () => newBoardFlow(), keybinding: null },
    { label: t('boards.openFolder'), icon: FolderOpen, value: openFolderAsBoard },
    { kind: 'separator', label: t('palette.boards') },
    ...registry.data.boards
      .filter((b) => !b.hidden && !b.missing)
      .sort((a, b) => (b.lastOpened ?? 0) - (a.lastOpened ?? 0))
      .map((b) => ({
        label: b.name,
        description: b.mirror ? t('palette.mirror') : b.path,
        icon: b.mirror ? Diamond : b.kind === 'files' ? FileText : LayoutGrid,
        value: () => openBoardTab(b.id),
      })),
  ];
  const r = await quickPick(items, { placeholder: t('boards.pickerPlaceholder'), matchOnDescription: true });
  if (typeof r === 'function') await r();
}

async function convertBoard() {
  const b = tabBoard();
  if (!b) return;
  const toFiles = b.kind === 'kanban';
  const ok = await confirm({
    title: t(toFiles ? 'boards.convertToFilesTitle' : 'boards.convertToKanbanTitle'),
    message: t(toFiles ? 'boards.convertToFilesMsg' : 'boards.convertToKanbanMsg'),
    confirmLabel: t('boards.convert'),
  });
  if (!ok) return;
  const ops: Op[] = [];
  if (toFiles) {
    ops.push({ op: 'setKind', kind: 'files' });
    for (const lane of b.lanes) {
      if (lane.order.length) {
        const id = await newCardId();
        ops.push({ op: 'createCard', id, parent: { kind: 'root' }, index: null, content: `# ${lane.name}\n` });
        ops.push({ op: 'move', ids: [...lane.order], to: { kind: 'card', id }, before: null });
      }
      ops.push({ op: 'trash', nodes: [], lanes: [lane.id] });
    }
  } else {
    const k = await newLaneId(b.id);
    ops.push({ op: 'setKind', kind: 'kanban' });
    ops.push({ op: 'createLane', id: k, name: t('templates.lanes.inbox'), index: null });
    if (b.rootOrder.length) ops.push({ op: 'move', ids: [...b.rootOrder], to: { kind: 'lane', id: k } as Parent, before: null });
  }
  await apply(b.id, { op: 'batch', ops }, t('boards.convert'));
}

export const boardCommands: Command[] = [
  { id: 'board.openPicker', title: 'commands.board.openPicker', category: 'board', icon: FolderOpen, run: boardPicker },
  { id: 'board.open', title: 'commands.board.open', category: 'board', icon: FolderOpen, run: openFolderAsBoard },
  { id: 'board.openById', title: 'commands.board.openPicker', category: 'board', hidden: true, run: (id?: string) => (id ? openBoardTab(id) : boardPicker()) },
  { id: 'board.new', title: 'commands.board.new', category: 'board', icon: Plus, run: () => newBoardFlow() },
  {
    id: 'board.rename',
    title: 'commands.board.rename',
    category: 'board',
    icon: Pencil,
    when: "tabKind == 'board'",
    run: async () => {
      const b = tabBoard();
      if (!b) return;
      const name = await inputBox({ title: t('commands.board.rename'), value: b.header.name, validate: (v) => (v.trim() ? null : t('validation.required')) });
      if (typeof name === 'string') await apply(b.id, { op: 'updateBoard', patch: { name: name.trim() } }, t('commands.board.rename'));
    },
  },
  { id: 'board.convert', title: 'commands.board.convert', category: 'board', icon: Repeat, when: "tabKind == 'board'", run: convertBoard },
  {
    id: 'board.toggleOrientation',
    title: 'commands.board.toggleOrientation',
    category: 'board',
    icon: Rows3,
    when: "tabKind == 'board' && boardType == 'kanban'",
    run: async () => {
      const b = tabBoard();
      if (!b) return;
      const view = { ...b.header.view, orientation: b.header.view.orientation === 'columns' ? 'rows' : 'columns' } as const;
      b.header = { ...b.header, view };
      await apply(b.id, { op: 'updateBoard', patch: { view } }, t('commands.board.toggleOrientation'));
    },
  },
  {
    id: 'board.toggleSpacing',
    title: 'commands.board.toggleSpacing',
    category: 'board',
    icon: Space,
    when: "tabKind == 'board' && boardType == 'kanban'",
    run: async () => {
      const b = tabBoard();
      if (!b) return;
      const view = { ...b.header.view, spacing: b.header.view.spacing === 'fixedMain' ? 'fixedCross' : 'fixedMain' } as const;
      b.header = { ...b.header, view };
      await apply(b.id, { op: 'updateBoard', patch: { view } }, t('commands.board.toggleSpacing'));
      toast.info(t(view.spacing === 'fixedMain' ? 'toasts.spacingMain' : 'toasts.spacingCross'));
    },
  },
  {
    id: 'board.toggleArchived',
    title: 'commands.board.toggleArchived',
    category: 'board',
    icon: Archive,
    run: () => {
      ui.showArchived = !ui.showArchived;
      toast.info(t(ui.showArchived ? 'toasts.archivedShown' : 'toasts.archivedHidden'));
    },
  },
  {
    id: 'board.filterMine',
    title: 'commands.board.filterMine',
    category: 'board',
    icon: FolderSearch,
    when: "tabKind == 'board'",
    run: async () => {
      const b = tabBoard();
      if (!b) return;
      const { myName } = await import('$lib/board/commands');
      const me = await myName();
      if (!me) return;
      ui.filter[b.id] = { ...(ui.filter[b.id] ?? { open: false, text: '' }), open: true, text: `@${me}` };
    },
  },
  {
    id: 'board.filter',
    title: 'commands.board.filter',
    category: 'board',
    icon: FolderSearch,
    when: "tabKind == 'board'",
    run: () => {
      const b = tabBoard();
      if (!b) return;
      const f = ui.filter[b.id] ?? { open: false, text: '' };
      ui.filter[b.id] = { ...f, open: true };
      queueMicrotask(() => document.querySelector<HTMLInputElement>('.board-filter input')?.focus());
    },
  },
  {
    id: 'board.reveal',
    title: 'commands.board.reveal',
    category: 'board',
    icon: FolderSearch,
    when: "tabKind == 'board'",
    run: () => {
      const b = tabBoard();
      if (b) void reveal(b.header.root);
    },
  },
  {
    id: 'board.pin',
    title: 'commands.board.pin',
    category: 'explorer',
    icon: Pin,
    run: async (id?: string) => {
      const bid = id ?? tabBoard()?.id;
      if (!bid) return;
      await updateRegistry({ id: bid, pinned: !boardEntry(bid)?.pinned });
    },
  },
  {
    id: 'board.hide',
    title: 'commands.board.hide',
    category: 'explorer',
    icon: EyeOff,
    run: async (id?: string) => {
      const bid = id ?? tabBoard()?.id;
      if (!bid) return;
      const hidden = !boardEntry(bid)?.hidden;
      await updateRegistry({ id: bid, hidden });
      toast.info(t(hidden ? 'toasts.boardHidden' : 'toasts.boardShown'), {
        action: hidden ? { label: t('common.undo'), run: () => void updateRegistry({ id: bid, hidden: false }) } : undefined,
      });
    },
  },
  {
    id: 'board.rescan',
    title: 'commands.board.rescan',
    category: 'explorer',
    run: async () => {
      await rpc('discovery.rescan');
      toast.info(t('toasts.rescanning'));
    },
  },
  {
    id: 'board.makeCopyUnique',
    title: 'commands.board.makeCopyUnique',
    category: 'explorer',
    hidden: true,
    run: async (path?: string) => {
      if (!path) return;
      await rpc('board.reassignId', { path });
      toast.success(t('toasts.copyUnique'));
    },
  },
];

export { openBoard, boards };
