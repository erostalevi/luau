import { describe, it, expect } from 'vitest';
import type { SearchHit } from '$lib/backend/types';
import { explicitCase, effectiveCase, withCase, rewrite, effectiveText } from './sync';
import { parse, toText, toggleValue } from './query';
import { groupByTag, navOrder } from './results';

describe('case token sync', () => {
  it('reads the last explicit case token', () => {
    expect(explicitCase('foo')).toBeNull();
    expect(explicitCase('foo case:yes case:no')).toBe(false);
    expect(effectiveCase('foo', true)).toBe(true);
    expect(effectiveCase('foo case:no', true)).toBe(false);
  });
  it('replaces or drops the token and applies the default for the backend', () => {
    expect(withCase('a case:no b', true)).toBe('a b case:yes');
    expect(withCase('a case:no', null)).toBe('a');
    expect(effectiveText('a', true)).toBe('a case:yes');
    expect(effectiveText('a case:no', true)).toBe('a case:no');
  });
  it('filter UI edits keep the explicit case token', () => {
    const text = 'login case:no';
    expect(rewrite(text, toggleValue(parse(text), 'tag', 'ui'))).toBe('login tag:ui case:no');
  });
});

describe('@mention sugar', () => {
  it('parses @person as a mention filter and serializes it back', () => {
    const q = parse('fix @ana -@bob');
    expect(q.filters).toEqual([
      { key: 'mention', cmp: 'eq', value: 'ana', negate: false },
      { key: 'mention', cmp: 'eq', value: 'bob', negate: true },
    ]);
    expect(toText(q)).toBe('fix @ana -@bob');
    expect(toText(q, { sugar: false })).toBe('fix mention:ana -mention:bob');
  });
});

describe('groupByTag / navOrder', () => {
  const hit = (id: string, tags: string[]) => ({ id, board: 'b', tags }) as unknown as SearchHit;
  const hits = [hit('1', ['ui', 'UI']), hit('2', ['api']), hit('3', ['ui']), hit('4', [])];
  it('groups case-insensitively, biggest first, untagged last', () => {
    const g = groupByTag(hits, 'No tag');
    expect(g.map((x) => [x.label, x.hits.map((h) => h.id)])).toEqual([
      ['#ui', ['1', '3']],
      ['#api', ['2']],
      ['No tag', ['4']],
    ]);
    expect(navOrder(g, new Set(['ui'])).map((h) => h.id)).toEqual(['2', '4']);
  });
});
