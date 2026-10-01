import { describe, expect, it } from 'vitest';
import { evaluateWhen } from '$lib/commands/when';
import { fuzzy, highlightSegments } from '$lib/util/fuzzy';
import { parseNaturalDate, iso } from '$lib/util/naturalDate';
import { parseMeta, setFooterFields, parseFooter, setTitle, toggleTaskLine, titleLine } from '$lib/markdown/meta';
import { compileFilter } from '$lib/board/filter';
import { normalizeKey, normalizeStroke } from '$lib/keybindings/keys';
import { hexToRgb, rgbToHex, rgbToOklch, oklchToRgb } from '$lib/theme/color';
import type { NodeDto } from '$lib/backend/types';

describe('when clauses', () => {
  const ctx = (m: Record<string, unknown>) => (k: string) => m[k];
  it('handles boolean logic and comparisons', () => {
    const c = ctx({ boardFocus: true, inputFocus: false, boardType: 'kanban', n: 2 });
    expect(evaluateWhen('boardFocus && !inputFocus', c)).toBe(true);
    expect(evaluateWhen("boardType == 'kanban'", c)).toBe(true);
    expect(evaluateWhen("boardType != 'kanban' || inputFocus", c)).toBe(false);
    expect(evaluateWhen('(boardFocus || inputFocus) && n == 2', c)).toBe(true);
    expect(evaluateWhen('', c)).toBe(true);
    expect(evaluateWhen(undefined, c)).toBe(true);
  });
  it('supports dotted config keys', () => {
    expect(evaluateWhen('config.board.singleKeyShortcuts', (k) => k === 'config.board.singleKeyShortcuts')).toBe(true);
  });
});

describe('fuzzy', () => {
  it('prefers contiguous and word-start matches', () => {
    const a = fuzzy('new card', 'Card: New card')!;
    const b = fuzzy('ncd', 'New card')!;
    expect(a.score).toBeGreaterThan(b.score);
    expect(fuzzy('xyz', 'New card')).toBeNull();
    const segs = highlightSegments('New card', fuzzy('nc', 'New card')!.positions);
    expect(
      segs
        .filter((s) => s.hit)
        .map((s) => s.text)
        .join(''),
    ).toBe('Nc');
  });
});

describe('natural dates', () => {
  const now = new Date(2026, 8, 30); // Wed 30 Sep 2026
  it('parses relative words in en/es/pt', () => {
    expect(parseNaturalDate('today', now)).toBe('2026-09-30');
    expect(parseNaturalDate('tomorrow', now)).toBe('2026-10-01');
    expect(parseNaturalDate('mañana', now)).toBe('2026-10-01');
    expect(parseNaturalDate('amanhã', now)).toBe('2026-10-01');
    expect(parseNaturalDate('pasado mañana', now)).toBe('2026-10-02');
    expect(parseNaturalDate('in 3 days', now)).toBe('2026-10-03');
    expect(parseNaturalDate('en 2 semanas', now)).toBe('2026-10-14');
  });
  it('parses weekdays and month names', () => {
    expect(parseNaturalDate('friday', now)).toBe('2026-10-02');
    expect(parseNaturalDate('next monday', now)).toBe('2026-10-05');
    expect(parseNaturalDate('lunes', now)).toBe('2026-10-05');
    expect(parseNaturalDate('oct 3', now)).toBe('2026-10-03');
    expect(parseNaturalDate('3 de octubre', now)).toBe('2026-10-03');
    expect(parseNaturalDate('15 de janeiro', now)).toBe('2027-01-15');
    expect(parseNaturalDate('2026-12-24', now)).toBe('2026-12-24');
    expect(parseNaturalDate('banana', now)).toBeNull();
    expect(parseNaturalDate('feb 30', now)).toBeNull();
    expect(iso(new Date(2026, 0, 5))).toBe('2026-01-05');
  });
});

