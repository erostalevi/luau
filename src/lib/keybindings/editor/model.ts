// Pure model for the keyboard shortcuts editor: table rows, conflicts,
// key search and edits of the user layer (VS Code semantics: editing a
// default binding writes a `-command` removal rule plus a user binding).

import type { Keybinding } from '../defaults';
import { normalizeKey } from '../keys';

export type BindingSource = 'default' | 'user' | 'preset' | 'extension' | 'none';

export interface BindingRow {
  /** Stable-ish id for keyed lists. */
  id: string;
  command: string;
  /** Normalized key ("" when the command has no binding). */
  key: string;
  when?: string;
  args?: unknown;
  source: BindingSource;
  /** Preset id or extension id. */
  sourceDetail?: string;
}

export function sourceKind(source: string | undefined): { kind: BindingSource; detail?: string } {
  if (!source || source === 'default' || source === 'core') return { kind: 'default' };
  if (source === 'user') return { kind: 'user' };
  if (source.startsWith('preset:')) return { kind: 'preset', detail: source.slice(7) };
  return { kind: 'extension', detail: source };
}

/** One row per effective binding plus one row per command without any binding. */
export function buildRows(commandIds: string[], effective: Keybinding[]): BindingRow[] {
  const rows: BindingRow[] = [];
  const bound = new Set<string>();
  effective.forEach((b, i) => {
    const { kind, detail } = sourceKind(b.source);
    bound.add(b.command);
    rows.push({
      id: `b${i}:${b.command}:${b.key}`,
      command: b.command,
      key: b.key,
      when: b.when || undefined,
      args: b.args,
      source: kind,
      sourceDetail: detail,
    });
  });
  for (const id of commandIds) {
    if (!bound.has(id)) rows.push({ id: `c:${id}`, command: id, key: '', source: 'none' });
  }
  return rows;
}

/** Other rows bound to exactly the same key sequence. */
export function conflictsFor(key: string, rows: BindingRow[], except?: { command: string; rowId?: string }): BindingRow[] {
  if (!key) return [];
  const k = normalizeKey(key);
  return rows.filter((r) => r.key === k && r.id !== except?.rowId && r.command !== except?.command);
}

/** Number of distinct other commands sharing the key. */
export function conflictCount(key: string, rows: BindingRow[], except?: { command: string; rowId?: string }): number {
  return new Set(conflictsFor(key, rows, except).map((r) => r.command)).size;
}

/**
 * Key search: one recorded stroke matches any binding containing that stroke;
 * a recorded chord matches bindings starting with that sequence.
 */
export function matchesKeys(rowKey: string, recorded: string): boolean {
  if (!rowKey || !recorded) return false;
  const rec = normalizeKey(recorded).split(' ');
  const strokes = rowKey.split(' ');
  if (rec.length === 1) return strokes.includes(rec[0]);
  return rec.every((s, i) => strokes[i] === s);
}

/** A single stroke without modifiers (shift allowed) on a printable key. */
export function isSingleKey(key: string): boolean {
  if (!key || key.includes(' ')) return false;
  const parts = key.split('+');
  const k = parts.pop() ?? '';
  return parts.every((p) => p === 'shift') && k.length === 1;
}

/** Bindings affected by the "single-key shortcuts on boards" toggle. */
export function isSingleKeyBinding(row: Pick<BindingRow, 'key' | 'when'>): boolean {
  return /config\.board\.singleKeyShortcuts/.test(row.when ?? '') || (isSingleKey(row.key) && /boardFocus/.test(row.when ?? ''));
}

/** Rewrite the platform modifier as `mod` so user files travel across OSes. */
export function portableKey(key: string, mac: boolean): string {
  const primary = mac ? 'cmd' : 'ctrl';
  return normalizeKey(key)
    .split(' ')
    .map((stroke) =>
      stroke
        .split('+')
        .map((p, i, arr) => (i < arr.length - 1 && p === primary ? 'mod' : p))
        .join('+'),
    )
    .join(' ');
}

const same = (a: unknown, b: unknown) => JSON.stringify(a ?? null) === JSON.stringify(b ?? null);

export function findUserIndex(user: Keybinding[], row: BindingRow): number {
  return user.findIndex((u) => u.command === row.command && normalizeKey(u.key) === row.key && (u.when || '') === (row.when || '') && same(u.args, row.args));
}

function entry(key: string, row: BindingRow, when: string | undefined): Keybinding {
  const b: Keybinding = { key, command: row.command };
  if (when) b.when = when;
  if (row.args !== undefined) b.args = row.args;
  return b;
}

function withRemoval(user: Keybinding[], row: BindingRow, mac: boolean): Keybinding[] {
  const key = portableKey(row.key, mac);
  const exists = user.some((u) => u.command === `-${row.command}` && normalizeKey(u.key) === row.key);
  return exists ? [...user] : [...user, { key, command: `-${row.command}` }];
}

/** Change the key of a row (replace in the user layer or override a default). */
export function changeKey(user: Keybinding[], row: BindingRow, newKey: string, mac: boolean): Keybinding[] {
  const key = portableKey(newKey, mac);
  if (row.source === 'user') {
    const i = findUserIndex(user, row);
    if (i >= 0) return user.map((u, j) => (j === i ? { ...u, key } : u));
  }
  if (!row.key) return [...user, entry(key, row, undefined)];
  return [...withRemoval(user, row, mac), entry(key, row, row.when)];
}

/** Add another binding for the row's command (keeps its when clause). */
export function addKey(user: Keybinding[], row: BindingRow, newKey: string, mac: boolean): Keybinding[] {
  return [...user, entry(portableKey(newKey, mac), row, row.key ? row.when : undefined)];
}

/** Remove a binding: user entries are deleted, others get a removal rule. */
export function removeKey(user: Keybinding[], row: BindingRow, mac: boolean): Keybinding[] {
  if (!row.key) return user;
  if (row.source === 'user') {
    const i = findUserIndex(user, row);
    if (i >= 0) return user.filter((_, j) => j !== i);
  }
  return withRemoval(user, row, mac);
}

/** Change the when clause of a bound row. */
export function changeWhen(user: Keybinding[], row: BindingRow, when: string, mac: boolean): Keybinding[] {
  if (!row.key) return user;
  const w = when.trim() || undefined;
  if (row.source === 'user') {
    const i = findUserIndex(user, row);
    if (i >= 0)
      return user.map((u, j) => {
        if (j !== i) return u;
        const { when: _old, ...rest } = u;
        return w ? { ...rest, when: w } : rest;
      });
  }
  return [...withRemoval(user, row, mac), entry(portableKey(row.key, mac), row, w)];
}

/** Drop every user entry (bindings and removal rules) for a command. */
export function resetCommand(user: Keybinding[], command: string): Keybinding[] {
  return user.filter((u) => u.command !== command && u.command !== `-${command}`);
}

export function isCustomized(user: Keybinding[], command: string): boolean {
  return user.some((u) => u.command === command || u.command === `-${command}`);
}
