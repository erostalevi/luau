// Scroll positions remembered per tab (and per scroller inside it), so switching
// tabs — which remounts the view — brings every scroller back where it was.
// Pure module: no DOM, no runes; persistence is injected (see keepScroll.ts).

export interface ScrollPos {
  x: number;
  y: number;
}

/** Persisted shape: `{ "<scope>|<key>": [x, y] }` (zero positions are omitted). */
export type ScrollSnapshot = Record<string, [number, number]>;

export interface ScrollMemoryOptions {
  /** Returns the persisted snapshot, or `undefined` while it is not available yet. */
  load?: () => ScrollSnapshot | undefined;
  /** Receives the snapshot to persist (throttled). */
  save?: (data: ScrollSnapshot) => void;
  /** Minimum time between two `save` calls (trailing write). */
  throttleMs?: number;
  /** Max remembered scrollers; the least recently scrolled ones are dropped first. */
  cap?: number;
  setTimer?: (fn: () => void, ms: number) => unknown;
  clearTimer?: (h: unknown) => void;
}

export const SCOPE_SEP = '|';

/** Full key of a scroller: the tab (or panel) scope plus the scroller's local key. */
export function scrollKey(scope: string, key: string): string {
  return `${scope}${SCOPE_SEP}${key}`;
}

export function maxScroll(el: { scrollWidth: number; clientWidth: number; scrollHeight: number; clientHeight: number }): ScrollPos {
  return { x: Math.max(0, el.scrollWidth - el.clientWidth), y: Math.max(0, el.scrollHeight - el.clientHeight) };
}

/** Clamp a remembered position to what the scroller can show right now. */
export function clampScroll(pos: ScrollPos, max: ScrollPos): ScrollPos {
  return { x: Math.min(Math.max(0, pos.x), max.x), y: Math.min(Math.max(0, pos.y), max.y) };
}

/** True when `actual` is at `target` (sub-pixel / zoom rounding tolerated). */
export function sameScroll(actual: ScrollPos, target: ScrollPos, tolerance = 1.5): boolean {
  return Math.abs(actual.x - target.x) <= tolerance && Math.abs(actual.y - target.y) <= tolerance;
}

const valid = (n: unknown): n is number => typeof n === 'number' && Number.isFinite(n) && n >= 0;

export function createScrollMemory(opts: ScrollMemoryOptions = {}) {
  const throttleMs = opts.throttleMs ?? 1000;
  const cap = opts.cap ?? 400;
  const setTimer = opts.setTimer ?? ((fn: () => void, ms: number) => setTimeout(fn, ms));
  const clearTimer = opts.clearTimer ?? ((h: unknown) => clearTimeout(h as ReturnType<typeof setTimeout>));
  const map = new Map<string, ScrollPos>();
  let loaded = !opts.load;
  let timer: unknown = null;

  function ensureLoaded() {
    if (loaded) return;
    const data = opts.load!();
    if (data === undefined) return;
    loaded = true;
    if (!data || typeof data !== 'object') return;
    // Positions recorded before the snapshot arrived win over persisted ones.
    for (const [k, v] of Object.entries(data)) {
      if (!map.has(k) && Array.isArray(v) && valid(v[0]) && valid(v[1])) map.set(k, { x: v[0], y: v[1] });
    }
    trim();
  }

  function trim() {
    while (map.size > cap) map.delete(map.keys().next().value as string);
  }

  function snapshot(): ScrollSnapshot {
    const out: ScrollSnapshot = {};
    for (const [k, p] of map) if (p.x || p.y) out[k] = [p.x, p.y];
    return out;
  }

  function flush() {
    if (timer !== null) clearTimer(timer);
    timer = null;
    opts.save?.(snapshot());
  }

  function schedule() {
    if (!opts.save || timer !== null) return;
    timer = setTimer(() => {
      timer = null;
      opts.save!(snapshot());
    }, throttleMs);
  }

  return {
    get(key: string): ScrollPos | undefined {
      ensureLoaded();
      const p = map.get(key);
      return p ? { ...p } : undefined;
    },
    set(key: string, pos: ScrollPos) {
      ensureLoaded();
      if (!valid(pos.x) || !valid(pos.y)) return;
      const next = { x: Math.round(pos.x), y: Math.round(pos.y) };
      const prev = map.get(key);
      if (prev && prev.x === next.x && prev.y === next.y) return;
      // Re-insert so the Map stays ordered by last use (oldest first).
      map.delete(key);
      map.set(key, next);
      trim();
      schedule();
    },
    /** Drop every scroller of a scope (a closed tab). */
    forget(scope: string) {
      ensureLoaded();
      const prefix = scope + SCOPE_SEP;
      let changed = false;
      for (const k of [...map.keys()])
        if (k.startsWith(prefix)) {
          map.delete(k);
          changed = true;
        }
      if (changed) schedule();
    },
    flush,
    snapshot,
    get size() {
      return map.size;
    },
  };
}

export type ScrollMemory = ReturnType<typeof createScrollMemory>;
