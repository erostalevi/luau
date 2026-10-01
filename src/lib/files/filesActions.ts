// Files-board actions shared by FilesHome and the `files.*` commands.

import { ExternalLink, PanelRight, Pencil, FilePlus2, LocateFixed } from '@lucide/svelte';
import type { Parent } from '$lib/backend/types';
import type { MenuItem } from '$lib/state/menu.svelte';
import { boards, openBoard, type BoardModel } from '$lib/state/boards.svelte';
import { sortedBoards } from '$lib/state/registry.svelte';
import { openDocTab } from '$lib/state/workspace.svelte';
import { inputBox, quickPick } from '$lib/quickinput/qi.svelte';
import { toast } from '$lib/state/toasts.svelte';
import { tabBoard } from '$lib/app/helpers';
import { cardMenu } from '$lib/board/cardMenu';
import { createCard, renameCard } from '$lib/board/cardActions';
import { openRow, revealCard } from '$lib/panels/explorer/explorerActions';
import { cardKey } from '$lib/panels/explorer/flatten';
import { t } from '$lib/i18n/index.svelte';

const MAX_TITLE = 200;

/** The active files board, or one picked by the user. */
async function targetBoard(boardId?: string): Promise<BoardModel | null> {
  if (boardId) return boards.get(boardId) ?? (await openBoard({ id: boardId }).catch(() => null));
  const cur = tabBoard();
  if (cur?.kind === 'files') return cur;
  const list = sortedBoards('boards').filter((b) => b.kind === 'files' && !b.missing && !b.mirror);
  if (!list.length) {
    toast.info(t('filesHome.noFilesBoards'));
    return null;
  }
  const picked =
    list.length === 1
      ? list[0]
      : await quickPick(
          list.map((b) => ({ label: b.name, value: b })),
          { placeholder: t('filesHome.pickBoard') },
        );
  if (!picked || typeof picked !== 'object' || Array.isArray(picked) || !('id' in picked)) return null;
  return openBoard({ id: (picked as { id: string }).id }).catch(() => null);
}

/** Ask for a title, create the document (under `parent`, default root) and open it. */
export async function newDocument(boardId?: string, parent: Parent = { kind: 'root' }): Promise<string | null> {
  const m = await targetBoard(boardId);
  if (!m) return null;
  if (m.header.readOnly) {
    toast.info(t('filesHome.readOnly'));
    return null;
  }
  const title = await inputBox({
    title: t('explorer.newDocTitle'),
    placeholder: t('explorer.titlePlaceholder'),
    validate: (v) => (v.trim().length > MAX_TITLE ? t('filesHome.titleTooLong', { max: MAX_TITLE }) : null),
  });
  if (typeof title !== 'string') return null;
  // A single-line title: newlines would turn into extra Markdown blocks.
  const clean = title.replace(/[\r\n]+/g, ' ').trim();
  const id = await createCard(m.id, parent, null, clean ? `# ${clean}\n` : '');
  if (id) await openDocTab(m.id, id);
  return id;
}

export async function renameDocument(boardId: string, id: string) {
  const n = boards.get(boardId)?.node(id);
  if (!n) return;
  const title = await inputBox({
    title: t('explorer.rename'),
    value: n.title,
    validate: (v) => (v.trim() ? null : t('validation.required')),
  });
  if (typeof title === 'string') await renameCard(boardId, id, title.replace(/[\r\n]+/g, ' ').trim());
}

const docRow = (boardId: string, id: string, label: string) => ({
  key: cardKey(boardId, id),
  type: 'card' as const,
  depth: 0,
  parentKey: null,
  boardId,
  id,
  label,
  expandable: false,
  expanded: false,
  kind: 'files' as const,
});

export function docMenu(m: BoardModel, id: string): MenuItem[] {
  const ro = !!m.header.readOnly;
  const label = m.node(id)?.title ?? '';
  const base = cardMenu(m.id, id).filter((it) => it.label !== t('cards.open') && it.label !== t('cards.rename') && it.command !== 'card.newSubcard');
  return [
    {
      label: t('explorer.open'),
      icon: ExternalLink,
      run: () => void openDocTab(m.id, id),
    },
    {
      label: t('explorer.openToSide'),
      icon: PanelRight,
      run: () => void openRow(docRow(m.id, id, label), true),
    },
    {
      label: t('explorer.rename'),
      icon: Pencil,
      run: () => void renameDocument(m.id, id),
      disabled: ro,
    },
    {
      label: t('explorer.newSubdoc'),
      icon: FilePlus2,
      run: () => void newDocument(m.id, { kind: 'card', id }),
      disabled: ro,
    },
    {
      label: t('searchPanel.revealInExplorer'),
      icon: LocateFixed,
      run: () => void revealCard(m.id, id),
    },
    { separator: true },
    ...base,
  ];
}
