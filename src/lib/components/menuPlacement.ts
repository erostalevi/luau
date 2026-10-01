// Context menu placement and motion (pure helpers, unit-tested).

import { cubicOut, quintOut } from 'svelte/easing';
import type { TransitionConfig } from 'svelte/transition';

export const MENU_MARGIN = 6;

export interface MenuRect {
  x: number;
  y: number;
}

/**
 * Top-left corner for a menu of size `w`×`h` requested at (`x`, `y`), kept inside the viewport.
 * When the menu would overflow on the right and `flipX` is given (a submenu's parent left edge),
 * it opens to the left of that edge instead, like native submenus.
 */
export function placeMenu(x: number, y: number, w: number, h: number, vw: number, vh: number, flipX?: number, margin = MENU_MARGIN): MenuRect {
  let left = x;
  if (left + w > vw - margin && flipX !== undefined && flipX - w >= margin) left = flipX - w;
  return {
    x: Math.max(margin, Math.min(left, vw - w - margin)),
    y: Math.max(margin, Math.min(y, vh - h - margin)),
  };
}

export type MotionLevel = 'full' | 'reduced' | 'none';

/** Fade duration and whether the menu may move (scale/slide) for a motion level. */
export function menuMotion(level: MotionLevel): { duration: number; move: boolean } {
  if (level === 'none') return { duration: 0, move: false };
  if (level === 'reduced') return { duration: 150, move: false };
  return { duration: 250, move: true };
}

/** Effective motion: the app setting (`data-motion`), lowered by the OS "reduce motion" preference. */
export function currentMotion(): MotionLevel {
  const set = (document.documentElement.dataset.motion as MotionLevel | undefined) ?? 'full';
  if (set === 'none') return 'none';
  const osReduced = typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches;
  return osReduced ? 'reduced' : set === 'reduced' ? 'reduced' : 'full';
}

/** Open: fade in plus the existing pop-in (slight slide up and scale). */
export function menuIn(_node: Element): TransitionConfig {
  const { duration, move } = menuMotion(currentMotion());
  return {
    duration,
    css: (t) => {
      const m = quintOut(t);
      return `opacity: ${cubicOut(t)};` + (move ? `transform: translateY(${(1 - m) * 6}px) scale(${0.98 + 0.02 * m});` : '');
    },
  };
}

/** Close: fade out in place (t runs 1 → 0; eased so the fade starts right away). */
export function menuOut(_node: Element): TransitionConfig {
  const { duration } = menuMotion(currentMotion());
  return { duration, css: (t) => `opacity: ${1 - cubicOut(1 - t)};` };
}
