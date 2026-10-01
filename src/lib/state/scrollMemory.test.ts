import { describe, expect, it, vi, afterEach } from 'vitest';
import { createScrollMemory, scrollKey, clampScroll, maxScroll, sameScroll, type ScrollSnapshot } from './scrollMemory';
import { attachKeepScroll, SCROLL_SCOPE_ATTR } from '$lib/components/keepScroll';

describe('scroll keys and geometry', () => {
  it('scopes keys per tab', () => {
    expect(scrollKey('tab1', 'lane:l1')).toBe('tab1|lane:l1');
    expect(scrollKey('tab1', 'board')).not.toBe(scrollKey('tab2', 'board'));
  });

  it('clamps to the current max scroll and never below zero', () => {
    const max = maxScroll({ scrollWidth: 1000, clientWidth: 400, scrollHeight: 300, clientHeight: 500 });
    expect(max).toEqual({ x: 600, y: 0 });
    expect(clampScroll({ x: 900, y: 120 }, max)).toEqual({ x: 600, y: 0 });
    expect(clampScroll({ x: -5, y: -1 }, { x: 10, y: 10 })).toEqual({ x: 0, y: 0 });
  });

  it('tolerates sub-pixel differences', () => {
    expect(sameScroll({ x: 100.4, y: 0 }, { x: 100, y: 0 })).toBe(true);
    expect(sameScroll({ x: 104, y: 0 }, { x: 100, y: 0 })).toBe(false);
  });
});

describe('createScrollMemory', () => {
  afterEach(() => vi.useRealTimers());

  it('keeps positions in memory and rounds them', () => {
    const m = createScrollMemory();
    m.set('a|board', { x: 10.6, y: 3.2 });
    expect(m.get('a|board')).toEqual({ x: 11, y: 3 });
    expect(m.get('b|board')).toBeUndefined();
  });

  it('ignores invalid positions', () => {
    const m = createScrollMemory();
    m.set('a|x', { x: NaN, y: 1 });
    m.set('a|y', { x: -1, y: 1 });
    expect(m.size).toBe(0);
  });

  it('throttles persistence to one trailing write per window, omitting zero positions', () => {
    vi.useFakeTimers();
    const save = vi.fn();
    const m = createScrollMemory({ save, throttleMs: 1000 });
    for (let i = 1; i <= 50; i++) m.set('t|board', { x: i, y: 0 });
    m.set('t|lane:1', { x: 0, y: 0 });
    expect(save).not.toHaveBeenCalled();
    vi.advanceTimersByTime(999);
    expect(save).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    expect(save).toHaveBeenCalledTimes(1);
    expect(save).toHaveBeenLastCalledWith({ 't|board': [50, 0] });
    // A repeated identical position does not schedule another write.
    m.set('t|board', { x: 50, y: 0 });
    vi.advanceTimersByTime(2000);
    expect(save).toHaveBeenCalledTimes(1);
  });

  it('loads the persisted snapshot lazily, once it is available, without clobbering newer positions', () => {
    const persisted: { data?: ScrollSnapshot } = {};
    const m = createScrollMemory({ load: () => persisted.data });
    m.set('t|board', { x: 5, y: 5 }); // before the snapshot arrived
    persisted.data = { 't|board': [100, 0], 't|lane:1': [0, 240], bad: [-1, 2] as [number, number] };
    expect(m.get('t|lane:1')).toEqual({ x: 0, y: 240 });
    expect(m.get('t|board')).toEqual({ x: 5, y: 5 });
    expect(m.get('bad')).toBeUndefined();
  });

  it('forgets every scroller of a closed tab', () => {
    const m = createScrollMemory();
    m.set('t1|board', { x: 1, y: 0 });
    m.set('t1|lane:a', { x: 0, y: 2 });
    m.set('t10|board', { x: 3, y: 0 });
    m.forget('t1');
    expect(m.get('t1|board')).toBeUndefined();
    expect(m.get('t1|lane:a')).toBeUndefined();
    expect(m.get('t10|board')).toEqual({ x: 3, y: 0 });
  });

  it('caps the number of scrollers, dropping the least recently scrolled', () => {
    const m = createScrollMemory({ cap: 2 });
    m.set('a|k', { x: 1, y: 0 });
    m.set('b|k', { x: 1, y: 0 });
    m.set('a|k', { x: 2, y: 0 }); // touched again → most recent
    m.set('c|k', { x: 1, y: 0 });
    expect(m.get('b|k')).toBeUndefined();
    expect(m.get('a|k')).toEqual({ x: 2, y: 0 });
    expect(m.get('c|k')).toEqual({ x: 1, y: 0 });
  });
});

