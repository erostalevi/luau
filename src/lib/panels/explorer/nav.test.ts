import { describe, it, expect } from 'vitest';
import type { Row } from './flatten';
import { step, edge, leftTarget, rightTarget, typeahead, reorderPinned } from './nav';

const row = (key: string, type: Row['type'], label: string, parentKey: string | null, extra: Partial<Row> = {}): Row => ({
  key,
  type,
  depth: 0,
  parentKey,
  boardId: 'b1',
  id: key,
  label,
  expandable: false,
  expanded: false,
  ...extra,
});

const rows: Row[] = [
  row('s:boards', 'section', 'Boards', null, {
    expandable: true,
    expanded: true,
  }),
  row('b:1', 'board', 'Álgebra', 's:boards', {
    expandable: true,
    expanded: true,
  }),
  row('l:1', 'lane', 'Doing', 'b:1', { expandable: true, expanded: false }),
  row('x', 'loading', '', 'b:1'),
  row('b:2', 'board', 'Alpha', 's:boards', { expandable: true }),
];

describe('explorer keyboard nav', () => {
  it('steps over placeholder rows and clamps', () => {
    expect(step(rows, 'l:1', 1)).toBe('b:2');
    expect(step(rows, 'b:2', 1)).toBe('b:2');
    expect(step(rows, 'nope', -1)).toBe('b:2');
    expect(edge(rows, false)).toBe('s:boards');
    expect(edge(rows, true)).toBe('b:2');
  });
  it('left collapses open rows or goes to the parent', () => {
    expect(leftTarget(rows, 'b:1')).toEqual({ collapse: 'b:1' });
    expect(leftTarget(rows, 'l:1')).toEqual({ focus: 'b:1' });
  });
  it('right expands closed rows or goes to the first child', () => {
    expect(rightTarget(rows, 'l:1')).toEqual({ expand: 'l:1' });
    expect(rightTarget(rows, 'b:1')).toEqual({ focus: 'l:1' });
    expect(rightTarget(rows, 'x')).toEqual({});
  });
  it('type-ahead is accent-insensitive and wraps', () => {
    expect(typeahead(rows, 'b:1', 'a')).toBe('b:2');
    expect(typeahead(rows, 'b:2', 'a')).toBe('b:1');
    expect(typeahead(rows, 'b:1', 'al')).toBe('b:1');
    expect(typeahead(rows, 'b:1', 'zz')).toBeNull();
  });
});

describe('reorderPinned', () => {
  it('moves a pinned board before/after another, keeping the rest', () => {
    expect(reorderPinned(['a', 'b', 'c', 'z'], ['a', 'b', 'c'], 'c', 'a', false)).toEqual(['c', 'a', 'b', 'z']);
    expect(reorderPinned(['a', 'b', 'c'], ['a', 'b', 'c'], 'a', 'b', true)).toEqual(['b', 'a', 'c']);
  });
  it('is a no-op for self or unknown targets', () => {
    expect(reorderPinned(['a', 'b'], ['a', 'b'], 'a', 'a', true)).toEqual(['a', 'b']);
    expect(reorderPinned(['a', 'b'], ['a', 'b'], 'a', 'q', true)).toEqual(['a', 'b']);
  });
});
