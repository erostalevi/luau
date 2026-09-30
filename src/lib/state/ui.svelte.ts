// Window-level UI state: left panel, editor surface, per-board view state.

import { uiGet, uiSet } from './persist.svelte';

export type PanelSection = 'explorer' | 'search' | 'history' | 'integrations' | 'extensions';

export const ui = $state({
  left: { visible: true, section: 'explorer' as PanelSection, width: 280 },
  /** Card editor (kanban cards open in a modal or right sidebar). */
  editor: { open: false, boardId: '', cardId: '', back: [] as { boardId: string; cardId: string }[], fwd: [] as { boardId: string; cardId: string }[] },
  /** Board quick filter per board. */
  filter: {} as Record<string, { open: boolean; text: string }>,
  zoom: {} as Record<string, number>,
  showArchived: false,
  cheatsheet: false,
  firstRun: false,
  /** Focus request token for the search panel input. */
  searchFocus: 0,
  searchQuery: '',
});

export function loadUi() {
  const l = uiGet<Partial<typeof ui.left>>('left', {});
  Object.assign(ui.left, l);
  ui.zoom = uiGet('zoom', {});
}

export function persistUi() {
  uiSet('left', { ...ui.left });
  uiSet('zoom', { ...ui.zoom });
}

export function showSection(s: PanelSection) {
  if (ui.left.visible && ui.left.section === s) {
    ui.left.visible = false;
  } else {
    ui.left.section = s;
    ui.left.visible = true;
  }
  persistUi();
}

export function openEditor(boardId: string, cardId: string, pushHistory = true) {
  const e = ui.editor;
  if (pushHistory && e.open && e.cardId && (e.cardId !== cardId || e.boardId !== boardId)) {
    e.back.push({ boardId: e.boardId, cardId: e.cardId });
    if (e.back.length > 50) e.back.shift();
    e.fwd = [];
  }
  e.open = true;
  e.boardId = boardId;
  e.cardId = cardId;
}

export function closeEditor() {
  ui.editor.open = false;
}

export function boardZoom(id: string): number {
  return ui.zoom[id] ?? 1;
}

export function setBoardZoom(id: string, z: number) {
  ui.zoom[id] = Math.round(Math.min(2, Math.max(0.4, z)) * 100) / 100;
  persistUi();
}
