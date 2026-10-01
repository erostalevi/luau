import { describe, expect, it } from 'vitest';
import { heroFrame } from './cardMorph';

describe('heroFrame', () => {
  const card = { left: 100, top: 200, width: 240, height: 80 };
  const modal = { left: 300, top: 40, width: 800, height: 700 };

  it('starts exactly on the card rect', () => {
    const f = heroFrame(card, modal, 0);
    expect(modal.left + f.tx).toBe(card.left);
    expect(modal.top + f.ty).toBe(card.top);
    expect(modal.width * f.sx).toBeCloseTo(card.width);
    expect(modal.height * f.sy).toBeCloseTo(card.height);
  });

  it('ends at identity', () => {
    const f = heroFrame(card, modal, 1);
    for (const [k, v] of Object.entries({ tx: 0, ty: 0, sx: 1, sy: 1 })) expect(f[k as keyof typeof f]).toBeCloseTo(v);
  });

  it('survives a zero-size target', () => {
    const f = heroFrame(card, { left: 0, top: 0, width: 0, height: 0 }, 0.5);
    expect(Number.isFinite(f.sx) && Number.isFinite(f.sy)).toBe(true);
  });
});
