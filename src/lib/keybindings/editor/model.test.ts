import { describe, it, expect } from 'vitest';
import type { Keybinding } from '../defaults';
import {
  addKey,
  buildRows,
  changeKey,
  changeWhen,
  conflictCount,
  conflictsFor,
  isCustomized,
  isSingleKey,
  isSingleKeyBinding,
  matchesKeys,
  portableKey,
  removeKey,
  resetCommand,
  sourceKind,
  type BindingRow,
} from './model';

const eff: Keybinding[] = [
  { key: 'ctrl+shift+p', command: 'palette.commands', source: 'default' },
  { key: 'ctrl+k ctrl+s', command: 'app.openKeybindings', when: '!editorFocus', source: 'default' },
  { key: 'ctrl+b', command: 'panel.toggle', when: '!editorFocus', source: 'default' },
  { key: 'ctrl+b', command: 'editor.bold', when: 'editorFocus', source: 'default' },
  { key: 'n', command: 'card.new', when: 'boardFocus && config.board.singleKeyShortcuts', source: 'default' },
  { key: 'j', command: 'nav.down', source: 'preset:vim' },
  { key: 'ctrl+j', command: 'my.ext', source: 'acme.ext' },
  { key: 'ctrl+alt+x', command: 'card.archive', source: 'user' },
];

const rows = buildRows(
  ['palette.commands', 'app.openKeybindings', 'panel.toggle', 'editor.bold', 'card.new', 'nav.down', 'my.ext', 'card.archive', 'card.rename'],
  eff,
);
const row = (cmd: string) => rows.find((r) => r.command === cmd)!;

describe('sourceKind', () => {
  it('classifies layers', () => {
    expect(sourceKind('default').kind).toBe('default');
    expect(sourceKind(undefined).kind).toBe('default');
    expect(sourceKind('user').kind).toBe('user');
    expect(sourceKind('preset:vim')).toEqual({ kind: 'preset', detail: 'vim' });
    expect(sourceKind('acme.ext')).toEqual({ kind: 'extension', detail: 'acme.ext' });
  });
});

describe('buildRows', () => {
  it('lists every binding plus unbound commands', () => {
    expect(rows).toHaveLength(eff.length + 1);
    const unbound = row('card.rename');
    expect(unbound.key).toBe('');
    expect(unbound.source).toBe('none');
    expect(row('nav.down').sourceDetail).toBe('vim');
  });
  it('produces unique ids', () => {
    expect(new Set(rows.map((r) => r.id)).size).toBe(rows.length);
  });
});

describe('conflicts', () => {
  it('finds other commands on the same key', () => {
    const c = conflictsFor('ctrl+b', rows, { command: 'panel.toggle' });
    expect(c.map((r) => r.command)).toEqual(['editor.bold']);
    expect(conflictCount('ctrl+b', rows, { command: 'panel.toggle' })).toBe(1);
  });
  it('normalizes the key first', () => {
    expect(conflictCount('shift+ctrl+p', rows, { command: 'x' })).toBe(1);
  });
  it('ignores empty keys and non-conflicting keys', () => {
    expect(conflictCount('', rows)).toBe(0);
    expect(conflictCount('ctrl+alt+shift+f12', rows)).toBe(0);
  });
});

describe('matchesKeys', () => {
  it('single stroke matches any stroke of a chord', () => {
    expect(matchesKeys('ctrl+k ctrl+s', 'ctrl+s')).toBe(true);
    expect(matchesKeys('ctrl+k ctrl+s', 'ctrl+k')).toBe(true);
    expect(matchesKeys('ctrl+b', 'ctrl+k')).toBe(false);
  });
  it('chord matches as a prefix sequence', () => {
    expect(matchesKeys('ctrl+k ctrl+s', 'ctrl+k ctrl+s')).toBe(true);
    expect(matchesKeys('ctrl+k ctrl+s', 'ctrl+s ctrl+k')).toBe(false);
  });
  it('never matches unbound rows', () => {
    expect(matchesKeys('', 'ctrl+k')).toBe(false);
  });
});

