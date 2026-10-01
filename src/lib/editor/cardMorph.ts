// Shared-element ("hero") motion for the card editor surface: the modal ⇄
// sidebar morph and the grow-from-card open / shrink-to-card close.

import { cubicOut } from 'svelte/easing';
import type { TransitionConfig } from 'svelte/transition';
import { currentMotion, type MotionLevel } from '$lib/components/menuPlacement';

export type Rect = { left: number; top: number; width: number; height: number };

/** Mode-switch morph duration (ms) and easing (matches `--ease-out`). */
export const MORPH_MS = 260;
export const MORPH_EASE = 'cubic-bezier(0.16, 1, 0.3, 1)';
const HERO_MS = 240;

export function motion(): MotionLevel {
  return typeof document === 'undefined' ? 'none' : currentMotion();
}

/** On-screen rect of a card on a board (first visible match), if any. */
export function cardRect(boardId: string, cardId: string): Rect | null {
  if (typeof document === 'undefined' || !cardId) return null;
  const sel = `[data-card="${CSS.escape(cardId)}"][data-board="${CSS.escape(boardId)}"]`;
  for (const n of document.querySelectorAll<HTMLElement>(sel)) {
    const r = n.getBoundingClientRect();
    if (r.width > 0 && r.height > 0 && r.bottom > 0 && r.right > 0 && r.top < innerHeight && r.left < innerWidth) return r;
  }
  return null;
}

/**
 * CSS for a frame of the hero transition: the element (laid out at `to`)
 * drawn as if it sat at `from` when `k` = 0 and at `to` when `k` = 1.
 * Uses transform only (no layout) with the origin at the top-left corner.
 */
export function heroFrame(from: Rect, to: Rect, k: number): { tx: number; ty: number; sx: number; sy: number } {
  const sx0 = to.width ? from.width / to.width : 1;
  const sy0 = to.height ? from.height / to.height : 1;
  return {
    tx: (from.left - to.left) * (1 - k),
    ty: (from.top - to.top) * (1 - k),
    sx: sx0 + (1 - sx0) * k,
    sy: sy0 + (1 - sy0) * k,
  };
}

/**
 * Svelte transition for opening / closing the editor surface. Grows out of
 * (or shrinks back into) the card on the board when it is on screen;
 * otherwise a small scale (modal) or slide (sidebar). Opacity leads so the
 * brief stretch of the content while scaling is hidden.
 */
export function heroTransition(node: HTMLElement, p: { boardId: string; cardId: string; mode: 'modal' | 'sidebar' }): TransitionConfig {
  const m = motion();
  if (m === 'none') return { duration: 0 };
  if (m === 'reduced') return { duration: 140, css: (t) => `opacity: ${t}` };
  const from = cardRect(p.boardId, p.cardId);
  if (!from) {
    return p.mode === 'modal'
      ? { duration: 200, easing: cubicOut, css: (t) => `opacity: ${t}; transform: scale(${0.97 + 0.03 * t})` }
      : { duration: 220, easing: cubicOut, css: (t) => `opacity: ${t}; transform: translateX(${(1 - t) * 40}px)` };
  }
  const to = node.getBoundingClientRect();
  return {
    duration: HERO_MS,
    css: (t) => {
      const k = cubicOut(t);
      const f = heroFrame(from, to, k);
      return `transform-origin: 0 0; transform: translate(${f.tx}px, ${f.ty}px) scale(${f.sx}, ${f.sy}); opacity: ${Math.min(1, t * 2.2)}`;
    },
  };
}

/**
 * FLIP the element from `first` to its current (last) layout by animating its
 * real box (left/top/width/height) while pinned with `data-morph`, so the
 * content reflows instead of stretching. Returns the running animation.
 */
export function morphBox(el: HTMLElement, first: Rect, firstRadius: string): Animation | null {
  const last = el.getBoundingClientRect();
  const lastRadius = getComputedStyle(el).borderRadius;
  if (Math.abs(first.left - last.left) + Math.abs(first.top - last.top) + Math.abs(first.width - last.width) + Math.abs(first.height - last.height) < 1)
    return null;
  el.dataset.morph = '';
  const px = (r: Rect) => ({ left: `${r.left}px`, top: `${r.top}px`, width: `${r.width}px`, height: `${r.height}px` });
  const a = el.animate(
    [
      { ...px(first), borderRadius: firstRadius },
      { ...px(last), borderRadius: lastRadius },
    ],
    { duration: MORPH_MS, easing: MORPH_EASE },
  );
  const done = () => {
    if (el.getAnimations().every((x) => x === a || x.playState !== 'running')) delete el.dataset.morph;
  };
  a.onfinish = done;
  a.oncancel = done;
  return a;
}