describe('markdown meta', () => {
  it('extracts title, tags, links, mentions, dates, tasks', () => {
    const m = parseMeta('# Title\n\nHello #urgent and #q4 @ana on [2026-10-03] see [[c1a2b3c]].\n`#code`\n\n- [x] a\n- [ ] b\n');
    expect(m.title).toBe('Title');
    expect(m.hasTitleLine).toBe(true);
    expect(m.tags).toEqual(['urgent', 'q4']);
    expect(m.mentions).toEqual(['ana']);
    expect(m.dates).toEqual(['2026-10-03']);
    expect(m.links[0].id).toBe('c1a2b3c');
    expect(m.tasks).toEqual({ total: 2, done: 1 });
    expect(m.face.kind).toBe('checklist');
  });
  it('faces: table, summary, none', () => {
    expect(parseMeta('# T\n\n| a | b |\n|---|---|\n| 1 | 2 |\n').face.kind).toBe('table');
    expect(parseMeta('# T\n\nSome text. More text.\n').face.kind).toBe('summary');
    expect(parseMeta('# T\n').face.kind).toBe('none');
  });
  it('footer set/parse roundtrip', () => {
    const c = setFooterFields('# T\n\nbody\n', [
      ['priority', 'high'],
      ['due', '2026-10-01'],
    ]);
    expect(c).toBe('# T\n\nbody\n\n---\npriority: high\ndue: 2026-10-01\n');
    const f = parseFooter(c.split('\n'));
    expect(f.priority).toBe('high');
    expect(setFooterFields(c, [])).toBe('# T\n\nbody\n');
  });
  it('title helpers and task toggling', () => {
    expect(titleLine('# Hello')).toBe('Hello');
    expect(titleLine('## Hello')).toBeNull();
    expect(setTitle('# Old\nx', 'New')).toBe('# New\nx');
    expect(setTitle('body', 'New')).toBe('# New\n\nbody');
    expect(toggleTaskLine('- [ ] a', 0)).toBe('- [x] a');
    expect(toggleTaskLine('plain', 0)).toBeNull();
  });
});

describe('board filter', () => {
  const node = (over: Partial<NodeDto>): NodeDto =>
    ({
      id: 'c1',
      parent: { kind: 'lane', id: 'k1' },
      isGroup: false,
      children: [],
      archived: false,
      cover: null,
      title: 'Fix login',
      hasTitleLine: true,
      tags: ['backend'],
      links: [],
      mentions: ['ana'],
      dates: [],
      footer: { startLine: null, fields: [], priority: 'high', due: '2020-01-01', start: null, assignees: [], labels: [] },
      tasks: { total: 2, done: 1 },
      face: { kind: 'none' },
      headings: [],
      attachments: [],
      mtime: 0,
      wordCount: 0,
      hasCode: false,
      ...over,
    }) as NodeDto;
  it('matches words, tags, people and predicates', () => {
    const n = node({});
    expect(compileFilter('login').test(n)).toBe(true);
    expect(compileFilter('#backend').test(n)).toBe(true);
    expect(compileFilter('#frontend').test(n)).toBe(false);
    expect(compileFilter('@an').test(n)).toBe(true);
    expect(compileFilter('is:open').test(n)).toBe(true);
    expect(compileFilter('due:overdue').test(n)).toBe(true);
    expect(compileFilter('priority:high -#backend').test(n)).toBe(false);
    expect(compileFilter('').empty).toBe(true);
  });
});

describe('keys', () => {
  it('normalizes modifier order and mod', () => {
    expect(normalizeStroke('Shift+Ctrl+P')).toBe('ctrl+shift+p');
    expect(normalizeKey('mod+k  mod+s').split(' ')).toHaveLength(2);
    expect(normalizeStroke('option+up')).toBe('alt+up');
  });
});

describe('color', () => {
  it('roundtrips through OKLCH', () => {
    const hex = '#7a7cf0';
    expect(rgbToHex(hexToRgb(hex))).toBe(hex);
    const back = rgbToHex(oklchToRgb(rgbToOklch(hexToRgb(hex))));
    expect(back).toBe(hex);
  });
});
