// Card-level actions shared by the board, explorer, editor and commands.

import { rpc } from '$lib/backend/rpc';
import type { Parent } from '$lib/backend/types';
import { apply, boards, newCardId } from '$lib/state/boards.svelte';
import { toggleTaskLine, setFooterFields, parseFooter, setTitle } from '$lib/markdown/meta';
import { toast } from '$lib/state/toasts.svelte';
import { runCommand } from '$lib/commands/registry.svelte';
import { t } from '$lib/i18n/index.svelte';
import { flash } from './boardUi.svelte';

export async function readCard(boardId: string, id: string): Promise<string> {
  return rpc<string>('card.read', { board: boardId, id });
}

export async function writeCard(boardId: string, id: string, content: string, label = t('ops.editCard')) {
  return apply(boardId, { op: 'writeCard', id, content }, label);
}

export async function toggleTask(boardId: string, id: string, line: number) {
  const content = await readCard(boardId, id);
  const next = toggleTaskLine(content, line);
  if (next !== null) await writeCard(boardId, id, next, t('ops.toggleTask'));
}

export async function createCard(boardId: string, parent: Parent, before: string | null, content: string): Promise<string | null> {
  const b = boards.get(boardId);
  if (!b) return null;
  const id = await newCardId();
  const siblings = b.childrenOf(parent);
  const index = before ? Math.max(0, siblings.indexOf(before)) : null;
  const r = await apply(boardId, { op: 'createCard', id, parent, index, content }, t('ops.createCard'));
  if (r) flash(id);
  return r ? id : null;
}

export async function trashCards(boardId: string, ids: string[]) {
  if (!ids.length) return;
  const r = await apply(boardId, { op: 'trash', nodes: ids, lanes: [] }, ids.length > 1 ? t('ops.deleteCards', { count: ids.length }) : t('ops.deleteCard'));
  if (r) toast.info(t('toasts.movedToTrash', { count: ids.length }), { action: { label: t('common.undo'), run: () => void runCommand('edit.undo') } });
}

export async function setArchived(boardId: string, ids: string[], archived: boolean) {
  const r = await apply(boardId, { op: 'setArchived', nodes: ids.map((i) => [i, archived] as [string, boolean]), lanes: [] }, archived ? t('ops.archive') : t('ops.unarchive'));
  if (r && archived) toast.info(t('toasts.archived', { count: ids.length }), { action: { label: t('common.undo'), run: () => void runCommand('edit.undo') } });
}

export async function duplicateCard(boardId: string, id: string): Promise<string | null> {
  const b = boards.get(boardId);
  const n = b?.node(id);
  if (!b || !n) return null;
  const content = await readCard(boardId, id);
  const siblings = b.childrenOf(n.parent);
  const next = siblings[siblings.indexOf(id) + 1] ?? null;
  const copy = n.hasTitleLine ? setTitle(content, `${n.title} ${t('cards.copySuffix')}`) : content;
  return createCard(boardId, n.parent, next, copy);
}

export async function renameCard(boardId: string, id: string, title: string) {
  const content = await readCard(boardId, id);
  await writeCard(boardId, id, setTitle(content, title), t('ops.renameCard'));
}

/** Set (or clear with '') a footer property like priority/due/assignees/labels. */
export async function setFooterField(boardId: string, id: string, key: string, value: string) {
  const content = await readCard(boardId, id);
  const lines = content.replace(/\r\n/g, '\n').split('\n');
  const f = parseFooter(lines);
  const fields: [string, string][] = f.fields.filter(([k]) => k !== key);
  if (value.trim()) {
    const i = f.fields.findIndex(([k]) => k === key);
    if (i >= 0) fields.splice(i, 0, [key, value]);
    else fields.push([key, value]);
  }
  await writeCard(boardId, id, setFooterFields(content, fields), t('ops.setProperty', { key }));
}

/** Append `#tag` to the card (after the body, before the footer). */
export async function addTag(boardId: string, id: string, tag: string) {
  const clean = tag.replace(/^#/, '').trim().replace(/\s+/g, '-');
  if (!clean) return;
  const content = await readCard(boardId, id);
  const lines = content.replace(/\r\n/g, '\n').split('\n');
  const f = parseFooter(lines);
  const end = f.startLine ?? lines.length;
  const body = lines.slice(0, end);
  while (body.length && body[body.length - 1].trim() === '') body.pop();
  const lastLine = body[body.length - 1] ?? '';
  const onlyTags = /^(\s*#[\p{L}\p{N}_/-]+)+\s*$/u.test(lastLine) && body.length > 1;
  if (onlyTags) body[body.length - 1] = `${lastLine.trimEnd()} #${clean}`;
  else body.push('', `#${clean}`);
  const footer = f.startLine !== null ? lines.slice(f.startLine) : [];
  const next = [...body, ...(footer.length ? ['', ...footer] : [''])].join('\n');
  await writeCard(boardId, id, next, t('ops.addTag'));
}

/** Move group children out next to the group, dissolving it. */
export async function ungroup(boardId: string, id: string) {
  const b = boards.get(boardId);
  const n = b?.node(id);
  if (!b || !n || !n.children.length) return;
  const siblings = b.childrenOf(n.parent);
  const next = siblings[siblings.indexOf(id) + 1] ?? null;
  await apply(boardId, { op: 'move', ids: [...n.children], to: n.parent, before: next }, t('ops.ungroup'));
}
