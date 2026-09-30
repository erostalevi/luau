import { describe, expect, it } from 'vitest';
import { parse, toText, setValues, toggleValue, values, getRange, setRange, removeFilter, tokenize, highlightTerms } from './query';
import { snippetSegments, highlightSegments, groupByBoard, groupByLane } from './results';
import type { SearchHit } from '$lib/backend/types';

describe('query parser (mirror of query.rs)', () => {
  it('parses a mixed query like the Rust test', () => {
    const q = parse('login "exact phrase" -draft tag:backend -tag:wip board:"Project Alpha" due:<2026-10-10 case:yes');
    expect(q.terms).toHaveLength(3);
    expect(q.terms[1]).toEqual({ value: 'exact phrase', negate: false, phrase: true });
    expect(q.terms[2].negate).toBe(true);
    expect(q.filters[0]).toEqual({ key: 'tag', cmp: 'eq', value: 'backend', negate: false });
    expect(q.filters[1].negate).toBe(true);
    expect(q.filters[2].value).toBe('Project Alpha');
    expect(q.filters[3].cmp).toBe('lt');
    expect(q.caseSensitive).toBe(true);
  });

  it('treats unknown keys as text and strips @/# prefixes', () => {
    const q = parse('http://x.com foo:bar assignee:@ana tag:#ui');
    expect(q.terms.map((t) => t.value)).toEqual(['http://x.com', 'foo:bar']);
    expect(q.filters[0].value).toBe('ana');
    expect(q.filters[1].value).toBe('ui');
  });

  it('round-trips through toText', () => {
    const src = 'a -b tag:x "c d" in:title updated:>=2026-01-01 -is:archived';
    const text = toText(parse(src));
    expect(parse(text)).toEqual(parse(src));
    expect(text).toBe('a -b "c d" tag:x updated:>=2026-01-01 -is:archived in:title');
  });

  it('handles comparison operators', () => {
    const q = parse('due:<=2026-01-01 due:>=2025-01-01 updated:>2025-06-01');
    expect(q.filters.map((f) => f.cmp)).toEqual(['le', 'ge', 'gt']);
  });

  it('ignores empty values, lone dashes and quoted keys', () => {
    const q = parse('tag: - "tag:x"');
    expect(q.filters).toHaveLength(0);
    expect(q.terms.map((t) => t.value)).toEqual(['tag:', '-', 'tag:x']);
  });

  it('keeps quoted whitespace in one token', () => {
    expect(tokenize('a "b c"  d')).toEqual(['a', '"b c"', 'd']);
  });

  it('case and in flags', () => {
    expect(parse('case:no x').caseSensitive).toBe(false);
    expect(parse('case:true x').caseSensitive).toBe(true);
    expect(parse('in:body x').titleOnly).toBe(false);
    expect(parse('in:title x').titleOnly).toBe(true);
    expect(highlightTerms(parse('a -b "c d"'))).toEqual(['a', 'c d']);
  });
});

describe('filter-UI helpers', () => {
  it('sets, toggles and reads values without touching other parts', () => {
    let q = parse('hello -tag:wip tag:a');
    q = setValues(q, 'tag', ['b', 'c']);
    expect(toText(q)).toBe('hello -tag:wip tag:b tag:c');
    q = toggleValue(q, 'tag', 'B');
    expect(values(q, 'tag')).toEqual(['c']);
    q = toggleValue(q, 'is', 'open');
    expect(toText(q)).toBe('hello -tag:wip tag:c is:open');
    q = removeFilter(q, 0);
    expect(toText(q)).toBe('hello tag:c is:open');
  });

  it('quotes values with spaces', () => {
    expect(toText(setValues(parse(''), 'board', ['Project Alpha']))).toBe('board:"Project Alpha"');
  });

  it('reads and writes date ranges', () => {
    let q = parse('x due:overdue');
    expect(getRange(q, 'due')).toEqual({ preset: 'overdue', from: null, to: null });
    q = setRange(q, 'due', { preset: null, from: '2026-01-01', to: '2026-02-01' });
    expect(toText(q)).toBe('x due:>=2026-01-01 due:<=2026-02-01');
    expect(getRange(q, 'due')).toEqual({ preset: null, from: '2026-01-01', to: '2026-02-01' });
    q = setRange(q, 'due', { preset: null, from: '2026-03-03', to: '2026-03-03' });
    expect(toText(q)).toBe('x due:2026-03-03');
    q = setRange(q, 'due', { preset: null, from: null, to: null });
    expect(toText(q)).toBe('x');
  });
});

describe('result helpers', () => {
  it('splits snippets on control markers', () => {
    expect(snippetSegments('a \u0002hit\u0003 b')).toEqual([
      { text: 'a ', hit: false },
      { text: 'hit', hit: true },
      { text: ' b', hit: false },
    ]);
    expect(snippetSegments('<b>x</b>')).toEqual([{ text: '<b>x</b>', hit: false }]);
  });

  it('highlights terms with and without case sensitivity', () => {
    expect(highlightSegments('Fix Login', ['login'])).toEqual([
      { text: 'Fix ', hit: false },
      { text: 'Login', hit: true },
    ]);
    expect(highlightSegments('Fix Login', ['login'], true)).toEqual([{ text: 'Fix Login', hit: false }]);
  });

  const hit = (board: string, id: string, laneName: string | null): SearchHit => ({
    board,
    boardName: board.toUpperCase(),
    id,
    title: id,
    snippet: '',
    laneName,
    tags: [],
    kind: 'card',
    isGroup: false,
    archived: false,
    mtime: 0,
    remoteKey: null,
    status: null,
  });

  it('groups by board in rank order and by lane name', () => {
    const hits = [hit('b', '1', 'Doing'), hit('a', '2', null), hit('b', '3', 'doing'), hit('a', '4', 'Done')];
    expect(groupByBoard(hits).map((g) => [g.key, g.hits.length])).toEqual([
      ['b', 2],
      ['a', 2],
    ]);
    const lanes = groupByLane(hits, 'No lane');
    expect(lanes.map((g) => [g.label, g.hits.length])).toEqual([
      ['Doing', 2],
      ['Done', 1],
      ['No lane', 1],
    ]);
  });
});
