// Search panel state, saved searches and the shared query runner (also used by
// saved-search virtual boards).

import { rpc } from '$lib/backend/rpc';
import type { SearchHit, SearchOptions } from '$lib/backend/types';
import { registry } from '$lib/state/registry.svelte';
import { uiGet, uiSet } from '$lib/state/persist.svelte';
import { parse, toText, type Query } from './query';

export * from './results';

export type GroupBy = 'board' | 'lane' | 'tag';

export interface SavedSearch {
  id: string;
  name: string;
  q: string;
  /** Lanes of the virtual board. */
  groupBy?: GroupBy;
}

export const search = $state({
  q: '',
  filtersOpen: false,
  limit: 200,
  saved: [] as SavedSearch[],
  savedLoaded: false,
  /** Result grouping in the panel. */
  groupBy: 'board' as Exclude<GroupBy, 'tag'>,
  /** Last `ui.searchFocus` token handled by the panel. */
  focusSeen: 0,
});

export function loadSaved() {
  if (search.savedLoaded) return;
  const v = uiGet<SavedSearch[]>('savedSearches', []);
  search.saved = Array.isArray(v) ? v.filter((s) => s && typeof s.q === 'string' && typeof s.name === 'string') : [];
  search.savedLoaded = true;
}

function persistSaved() {
  uiSet(
    'savedSearches',
    search.saved.map((s) => ({ ...s })),
  );
}

export function addSaved(name: string, q: string): SavedSearch {
  loadSaved();
  const s: SavedSearch = { id: Math.random().toString(36).slice(2, 10), name: name.trim(), q: q.trim() };
  search.saved = [...search.saved, s];
  persistSaved();
  return s;
}

export function renameSaved(id: string, name: string) {
  search.saved = search.saved.map((s) => (s.id === id ? { ...s, name: name.trim() } : s));
  persistSaved();
}

export function updateSaved(id: string, patch: Partial<Omit<SavedSearch, 'id'>>) {
  search.saved = search.saved.map((s) => (s.id === id ? { ...s, ...patch } : s));
  persistSaved();
}

export function removeSaved(id: string): SavedSearch | undefined {
  const s = search.saved.find((x) => x.id === id);
  search.saved = search.saved.filter((x) => x.id !== id);
  persistSaved();
  return s;
}

export function restoreSaved(s: SavedSearch, index: number) {
  const list = [...search.saved];
  list.splice(Math.min(index, list.length), 0, s);
  search.saved = list;
  persistSaved();
}

/** Registry board id for a `board:` value (exact id or exact human name). */
export function resolveBoard(value: string): string | null {
  const v = value.toLowerCase();
  const b = registry.data.boards.find((x) => x.id === value) ?? registry.data.boards.find((x) => x.name.toLowerCase() === v);
  return b?.id ?? null;
}

/**
 * Prepare a query for the backend. Positive `board:` filters that name a known
 * board are moved into `opts.boards`, so several boards mean "any of them"
 * (the backend ANDs repeated filters).
 */
export function prepare(text: string): { q: string; boards: string[]; parsed: Query } {
  const parsed = parse(text);
  const boards: string[] = [];
  const rest = parsed.filters.filter((f) => {
    if (f.key !== 'board' || f.negate || f.cmp !== 'eq') return true;
    const id = resolveBoard(f.value);
    if (!id) return true;
    boards.push(id);
    return false;
  });
  return { q: toText({ ...parsed, filters: rest }, { sugar: false }), boards, parsed };
}

export function hasContent(text: string): boolean {
  const p = parse(text);
  return p.terms.length > 0 || p.filters.length > 0;
}

export async function runSearch(text: string, opts: Omit<SearchOptions, 'boards'> = {}): Promise<SearchHit[]> {
  const { q, boards } = prepare(text);
  const o: SearchOptions = { ...opts };
  if (boards.length) o.boards = boards;
  // A query made only of board filters still lists those boards' cards.
  return rpc<SearchHit[]>('search.query', { q, opts: o });
}
