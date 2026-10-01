// Pure helpers for the files-board home: which documents to show and in what order.

import type { NodeDto } from '$lib/backend/types';

export type DocSort = 'recent' | 'title';

export interface DocItem {
  id: string;
  title: string;
  mtime: number;
  /** Number of subdocuments (all descendants). */
  subdocs: number;
  /** Ancestor titles, root first. */
  path: string[];
  tags: string[];
  wordCount: number;
  archived: boolean;
}

interface DocSource {
  rootOrder: string[];
  node(id: string): NodeDto | undefined;
}

/** Every document in tree order with its ancestor path. */
export function collectDocs(m: DocSource, untitled = ''): DocItem[] {
  const out: DocItem[] = [];
  const seen = new Set<string>();
  const walk = (ids: string[], path: string[]): number => {
    let count = 0;
    for (const id of ids) {
      const n = m.node(id);
      if (!n || seen.has(id)) continue;
      seen.add(id);
      const title = n.title.trim() || untitled;
      const item: DocItem = {
        id,
        title,
        mtime: n.mtime,
        subdocs: 0,
        path,
        tags: n.tags,
        wordCount: n.wordCount,
        archived: n.archived,
      };
      out.push(item);
      item.subdocs = walk(n.children, [...path, title]);
      count += 1 + item.subdocs;
    }
    return count;
  };
  walk(m.rootOrder, []);
  return out;
}

function fold(s: string): string {
  return s.normalize('NFD').replace(/\p{M}/gu, '').toLowerCase();
}

/** Case/diacritic-insensitive match of every word on title, path and tags. */
export function matchesFilter(d: DocItem, filter: string): boolean {
  const f = fold(filter.trim());
  if (!f) return true;
  const hay = fold([d.title, ...d.path, ...d.tags.map((x) => `#${x}`)].join(' '));
  return f.split(/\s+/).every((w) => hay.includes(w));
}

/** Natural, case-insensitive title order ("Doc 2" before "Doc 10"). */
const byTitle = (a: DocItem, b: DocItem) => a.title.localeCompare(b.title, undefined, { sensitivity: 'base', numeric: true });

export function visibleDocs(
  all: DocItem[],
  opts: {
    filter: string;
    sort: DocSort;
    showArchived: boolean;
    limit?: number;
  },
): DocItem[] {
  const list = all.filter((d) => (opts.showArchived || !d.archived) && matchesFilter(d, opts.filter));
  list.sort((a, b) => (opts.sort === 'recent' ? b.mtime - a.mtime || byTitle(a, b) : byTitle(a, b)));
  return opts.limit ? list.slice(0, opts.limit) : list;
}
