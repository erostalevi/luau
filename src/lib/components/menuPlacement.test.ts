import { describe, it, expect, afterEach, vi } from 'vitest';
import { placeMenu, menuMotion, currentMotion } from './menuPlacement';

describe('placeMenu', () => {
  const vw = 1000;
  const vh = 800;

  it('opens exactly at the requested point when it fits', () => {
    expect(placeMenu(120, 200, 220, 300, vw, vh)).toEqual({ x: 120, y: 200 });
  });

  it('clamps to the right and bottom edges with a margin', () => {
    expect(placeMenu(950, 700, 220, 300, vw, vh)).toEqual({ x: 1000 - 220 - 6, y: 800 - 300 - 6 });
  });

  it('clamps to the left and top margin', () => {
    expect(placeMenu(-40, -10, 220, 300, vw, vh)).toEqual({ x: 6, y: 6 });
  });

  it('keeps the margin even when the menu is larger than the viewport', () => {
    expect(placeMenu(10, 10, 220, 900, vw, vh)).toEqual({ x: 10, y: 6 });
  });

  it('flips a submenu to the left of its parent when it overflows on the right', () => {
    // Parent menu spans 700..920, submenu requested at its right edge.
    expect(placeMenu(916, 100, 220, 200, vw, vh, 704)).toEqual({ x: 704 - 220, y: 100 });
  });

  it('does not flip when there is no room on the left either', () => {
    expect(placeMenu(150, 100, 220, 200, 300, vh, 100)).toEqual({ x: 300 - 220 - 6, y: 100 });
  });
});

describe('menuMotion', () => {
  it('fades for about 250 ms with movement at full motion', () => {
    expect(menuMotion('full')).toEqual({ duration: 250, move: true });
  });

  it('only fades, shorter, when motion is reduced', () => {
    expect(menuMotion('reduced')).toEqual({ duration: 150, move: false });
  });

  it('is instant when motion is off', () => {
    expect(menuMotion('none')).toEqual({ duration: 0, move: false });
  });
});

describe('currentMotion', () => {
  afterEach(() => {
    delete document.documentElement.dataset.motion;
    vi.unstubAllGlobals();
  });

  const stubOs = (reduce: boolean) => vi.stubGlobal('matchMedia', (q: string) => ({ matches: reduce && q.includes('reduce') }) as MediaQueryList);

  it('follows the app setting', () => {
    stubOs(false);
    document.documentElement.dataset.motion = 'none';
    expect(currentMotion()).toBe('none');
    document.documentElement.dataset.motion = 'reduced';
    expect(currentMotion()).toBe('reduced');
    document.documentElement.dataset.motion = 'full';
    expect(currentMotion()).toBe('full');
  });

  it('respects prefers-reduced-motion from the OS', () => {
    stubOs(true);
    document.documentElement.dataset.motion = 'full';
    expect(currentMotion()).toBe('reduced');
  });
});
