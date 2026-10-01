// Pure keyboard-navigation and ordering helpers for the explorer tree.

import type { Row } from './flatten';

/** Rows that can hold keyboard focus (placeholders cannot). */
export function focusable(r: Row): boolean {
  return r.type === 'section' || r.type === 'board' || r.type === 'lane' || r.type === 'card';
}

export function indexOfKey(rows: Row[], key: string): number {
  return rows.findIndex((r) => r.key === key);
}

/** Move focus by `delta` focusable rows (clamped). Unknown key → first row. */
export function step(rows: Row[], key: string, delta: number): string {
  const list = rows.filter(focusable);
  if (!list.length) return '';
  const i = list.findIndex((r) => r.key === key);
  if (i < 0) return list[delta < 0 ? list.length - 1 : 0].key;
  return list[Math.max(0, Math.min(list.length - 1, i + delta))].key;
}

export function edge(rows: Row[], last: boolean): string {
  const list = rows.filter(focusable);
  return (last ? list[list.length - 1] : list[0])?.key ?? '';
}

/** ArrowLeft: collapse an open row, otherwise go to its parent. */
export function leftTarget(rows: Row[], key: string): { collapse?: string; focus?: string } {
  const r = rows.find((x) => x.key === key);
  if (!r) return {};
  if (r.expandable && r.expanded) return { collapse: key };
  return r.parentKey ? { focus: r.parentKey } : {};
}

/** ArrowRight: expand a closed row, otherwise go to its first child. */
export function rightTarget(rows: Row[], key: string): { expand?: string; focus?: string } {
  const i = indexOfKey(rows, key);
  const r = rows[i];
  if (!r || !r.expandable) return {};
  if (!r.expanded) return { expand: key };
  const child = rows[i + 1];
  return child && child.parentKey === key && focusable(child) ? { focus: child.key } : {};
}

/**
 * Type-ahead: next focusable row whose label starts with `buffer`
 * (case/accent-insensitive), searching after the current row and wrapping.
 * With a multi-letter buffer the current row may match itself.
 */
export function typeahead(rows: Row[], fromKey: string, buffer: string): string | null {
  const needle = fold(buffer);
  if (!needle) return null;
  const n = rows.length;
  const from = indexOfKey(rows, fromKey);
  const startOffset = buffer.length > 1 ? 0 : 1;
  for (let k = 0; k < n; k++) {
    const r = rows[(Math.max(from, 0) + startOffset + k + n) % n];
    if (focusable(r) && fold(r.label).startsWith(needle)) return r.key;
  }
  return null;
}

function fold(s: string): string {
  return s
    .normalize('NFD')
    .replace(/[̀-ͯ]/g, '')
    .toLowerCase()
    .trim();
}

/**
 * Manual order for pinned boards after dropping `moving` before/after
 * `target`. `pinned` is the current display order of the section's pinned
 * boards; ids of `order` not involved keep their relative order at the end.
 */
export function reorderPinned(order: string[], pinned: string[], moving: string, target: string, after: boolean): string[] {
  if (moving === target) return order;
  const list = pinned.filter((id) => id !== moving);
  const at = list.indexOf(target);
  if (at < 0) return order;
  list.splice(after ? at + 1 : at, 0, moving);
  return [...list, ...order.filter((id) => !list.includes(id))];
}
