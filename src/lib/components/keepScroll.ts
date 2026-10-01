// `use:keepScroll={'lane:' + lane.id}` — remembers a scroller's position per tab
// and restores it when the view mounts again (tab switch, split move, restart).
//
// The scope comes from the nearest `[data-scroll-scope]` ancestor (PaneView sets
// the tab id, LeftPanel sets "left"). Without a scope, or with a null key, the
// action does nothing (e.g. the card editor in the modal/sidebar).
//
// Restore waits for layout: content of lazy views, async board/document loads,
// CodeMirror measuring and images can grow the scroller after mount, so the
// target is re-applied (clamped) whenever the scroller or its children resize,
// until it is reached, the user takes over (wheel/touch/pointer/key, or any
// scroll we did not cause — keyboard navigation, find, "open card"), or a
// timeout passes.

import { uiGet, uiSet, uiState } from '$lib/state/persist.svelte';
import {
  createScrollMemory,
  scrollKey,
  maxScroll,
  clampScroll,
  sameScroll,
  type ScrollMemory,
  type ScrollPos,
  type ScrollSnapshot,
} from '$lib/state/scrollMemory';

export const SCROLL_SCOPE_ATTR = 'data-scroll-scope';
const UI_KEY = 'scrollPositions';
/** Give up re-applying after this long (content that never grows back). */
const RESTORE_TIMEOUT_MS = 2500;

export const scrollMemory: ScrollMemory = createScrollMemory({
  load: () => (uiState.loaded ? uiGet<ScrollSnapshot>(UI_KEY, {}) : undefined),
  save: (data) => uiSet(UI_KEY, data),
  throttleMs: 1000,
});

/** Forget the positions of a closed tab. */
export function forgetScroll(scope: string) {
  scrollMemory.forget(scope);
}

function scopeOf(node: HTMLElement): string | null {
  return node.parentElement?.closest<HTMLElement>(`[${SCROLL_SCOPE_ATTR}]`)?.getAttribute(SCROLL_SCOPE_ATTR) || null;
}

const pos = (el: HTMLElement): ScrollPos => ({ x: el.scrollLeft, y: el.scrollTop });
const INTENT = ['wheel', 'touchstart', 'pointerdown', 'keydown'] as const;

export function attachKeepScroll(node: HTMLElement, localKey: string | null | undefined, memory: ScrollMemory = scrollMemory, timeoutMs = RESTORE_TIMEOUT_MS) {
  const scope = localKey ? scopeOf(node) : null;
  if (!scope || !localKey) return () => {};
  // Captured once: during a tab switch the scope attribute of the outgoing view
  // is already gone, and late scroll events must not land on the new tab.
  const key = scrollKey(scope, localKey);
  const target = memory.get(key);
  let restoring = !!target && (target.x > 0 || target.y > 0);
  let applied: ScrollPos | null = null;
  let ro: ResizeObserver | null = null;
  let mo: MutationObserver | null = null;
  let timer: ReturnType<typeof setTimeout> | null = null;

  function stop(record: boolean) {
    if (!restoring) return;
    restoring = false;
    ro?.disconnect();
    mo?.disconnect();
    ro = mo = null;
    if (timer) clearTimeout(timer);
    timer = null;
    for (const ev of INTENT) node.removeEventListener(ev, onIntent);
    // Clamp the memory to what the content allows now.
    if (record && node.isConnected && node.clientHeight > 0) memory.set(key, pos(node));
  }

  function apply() {
    if (!restoring || !target) return;
    if (!node.isConnected || node.clientHeight === 0 || node.clientWidth === 0) return; // not laid out yet
    const want = clampScroll(target, maxScroll(node));
    if (node.scrollLeft !== want.x) node.scrollLeft = want.x;
    if (node.scrollTop !== want.y) node.scrollTop = want.y;
    applied = pos(node);
    if (sameScroll(applied, target)) stop(false);
  }

  function onIntent() {
    stop(false);
  }

  function onScroll() {
    if (restoring) {
      // Our own writes echo back as scroll events; anything else means the
      // user or the app scrolled on purpose, so let go.
      if (applied && sameScroll(pos(node), applied)) return;
      stop(false);
    }
    memory.set(key, pos(node));
  }

  node.addEventListener('scroll', onScroll, { passive: true });
  if (restoring) {
    for (const ev of INTENT) node.addEventListener(ev, onIntent, { passive: true });
    apply();
  }
  if (restoring) {
    if (typeof ResizeObserver !== 'undefined') {
      ro = new ResizeObserver(() => apply());
      ro.observe(node);
      for (const c of node.children) ro.observe(c);
      if (typeof MutationObserver !== 'undefined') {
        // Children rendered later ({#if loaded}, lazy views) are observed too.
        mo = new MutationObserver((recs) => {
          for (const r of recs) for (const n of r.addedNodes) if (n instanceof Element) ro?.observe(n);
          apply();
        });
        mo.observe(node, { childList: true });
      }
    }
    timer = setTimeout(() => {
      apply();
      stop(true);
    }, timeoutMs);
  }

  return () => {
    stop(false);
    node.removeEventListener('scroll', onScroll);
  };
}

/** Svelte action. The key is local to the tab, e.g. `"board"` or `"lane:<id>"`. */
export function keepScroll(node: HTMLElement, localKey: string | null | undefined) {
  let current = localKey;
  let detach = attachKeepScroll(node, current);
  return {
    update(next: string | null | undefined) {
      if (next === current) return;
      detach();
      current = next;
      detach = attachKeepScroll(node, current);
    },
    destroy() {
      detach();
    },
  };
}
