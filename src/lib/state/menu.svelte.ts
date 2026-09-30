// Context menus (one at a time).

import type { Component } from 'svelte';

export interface MenuItem {
  label?: string;
  icon?: Component<any>;
  /** Command id: shows its keybinding and runs it when no `run` given. */
  command?: string;
  args?: unknown;
  run?: () => void;
  danger?: boolean;
  disabled?: boolean;
  checked?: boolean;
  separator?: boolean;
  submenu?: MenuItem[];
  /** Small colored dot (lane colors, tags). */
  color?: string;
}

export const menu = $state<{ open: boolean; x: number; y: number; items: MenuItem[]; gen: number }>({
  open: false,
  x: 0,
  y: 0,
  items: [],
  gen: 0,
});

export function openMenu(at: MouseEvent | { x: number; y: number }, items: MenuItem[]) {
  if ('preventDefault' in at) {
    at.preventDefault();
    at.stopPropagation();
  }
  menu.x = 'clientX' in at ? at.clientX : at.x;
  menu.y = 'clientY' in at ? at.clientY : at.y;
  menu.items = items.filter((it, i, arr) => !(it.separator && (i === 0 || arr[i - 1]?.separator || i === arr.length - 1)));
  menu.open = true;
  menu.gen++;
}

export function openMenuAt(el: HTMLElement, items: MenuItem[]) {
  const r = el.getBoundingClientRect();
  openMenu({ x: r.left, y: r.bottom + 4 }, items);
}

export function closeMenu() {
  menu.open = false;
}
