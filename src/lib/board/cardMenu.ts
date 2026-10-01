// Context menu for cards (board view and explorer share it).

import {
  ExternalLink,
  Pencil,
  Copy,
  Link,
  Archive,
  ArchiveRestore,
  Trash2,
  FolderSearch,
  Tag,
  Calendar,
  Flag,
  ArrowRightLeft,
  Image,
  Ungroup,
  FilePlus2,
  Hash,
  Plug,
} from '@lucide/svelte';
import type { MenuItem } from '$lib/state/menu.svelte';
import { boards } from '$lib/state/boards.svelte';
import { select } from '$lib/state/selection.svelte';
import { selection } from '$lib/state/selection.svelte';
import { openCard } from '$lib/app/open';
import { copyText, reveal } from '$lib/app/helpers';
import { rpc } from '$lib/backend/rpc';
import { runCommand } from '$lib/commands/registry.svelte';
import { boardUi } from './boardUi.svelte';
import { trashCards, setArchived, duplicateCard, ungroup } from './cardActions';
import { contributions } from '$lib/contributions/registry.svelte';
import { t } from '$lib/i18n/index.svelte';

export function cardMenu(boardId: string, id: string): MenuItem[] {
  const b = boards.get(boardId);
  const n = b?.node(id);
  if (!b || !n) return [];
  if (!(selection.boardId === boardId && selection.ids.includes(id))) select(boardId, id);
  const ids = selection.ids.length > 1 && selection.ids.includes(id) ? [...selection.ids] : [id];
  const multi = ids.length > 1;
  const readOnly = !!b.header.readOnly;
  const remote = b.remote.get(id);
  const extra = contributions.cardActions
    .filter((a) => !a.when || a.when({ boardId, id, remote }))
    .map((a) => ({ label: a.label(), icon: a.icon, run: () => a.run({ boardId, id, remote }) }));
  const items: MenuItem[] = [
    { label: t('cards.open'), icon: ExternalLink, run: () => void openCard(boardId, id), disabled: multi },
    { label: t('cards.rename'), icon: Pencil, run: () => (boardUi.renamingCard = id), disabled: multi || readOnly, command: 'card.rename' },
    { separator: true },
    { label: t('cards.addTag'), icon: Tag, command: 'card.addTag', disabled: readOnly },
    { label: t('cards.setDue'), icon: Calendar, command: 'card.setDue', disabled: readOnly },
    { label: t('cards.setPriority'), icon: Flag, command: 'card.setPriority', disabled: readOnly },
    { label: t('cards.moveTo'), icon: ArrowRightLeft, command: 'card.moveTo', disabled: readOnly },
    {
      label: t('cards.setCover'),
      icon: Image,
      command: 'card.setCover',
      disabled: multi || readOnly || !n.attachments.some((a) => a.kind === 'image' || a.kind === 'pdf'),
    },
    { label: t('cards.newSubcard'), icon: FilePlus2, command: 'card.newSubcard', disabled: multi || readOnly },
    ...(n.isGroup ? [{ label: t('cards.ungroup'), icon: Ungroup, run: () => void ungroup(boardId, id), disabled: readOnly }] : []),
    { separator: true },
    { label: t('cards.duplicate'), icon: Copy, run: () => void duplicateCard(boardId, id), disabled: multi || readOnly, command: 'card.duplicate' },
    { label: t('cards.copyLink'), icon: Link, run: () => void copyText(ids.map((i) => `[[${i}]]`).join(' ')), command: 'card.copyLink' },
    { label: t('cards.copyId'), icon: Hash, run: () => void copyText(ids.join(' ')) },
    {
      label: t('cards.reveal'),
      icon: FolderSearch,
      disabled: multi,
      run: async () => {
        const r = await rpc<{ file: string | null }>('card.path', { board: boardId, id });
        if (r.file) await reveal(r.file);
      },
    },
    ...(extra.length ? [{ separator: true } as MenuItem, ...extra] : []),
    ...(remote ? [{ separator: true } as MenuItem, { label: t('remote.actions'), icon: Plug, command: 'remote.actions' }] : []),
    { separator: true },
    n.archived
      ? { label: t('cards.unarchive'), icon: ArchiveRestore, run: () => void setArchived(boardId, ids, false), disabled: readOnly }
      : { label: t('cards.archive'), icon: Archive, run: () => void setArchived(boardId, ids, true), disabled: readOnly, command: 'card.archive' },
    {
      label: multi ? t('cards.deleteMany', { count: ids.length }) : t('cards.delete'),
      icon: Trash2,
      danger: true,
      run: () => void trashCards(boardId, ids),
      disabled: readOnly,
      command: 'card.delete',
    },
  ];
  void runCommand;
  return items;
}
