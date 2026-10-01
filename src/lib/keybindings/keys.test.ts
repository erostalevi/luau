import { describe, expect, it } from 'vitest';
import { eventToStroke, keyName } from './keys';

const ev = (o: Partial<KeyboardEvent> & { altGraph?: boolean }): KeyboardEvent =>
  ({
    key: '',
    code: '',
    ctrlKey: false,
    altKey: false,
    shiftKey: false,
    metaKey: false,
    getModifierState: (m: string) => m === 'AltGraph' && !!o.altGraph,
    ...o,
  }) as unknown as KeyboardEvent;

describe('keyName', () => {
  it('follows the layout for letters (AZERTY Z sits on KeyW)', () => {
    expect(keyName(ev({ code: 'KeyW', key: 'z' }))).toBe('z');
    expect(keyName(ev({ code: 'KeyY', key: 'z' }))).toBe('z'); // QWERTZ
  });
  it('falls back to the physical key for non-Latin output', () => {
    expect(keyName(ev({ code: 'KeyN', key: '˜', altKey: true }))).toBe('n'); // ⌥N on macOS
    expect(keyName(ev({ code: 'KeyF', key: 'а' }))).toBe('f'); // Cyrillic
  });
  it('uses codes for digits and punctuation', () => {
    expect(keyName(ev({ code: 'Digit1', key: '&' }))).toBe('1');
    expect(keyName(ev({ code: 'BracketLeft', key: '^' }))).toBe('[');
  });
});

describe('eventToStroke', () => {
  it('ignores AltGr characters', () => {
    expect(eventToStroke(ev({ code: 'Digit2', key: '@', ctrlKey: true, altKey: true, altGraph: true }))).toBeNull();
  });
  it('keeps real Ctrl+Alt shortcuts', () => {
    expect(eventToStroke(ev({ code: 'KeyN', key: 'n', ctrlKey: true, altKey: true }))).toBe('ctrl+alt+n');
  });
});
