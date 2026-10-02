import { describe, expect, it } from 'vitest';
import { changeRange } from './change.svelte';

describe('change with AI range', () => {
  it('uses the selection when there is one', () => {
    expect(changeRange('# T\n\nabc', { from: 5, to: 7 })).toEqual({ from: 5, to: 7, whole: false });
  });
  it('without a selection: the body, not the title line or property footer', () => {
    const doc = '# Title\n\nBody one.\n\nBody two.\n\n---\npriority: high\n';
    const r = changeRange(doc, { from: 0, to: 0 });
    expect(r.whole).toBe(true);
    expect(doc.slice(r.from, r.to)).toBe('Body one.\n\nBody two.');
  });
  it('a card without a title line', () => {
    const doc = 'Just text\n';
    const r = changeRange(doc, { from: 3, to: 3 });
    expect(doc.slice(r.from, r.to)).toBe('Just text');
  });
});