describe('single keys', () => {
  it('detects printable single strokes', () => {
    expect(isSingleKey('n')).toBe(true);
    expect(isSingleKey('shift+/')).toBe(true);
    expect(isSingleKey('ctrl+n')).toBe(false);
    expect(isSingleKey('g g')).toBe(false);
    expect(isSingleKey('enter')).toBe(false);
  });
  it('detects bindings governed by the board toggle', () => {
    expect(isSingleKeyBinding(row('card.new'))).toBe(true);
    expect(isSingleKeyBinding(row('palette.commands'))).toBe(false);
  });
});

describe('portableKey', () => {
  it('rewrites the platform modifier as mod', () => {
    expect(portableKey('ctrl+shift+p', false)).toBe('mod+shift+p');
    expect(portableKey('cmd+k cmd+s', true)).toBe('mod+k mod+s');
    expect(portableKey('ctrl+x', true)).toBe('ctrl+x');
    expect(portableKey('n', false)).toBe('n');
  });
});

describe('user layer edits', () => {
  const user: Keybinding[] = [{ key: 'ctrl+alt+x', command: 'card.archive' }];

  it('changing a default adds a removal rule and a user binding with the same when', () => {
    const next = changeKey([], row('app.openKeybindings'), 'ctrl+alt+k', false);
    expect(next).toEqual([
      { key: 'mod+k mod+s', command: '-app.openKeybindings' },
      { key: 'mod+alt+k', command: 'app.openKeybindings', when: '!editorFocus' },
    ]);
  });

  it('changing a user binding replaces it in place', () => {
    const next = changeKey(user, row('card.archive'), 'ctrl+alt+y', false);
    expect(next).toEqual([{ key: 'mod+alt+y', command: 'card.archive' }]);
  });

  it('binding an unbound command just appends', () => {
    expect(changeKey([], row('card.rename'), 'f2', false)).toEqual([{ key: 'f2', command: 'card.rename' }]);
  });

  it('adding keeps existing bindings', () => {
    const next = addKey([], row('palette.commands'), 'f1', false);
    expect(next).toEqual([{ key: 'f1', command: 'palette.commands' }]);
  });

  it('removing a default writes a single removal rule', () => {
    const once = removeKey([], row('panel.toggle'), false);
    expect(once).toEqual([{ key: 'mod+b', command: '-panel.toggle' }]);
    expect(removeKey(once, row('panel.toggle'), false)).toHaveLength(1);
  });

  it('removing a user binding deletes it', () => {
    expect(removeKey(user, row('card.archive'), false)).toEqual([]);
  });

  it('removing an unbound row is a no-op', () => {
    expect(removeKey(user, row('card.rename'), false)).toBe(user);
  });

  it('changing when on a user row edits it; empty clears it', () => {
    const withWhen = changeWhen(user, row('card.archive'), 'boardFocus', false);
    expect(withWhen).toEqual([{ key: 'ctrl+alt+x', command: 'card.archive', when: 'boardFocus' }]);
    const r: BindingRow = { ...row('card.archive'), when: 'boardFocus' };
    expect(changeWhen(withWhen, r, '  ', false)).toEqual([{ key: 'ctrl+alt+x', command: 'card.archive' }]);
  });

  it('changing when on a default overrides it', () => {
    const next = changeWhen([], row('panel.toggle'), 'true', false);
    expect(next).toEqual([
      { key: 'mod+b', command: '-panel.toggle' },
      { key: 'mod+b', command: 'panel.toggle', when: 'true' },
    ]);
  });

  it('reset drops bindings and removal rules for a command only', () => {
    const list: Keybinding[] = [
      { key: 'mod+b', command: '-panel.toggle' },
      { key: 'mod+j', command: 'panel.toggle' },
      { key: 'f2', command: 'card.rename' },
    ];
    expect(isCustomized(list, 'panel.toggle')).toBe(true);
    const next = resetCommand(list, 'panel.toggle');
    expect(next).toEqual([{ key: 'f2', command: 'card.rename' }]);
    expect(isCustomized(next, 'panel.toggle')).toBe(false);
  });

  it('keeps args when overriding (tab.goto style bindings)', () => {
    const r: BindingRow = { id: 'x', command: 'tab.goto', key: 'ctrl+1', args: 0, source: 'default' };
    expect(changeKey([], r, 'alt+1', false)).toEqual([
      { key: 'mod+1', command: '-tab.goto' },
      { key: 'alt+1', command: 'tab.goto', args: 0 },
    ]);
  });
});
