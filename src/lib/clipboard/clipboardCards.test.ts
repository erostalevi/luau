import { describe, expect, it } from 'vitest';
import type { Parent } from '$lib/backend/types';
import {
  acceptAiCards,
  asIsCard,
  asIsSection,
  blockForInsertion,
  cleanText,
  createCardsOp,
  insertionTarget,
  MAX_CARDS,
  previewText,
  tooLong,
  type BoardLike,
} from './clipboardCards';

function board(kind: 'kanban' | 'files'): BoardLike {
  const nodes: Record<string, { parent: Parent }> = {
    a: { parent: { kind: 'lane', id: 'L1' } },
    b: { parent: { kind: 'lane', id: 'L1' } },
    sub: { parent: { kind: 'card', id: 'a' } },
    r1: { parent: { kind: 'root' } },
    r2: { parent: { kind: 'root' } },
  };
  return {
    id: 'B',
    kind,
    lanes: [
      { id: 'L0', archived: true },
      { id: 'L1', archived: false },
      { id: 'L2', archived: false },
    ],
    node: (id) => nodes[id],
    childrenOf: (p) => (p.kind === 'lane' ? (p.id === 'L1' ? ['a', 'b'] : []) : p.kind === 'card' ? ['sub'] : ['r1', 'r2']),
  };
}

describe('as is', () => {
  it('turns the first line into the title and keeps the rest verbatim', () => {
    expect(asIsCard('\n\n  Buy milk  \n\n- 2 litres\n  * skimmed\n')).toBe('# Buy milk\n\n- 2 litres\n  * skimmed\n');
    expect(asIsCard('Only a title')).toBe('# Only a title\n');
    expect(asIsCard('## Already a heading\nbody')).toBe('## Already a heading\n\nbody\n');
    expect(asIsCard('- [ ] call Ana\n- other')).toBe('# call Ana\n\n- other\n');
    expect(asIsCard('#hashtag first')).toBe('# #hashtag first\n');
    expect(asIsCard('   \n\t\n')).toBeNull();
    expect(asIsSection('# Title\nx')).toBe('## Title\n\nx\n');
    expect(asIsSection('Plain\nx')).toBe('## Plain\n\nx\n');
  });

  it('cleans control characters and CRLF', () => {
    expect(cleanText('a\r\nb\u0007\u0000c\r  \n')).toBe('a\nbc');
    expect(tooLong('x'.repeat(256 * 1024))).toBe(false);
    expect(tooLong('ñ'.repeat(128 * 1024 + 1))).toBe(true);
  });
});

describe('insertion target', () => {
  it('kanban: after the selected card at lane level, else focused lane, else first open lane', () => {
    const b = board('kanban');
    expect(insertionTarget({ board: b, docEditor: false, focus: 'a', lane: null })).toEqual({
      kind: 'cards',
      parent: { kind: 'lane', id: 'L1' },
      before: 'b',
    });
    expect(insertionTarget({ board: b, docEditor: false, focus: 'b', lane: null })).toEqual({
      kind: 'cards',
      parent: { kind: 'lane', id: 'L1' },
      before: null,
    });
    // A sub-card climbs to its lane-level card.
    expect(insertionTarget({ board: b, docEditor: false, focus: 'sub', lane: null })).toMatchObject({ before: 'b' });
    expect(insertionTarget({ board: b, docEditor: false, focus: null, lane: 'L2' })).toEqual({
      kind: 'cards',
      parent: { kind: 'lane', id: 'L2' },
      before: null,
    });
    // Archived / unknown lanes are skipped; the doc editor flag does not apply to kanban.
    expect(insertionTarget({ board: b, docEditor: true, focus: 'gone', lane: 'L0' })).toMatchObject({ parent: { kind: 'lane', id: 'L1' } });
    expect(insertionTarget({ board: { ...b, lanes: [] }, docEditor: false, focus: null, lane: null })).toBeNull();
    expect(insertionTarget({ board: null, docEditor: false, focus: null, lane: null })).toBeNull();
  });

  it('files: at the cursor with the editor, else after the selected document, else at the end', () => {
    const b = board('files');
    expect(insertionTarget({ board: b, docEditor: true, focus: 'r1', lane: null })).toEqual({ kind: 'doc' });
    expect(insertionTarget({ board: b, docEditor: false, focus: 'r1', lane: null })).toEqual({
      kind: 'cards',
      parent: { kind: 'root' },
      before: 'r2',
    });
    expect(insertionTarget({ board: b, docEditor: false, focus: null, lane: null })).toEqual({
      kind: 'cards',
      parent: { kind: 'root' },
      before: null,
    });
  });
});

describe('batch op', () => {
  const lane: Parent = { kind: 'lane', id: 'L1' };
  it('one card is a plain createCard; several are one batch in order', () => {
    expect(createCardsOp(['x'], ['# X\n'], lane, ['a', 'b'], null)).toEqual({ op: 'createCard', id: 'x', parent: lane, index: null, content: '# X\n' });
    const op = createCardsOp(['x', 'y'], ['# X\n', '# Y\n'], lane, ['a', 'b'], 'b');
    expect(op).toEqual({
      op: 'batch',
      ops: [
        { op: 'createCard', id: 'x', parent: lane, index: 1, content: '# X\n' },
        { op: 'createCard', id: 'y', parent: lane, index: 2, content: '# Y\n' },
      ],
    });
  });
});

describe('document insertion', () => {
  it('pads the block with blank lines only where needed', () => {
    expect(blockForInsertion(['## A\n\nx\n', '## B\n'], '', '')).toBe('## A\n\nx\n\n## B\n');
    expect(blockForInsertion(['## A\n'], 'intro', 'outro')).toBe('\n\n## A\n\n');
    expect(blockForInsertion(['## A\n'], 'intro\n', '\nmore')).toBe('\n## A\n');
    expect(blockForInsertion(['## A\n'], 'p\n\n', '\n\nq')).toBe('## A');
    expect(blockForInsertion(['  '], 'a', 'b')).toBe('');
  });
});

describe('AI answer', () => {
  it('keeps well-formed cards only, capped', () => {
    const ok = { title: 'T', markdown: '# T\n\nbody\n', section: '## T\n\nbody\n' };
    const raw = { cards: [ok, { title: 1 }, { title: 'x', markdown: 'no heading', section: '## x' }, ...Array(20).fill(ok)] };
    const cards = acceptAiCards(raw);
    expect(cards.length).toBe(MAX_CARDS);
    expect(cards[0]).toEqual(ok);
    expect(acceptAiCards(null)).toEqual([]);
    expect(acceptAiCards({ cards: 'nope' })).toEqual([]);
    expect(previewText(cards, 2)).toBe('• T\n• T\n… +10');
  });
});
