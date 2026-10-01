import { describe, it, expect } from 'vitest';
import type { NodeDto } from '$lib/backend/types';
import { collectDocs, matchesFilter, visibleDocs } from './filesHome';

const node = (id: string, title: string, mtime: number, children: string[] = [], extra: Partial<NodeDto> = {}) =>
  ({
    id,
    title,
    mtime,
    children,
    tags: [],
    wordCount: 0,
    archived: false,
    ...extra,
  }) as unknown as NodeDto;

const nodes = new Map<string, NodeDto>([
  ['c1', node('c1', 'Notas de diseño', 10, ['c2'])],
  ['c2', node('c2', 'Colors', 30, [], { tags: ['ui'] })],
  ['c3', node('c3', '', 20, [], { archived: true })],
]);
const m = {
  rootOrder: ['c1', 'c3', 'missing'],
  node: (id: string) => nodes.get(id),
};

describe('collectDocs', () => {
  it('walks the tree with paths and subdoc counts, skipping missing ids', () => {
    const d = collectDocs(m, 'Untitled');
    expect(d.map((x) => x.id)).toEqual(['c1', 'c2', 'c3']);
    expect(d[0].subdocs).toBe(1);
    expect(d[1].path).toEqual(['Notas de diseño']);
    expect(d[2].title).toBe('Untitled');
  });
});

describe('filter + sort', () => {
  const all = collectDocs(m, 'Untitled');
  it('matches title, ancestor path and tags, ignoring accents', () => {
    expect(matchesFilter(all[0], 'diseno')).toBe(true);
    expect(matchesFilter(all[1], 'notas colors')).toBe(true);
    expect(matchesFilter(all[1], '#ui')).toBe(true);
    expect(matchesFilter(all[1], 'nope')).toBe(false);
  });
  it('hides archived unless asked, sorts by recency or title, limits', () => {
    expect(visibleDocs(all, { filter: '', sort: 'recent', showArchived: false }).map((x) => x.id)).toEqual(['c2', 'c1']);
    expect(visibleDocs(all, { filter: '', sort: 'title', showArchived: true }).map((x) => x.id)).toEqual(['c2', 'c1', 'c3']);
    expect(
      visibleDocs(all, {
        filter: '',
        sort: 'recent',
        showArchived: true,
        limit: 1,
      }).map((x) => x.id),
    ).toEqual(['c2']);
  });
  it('sorts numbered titles naturally', () => {
    const docs = ['10 · Ten', '2 · Two', '1 · One'].map((title, i) => ({ id: `c${i}`, title, mtime: 1, archived: false }) as (typeof all)[number]);
    expect(visibleDocs(docs, { filter: '', sort: 'title', showArchived: false }).map((x) => x.title)).toEqual(['1 · One', '2 · Two', '10 · Ten']);
    expect(visibleDocs(docs, { filter: '', sort: 'recent', showArchived: false })[0].title).toBe('1 · One');
  });
});
