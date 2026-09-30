// Card selection within the active board (single, range, toggle, lasso).

import { ctx } from '$lib/commands/context.svelte';

export const selection = $state({
  boardId: '',
  ids: [] as string[],
  /** Keyboard focus (last clicked / navigated). */
  focus: '' as string,
  anchor: '' as string,
  /** Focused lane (keyboard nav on empty lanes). */
  lane: '' as string,
});

function sync() {
  ctx.cardSelected = selection.ids.length > 0;
  ctx.multiSelect = selection.ids.length > 1;
}

export function select(boardId: string, id: string, mode: 'replace' | 'toggle' | 'add' = 'replace') {
  if (selection.boardId !== boardId) {
    selection.boardId = boardId;
    selection.ids = [];
  }
  if (mode === 'replace') selection.ids = [id];
  else if (mode === 'toggle') selection.ids = selection.ids.includes(id) ? selection.ids.filter((x) => x !== id) : [...selection.ids, id];
  else if (!selection.ids.includes(id)) selection.ids = [...selection.ids, id];
  selection.focus = id;
  if (mode === 'replace') selection.anchor = id;
  sync();
}

export function selectMany(boardId: string, ids: string[], additive = false) {
  if (selection.boardId !== boardId) selection.ids = [];
  selection.boardId = boardId;
  selection.ids = additive ? [...new Set([...selection.ids, ...ids])] : ids;
  selection.focus = ids[ids.length - 1] ?? selection.focus;
  sync();
}

export function clearSelection() {
  selection.ids = [];
  sync();
}

export function isSelected(boardId: string, id: string): boolean {
  return selection.boardId === boardId && selection.ids.includes(id);
}
