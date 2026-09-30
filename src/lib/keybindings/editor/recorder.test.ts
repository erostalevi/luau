import { describe, it, expect } from 'vitest';
import { ChordRecorder } from './recorder';
import { filterRows, parseKbQuery } from './filter';
import { buildRows } from './model';

describe('ChordRecorder', () => {
  it('combines two quick strokes into a chord', () => {
    let now = 0;
    const r = new ChordRecorder(1200, () => now);
    r.push('ctrl+k');
    now = 500;
    expect(r.push('ctrl+s')).toEqual(['ctrl+k', 'ctrl+s']);
    expect(r.key).toBe('ctrl+k ctrl+s');
  });

  it('starts over after the chord window', () => {
    let now = 0;
    const r = new ChordRecorder(1200, () => now);
    r.push('ctrl+k');
    now = 2000;
    expect(r.push('ctrl+s')).toEqual(['ctrl+s']);
  });

  it('starts over after a complete chord', () => {
    let now = 0;
    const r = new ChordRecorder(1200, () => now);
    r.push('a');
    r.push('b');
    now = 10;
    expect(r.push('c')).toEqual(['c']);
  });

  it('armChord extends regardless of timing', () => {
    let now = 0;
    const r = new ChordRecorder(1200, () => now);
    r.push('ctrl+k');
    r.armChord();
    expect(r.chordArmed).toBe(true);
    now = 99999;
    expect(r.push('ctrl+s')).toEqual(['ctrl+k', 'ctrl+s']);
    expect(r.chordArmed).toBe(false);
  });

  it('clear resets', () => {
    const r = new ChordRecorder();
    r.push('x');
    r.clear();
    expect(r.key).toBe('');
  });
});

describe('keybinding table filter', () => {
  const rows = buildRows(['card.rename'], [
    { key: 'ctrl+b', command: 'panel.toggle', source: 'default' },
    { key: 'n', command: 'card.new', when: 'config.board.singleKeyShortcuts', source: 'default' },
    { key: 'ctrl+alt+x', command: 'card.archive', source: 'user' },
  ]);
  const text = (r: { command: string }) => ({ title: r.command === 'panel.toggle' ? 'Toggle sidebar' : r.command, category: 'View' });

  it('parses filters', () => {
    expect(parseKbQuery('@source:user  Move @singlekey @unbound')).toEqual({ words: ['move'], sources: ['user'], singleKey: true, unbound: true });
  });
  it('matches words on title, id and category', () => {
    expect(filterRows(rows, parseKbQuery('toggle side'), text).map((r) => r.command)).toEqual(['panel.toggle']);
    expect(filterRows(rows, parseKbQuery('card.'), text)).toHaveLength(3);
  });
  it('applies source, single-key, unbound and key filters', () => {
    expect(filterRows(rows, parseKbQuery('@source:user'), text).map((r) => r.command)).toEqual(['card.archive']);
    expect(filterRows(rows, parseKbQuery('@singlekey'), text).map((r) => r.command)).toEqual(['card.new']);
    expect(filterRows(rows, parseKbQuery('@unbound'), text).map((r) => r.command)).toEqual(['card.rename']);
    expect(filterRows(rows, parseKbQuery(''), text, 'ctrl+b').map((r) => r.command)).toEqual(['panel.toggle']);
  });
});
