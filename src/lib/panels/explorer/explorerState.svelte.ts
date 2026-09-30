// Explorer state: expanded nodes (persisted across sessions), keyboard focus,
// inline rename and "reveal" requests from commands.

import { SvelteSet } from 'svelte/reactivity';
import { uiGet, uiSet, uiState } from '$lib/state/persist.svelte';
import { boards } from '$lib/state/boards.svelte';
import { boardEntry } from '$lib/state/registry.svelte';
import { ui, persistUi } from '$lib/state/ui.svelte';
import { activeCard, tabBoard } from '$lib/app/helpers';
import { ancestorKeys, boardKey, cardKey, sectionCollapsedKey, type Section } from './flatten';

const MAX_KEYS = 4000;

export const expanded = new SvelteSet<string>();

export const explorer = $state({
  /** Row with keyboard focus. */
  focusKey: '',
  /** Row being renamed inline. */
  renaming: '',
  /** Incremented to ask the tree to scroll `focusKey` into view. */
  revealTick: 0,
  loaded: false,
});

let timer: ReturnType<typeof setTimeout> | null = null;

export function loadExpanded() {
  if (explorer.loaded || !uiState.loaded) return;
  const saved = uiGet<unknown>('explorerExpanded', []);
  if (Array.isArray(saved)) for (const k of saved) if (typeof k === 'string') expanded.add(k);
  explorer.loaded = true;
}

function persist() {
  if (timer) clearTimeout(timer);
  timer = setTimeout(() => {
    timer = null;
    uiSet('explorerExpanded', [...expanded].slice(-MAX_KEYS));
  }, 300);
}

export function setExpanded(key: string, open: boolean) {
  if (open === expanded.has(key)) return;
  if (open) expanded.add(key);
  else expanded.delete(key);
  persist();
}

export function toggleExpanded(key: string) {
  setExpanded(key, !expanded.has(key));
}

export function setSectionOpen(s: Section, open: boolean) {
  setExpanded(sectionCollapsedKey(s), !open);
}

/** Collapse everything below the sections (sections keep their state). */
export function collapseAll() {
  for (const k of [...expanded]) if (!k.startsWith('s:')) expanded.delete(k);
  persist();
}

export function sectionOf(boardId: string): Section {
  const e = boardEntry(boardId);
  return ((e?.section ?? (e?.mirror ? 'mirrors' : 'boards')) === 'mirrors' ? 'mirrors' : 'boards') as Section;
}

/** Expand the path to the active card (or board) and focus it in the tree. */
export function revealActive(): boolean {
  const c = activeCard();
  const b = c?.board ?? tabBoard();
  if (!b) return false;
  setSectionOpen(sectionOf(b.id), true);
  if (c && boards.get(b.id)) {
    for (const k of ancestorKeys(b.id, c.id, b)) setExpanded(k, true);
    explorer.focusKey = cardKey(b.id, c.id);
  } else explorer.focusKey = boardKey(b.id);
  if (!ui.left.visible || ui.left.section !== 'explorer') {
    ui.left.visible = true;
    ui.left.section = 'explorer';
    persistUi();
  }
  explorer.revealTick++;
  return true;
}
