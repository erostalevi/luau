import { describe, expect, it } from 'vitest';
import { capKeys, clickSelect, dragKeys, emptySelection, inOrder, prune, selectAll } from './resultSelection';

const order = ['A-1', 'A-2', 'A-3', 'A-4', 'A-5'];

describe('result list selection', () => {
  it('click replaces, ⌘-click toggles, keys stay in list order', () => {
    let s = clickSelect(order, emptySelection(), 'A-3');
    expect(s).toEqual({ keys: ['A-3'], anchor: 'A-3' });
    s = clickSelect(order, s, 'A-1', { toggle: true });
    expect(s.keys).toEqual(['A-1', 'A-3']);
    s = clickSelect(order, s, 'A-3', { toggle: true });
    expect(s.keys).toEqual(['A-1']);
    s = clickSelect(order, s, 'A-5');
    expect(s).toEqual({ keys: ['A-5'], anchor: 'A-5' });
  });

  it('⇧-click selects a range from the anchor in both directions; ⌘⇧ adds it', () => {
    let s = clickSelect(order, emptySelection(), 'A-4');
    s = clickSelect(order, s, 'A-2', { range: true });
    expect(s).toEqual({ keys: ['A-2', 'A-3', 'A-4'], anchor: 'A-4' });
    s = clickSelect(order, s, 'A-5', { range: true });
    expect(s.keys).toEqual(['A-4', 'A-5']);
    s = clickSelect(order, clickSelect(order, emptySelection(), 'A-1'), 'A-5', { toggle: true });
    s = clickSelect(order, s, 'A-3', { toggle: true, range: true });
    expect(s.keys).toEqual(['A-1', 'A-3', 'A-4', 'A-5']);
    // Without an anchor a range click is a plain click.
    expect(clickSelect(order, emptySelection(), 'A-2', { range: true }).keys).toEqual(['A-2']);
  });

  it('select all, prune, unknown keys', () => {
    expect(selectAll(order).keys).toEqual(order);
    expect(clickSelect(order, emptySelection(), 'X-9')).toEqual(emptySelection());
    expect(prune(['A-2', 'A-9'], { keys: ['A-1', 'A-2'], anchor: 'A-1' })).toEqual({ keys: ['A-2'], anchor: 'A-2' });
    expect(inOrder(order, new Set(['A-5', 'A-1', 'Z']))).toEqual(['A-1', 'A-5']);
  });

  it('drag carries the selection only when it starts on a selected row', () => {
    const s = { keys: ['A-4', 'A-2'], anchor: 'A-2' };
    expect(dragKeys(order, s, 'A-4')).toEqual(['A-2', 'A-4']);
    expect(dragKeys(order, s, 'A-1')).toEqual(['A-1']);
  });

  it('caps to the maximum', () => {
    const many = Array.from({ length: 120 }, (_, i) => `K-${i}`);
    const r = capKeys(many);
    expect(r.capped).toBe(true);
    expect(r.keys).toHaveLength(100);
    expect(r.keys[0]).toBe('K-0');
    expect(capKeys(order)).toEqual({ keys: order, capped: false });
  });
});