/** A jsdom element with a fake, adjustable layout (jsdom has none). */
function scroller(content: { w: number; h: number }, view = { w: 400, h: 300 }) {
  const scope = document.createElement('div');
  scope.setAttribute(SCROLL_SCOPE_ATTR, 'tab1');
  const el = document.createElement('div');
  scope.appendChild(el);
  document.body.appendChild(scope);
  let left = 0;
  let top = 0;
  const max = () => ({ x: Math.max(0, content.w - view.w), y: Math.max(0, content.h - view.h) });
  Object.defineProperties(el, {
    clientWidth: { get: () => view.w },
    clientHeight: { get: () => view.h },
    scrollWidth: { get: () => Math.max(view.w, content.w) },
    scrollHeight: { get: () => Math.max(view.h, content.h) },
    scrollLeft: { get: () => left, set: (v: number) => (left = Math.min(Math.max(0, v), max().x)) },
    scrollTop: { get: () => top, set: (v: number) => (top = Math.min(Math.max(0, v), max().y)) },
  });
  return { el, content, cleanup: () => scope.remove() };
}

describe('attachKeepScroll', () => {
  afterEach(() => vi.useRealTimers());

  it('records scrolls under the tab scope and restores them on the next mount', () => {
    const m = createScrollMemory();
    const a = scroller({ w: 2000, h: 300 });
    const detach = attachKeepScroll(a.el, 'board', m);
    a.el.scrollLeft = 700;
    a.el.dispatchEvent(new Event('scroll'));
    expect(m.get('tab1|board')).toEqual({ x: 700, y: 0 });
    detach();
    a.cleanup();

    const b = scroller({ w: 2000, h: 300 });
    attachKeepScroll(b.el, 'board', m);
    expect(b.el.scrollLeft).toBe(700);
    b.cleanup();
  });

  it('does nothing without a scope or a key', () => {
    const m = createScrollMemory();
    const el = document.createElement('div');
    document.body.appendChild(el);
    attachKeepScroll(el, 'board', m);
    el.dispatchEvent(new Event('scroll'));
    expect(m.size).toBe(0);
    const s = scroller({ w: 100, h: 100 });
    attachKeepScroll(s.el, null, m);
    s.el.dispatchEvent(new Event('scroll'));
    expect(m.size).toBe(0);
    el.remove();
    s.cleanup();
  });

  it('keeps the target while content is still short, then clamps memory on timeout', () => {
    vi.useFakeTimers();
    const m = createScrollMemory();
    m.set('tab1|doc', { x: 0, y: 900 });
    const s = scroller({ w: 400, h: 500 }); // only 200px scrollable so far
    attachKeepScroll(s.el, 'doc', m, 1000);
    expect(s.el.scrollTop).toBe(200);
    // Our own write echoes as a scroll event: must not overwrite the target.
    s.el.dispatchEvent(new Event('scroll'));
    expect(m.get('tab1|doc')).toEqual({ x: 0, y: 900 });
    vi.advanceTimersByTime(1000);
    expect(m.get('tab1|doc')).toEqual({ x: 0, y: 200 });
    s.cleanup();
  });

  it('lets go as soon as the user (or the app) scrolls elsewhere', () => {
    vi.useFakeTimers();
    const m = createScrollMemory();
    m.set('tab1|lane:a', { x: 0, y: 900 });
    const s = scroller({ w: 400, h: 500 });
    attachKeepScroll(s.el, 'lane:a', m, 1000);
    s.el.dispatchEvent(new Event('wheel'));
    s.el.scrollTop = 50;
    s.el.dispatchEvent(new Event('scroll'));
    expect(m.get('tab1|lane:a')).toEqual({ x: 0, y: 50 });
    // A foreign programmatic scroll (scrollIntoView) also ends the restore.
    m.set('tab1|lane:b', { x: 0, y: 900 });
    const s2 = scroller({ w: 400, h: 500 });
    attachKeepScroll(s2.el, 'lane:b', m, 1000);
    s2.el.scrollTop = 120;
    s2.el.dispatchEvent(new Event('scroll'));
    expect(m.get('tab1|lane:b')).toEqual({ x: 0, y: 120 });
    vi.advanceTimersByTime(1000);
    expect(s2.el.scrollTop).toBe(120);
    s.cleanup();
    s2.cleanup();
  });
});
