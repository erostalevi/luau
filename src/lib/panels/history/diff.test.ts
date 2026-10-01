import { describe, expect, it } from 'vitest';
import { classifyLines, collapse, diffLines, diffStats, diffWords, enrich, sideBySide, splitLines, tokenize, toHunks, unifiedText } from './diff';

describe('splitLines', () => {
  it('handles empty, CRLF and trailing newline', () => {
    expect(splitLines('')).toEqual([]);
    expect(splitLines('a\r\nb\n')).toEqual(['a', 'b']);
    expect(splitLines('a\n\n')).toEqual(['a', '']);
  });
});

describe('diffLines', () => {
  it('returns only equal lines for identical text', () => {
    const d = diffLines('a\nb\n', 'a\nb\n');
    expect(d.every((l) => l.kind === 'eq')).toBe(true);
    expect(diffStats(d)).toEqual({ added: 0, removed: 0 });
  });

  it('finds a minimal edit with correct line numbers', () => {
    const d = diffLines('# T\none\ntwo\nthree\n', '# T\none\n2\nthree\nfour\n');
    expect(d.map((l) => l.kind + ':' + l.text)).toEqual(['eq:# T', 'eq:one', 'del:two', 'add:2', 'eq:three', 'add:four']);
    expect(d.find((l) => l.kind === 'del')!.a).toBe(3);
    expect(d.find((l) => l.text === 'four')!.b).toBe(5);
    expect(diffStats(d)).toEqual({ added: 2, removed: 1 });
  });

  it('handles all-added and all-removed', () => {
    expect(diffLines('', 'x\ny').map((l) => l.kind)).toEqual(['add', 'add']);
    expect(diffLines('x\ny', '').map((l) => l.kind)).toEqual(['del', 'del']);
  });

  it('reconstructs both sides for random-ish edits', () => {
    const a = Array.from({ length: 40 }, (_, i) => `line ${i % 7} ${i}`).join('\n');
    const b = a
      .split('\n')
      .filter((_, i) => i % 5 !== 0)
      .map((l, i) => (i % 6 === 0 ? l + ' changed' : l))
      .join('\n');
    const d = diffLines(a, b);
    expect(d.filter((l) => l.kind !== 'add').map((l) => l.text)).toEqual(splitLines(a));
    expect(d.filter((l) => l.kind !== 'del').map((l) => l.text)).toEqual(splitLines(b));
  });
});

describe('hunks, side-by-side and unified text', () => {
  const d = diffLines('1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n', '1\n2\n3\n4\nfive\n6\n7\n8\n9\n10\n');
  it('groups changes with context', () => {
    const h = toHunks(d, 2);
    expect(h).toHaveLength(1);
    expect(h[0]).toMatchObject({ aStart: 3, aLen: 5, bStart: 3, bLen: 5 });
    expect(unifiedText(d, 1)).toBe('@@ -4,3 +4,3 @@\n 4\n-5\n+five\n 6');
  });
  it('pairs deletions with additions', () => {
    const rows = sideBySide(d);
    const changed = rows.find((r) => r.left?.kind === 'del')!;
    expect(changed.right?.text).toBe('five');
    expect(rows).toHaveLength(10);
  });
  it('collapses long unchanged runs', () => {
    const rows = collapse(d, (l) => l.kind !== 'eq', 1);
    expect(rows.map((r) => (r.type === 'gap' ? `gap${r.count}` : r.line.text))).toEqual(['gap3', '4', '5', 'five', '6', 'gap4']);
  });
});

describe('word diff', () => {
  it('keeps markdown constructs atomic', () => {
    expect(tokenize('See [[cwelcom]] and [docs](https://x.y) #tag @ana `a b`')).toEqual([
      'See',
      ' ',
      '[[cwelcom]]',
      ' ',
      'and',
      ' ',
      '[docs](https://x.y)',
      ' ',
      '#tag',
      ' ',
      '@ana',
      ' ',
      '`a b`',
    ]);
    expect(tokenize('**bold** text')).toEqual(['**', 'bold', '**', ' ', 'text']);
  });

  it('highlights only the changed words', () => {
    const w = diffWords('The quick brown fox', 'The quick red fox');
    expect(w.left).toEqual([
      { kind: 'eq', text: 'The quick ' },
      { kind: 'del', text: 'brown' },
      { kind: 'eq', text: ' fox' },
    ]);
    expect(w.right.find((s) => s.kind === 'add')?.text).toBe('red');
    expect(w.similarity).toBeGreaterThan(0.6);
  });

  it('reports low similarity for rewrites', () => {
    expect(diffWords('Completely different', 'Nothing alike here').similarity).toBeLessThan(0.35);
  });

  it('handles unicode words', () => {
    const w = diffWords('Revisión con @ana', 'Revisión con @luis');
    expect(w.left.find((s) => s.kind === 'del')?.text).toBe('@ana');
  });
});

describe('markdown awareness', () => {
  it('classifies headings, tasks, fences and the footer', () => {
    const kinds = classifyLines([
      '# Title',
      '',
      '- [ ] todo',
      '- [x] done',
      '- item',
      '```js',
      '# not a heading',
      '```',
      '> quote',
      '---',
      'priority: high',
    ]).map((m) => m.kind + (m.level ?? ''));
    expect(kinds).toEqual(['heading1', 'blank', 'task', 'taskDone', 'list', 'fence', 'code', 'fence', 'quote', 'rule', 'footer']);
  });

  it('enrich attaches md info and word segments to similar pairs only', () => {
    const a = '# Plan\n- [ ] Draft copy\nOld paragraph here\n';
    const b = '# Plan\n- [x] Draft copy\nTotally new words entirely\n';
    const r = enrich(diffLines(a, b), a, b);
    const task = r.find((l) => l.kind === 'add' && l.text.includes('Draft'))!;
    expect(task.md.kind).toBe('taskDone');
    expect(task.segs?.some((s) => s.kind === 'add' && s.text === 'x')).toBe(true);
    const para = r.find((l) => l.kind === 'add' && l.text.startsWith('Totally'))!;
    expect(para.segs).toBeUndefined();
    expect(r[0].md).toEqual({ kind: 'heading', level: 1 });
  });
});
