import { describe, expect, it } from 'vitest';
import { GUIDE_TEMPLATES, guideSpec } from './index';
import { NEW_BOARD_TEMPLATES } from '../io';

describe('guide boards', () => {
  it('are offered as templates', () => {
    for (const id of GUIDE_TEMPLATES) expect(NEW_BOARD_TEMPLATES).toContain(id);
  });

  for (const id of GUIDE_TEMPLATES) {
    it(`${id}: links, assets and titles resolve`, () => {
      const s = guideSpec(id);
      const texts = [...s.lanes.flatMap((l) => l.cards), ...s.notes];
      if (s.kind === 'files') expect(s.lanes).toEqual([]);
      else expect(s.notes).toEqual([]);
      expect(texts.length).toBeGreaterThan(10);
      texts.forEach((text, i) => {
        expect(text.startsWith('# ')).toBe(true);
        // Every card link was numbered and points at a card of this template.
        for (const m of text.matchAll(/\{\{card:([^}]*)\}\}/g)) {
          expect(m[1]).toMatch(/^\d+$/);
          expect(Number(m[1])).toBeLessThan(texts.length);
          expect(Number(m[1])).not.toBe(i);
        }
        // Every asset placeholder has a file attached to the same card.
        for (const m of text.matchAll(/\{\{asset:([^}]*)\}\}/g)) {
          expect(s.assets?.some((a) => a.card === i && a.name === m[1])).toBe(true);
        }
      });
      for (const a of s.assets ?? []) {
        expect(a.card).toBeLessThan(texts.length);
        expect(a.data).toMatch(/^[A-Za-z0-9+/]+=*$/);
        expect(texts[a.card]).toContain(`{{asset:${a.name}}}`);
      }
      expect(atob(s.assets!.find((a) => a.name.endsWith('.pdf'))!.data).startsWith('%PDF-')).toBe(true);
      expect(atob(s.assets!.find((a) => a.name.endsWith('.svg'))!.data)).toContain('<svg');
    });
  }
});
