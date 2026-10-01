import { describe, expect, it } from 'vitest';
import { plainText, resolveLinks, resolveRange, slackText, ymd } from './range';

// 2026-09-30 is a Wednesday.
const wed = new Date(2026, 8, 30, 15, 0, 0);
const mon = new Date(2026, 8, 28, 9, 0, 0);

describe('resolveRange', () => {
  it('yesterday covers the whole previous day', () => {
    const r = resolveRange('yesterday', wed)!;
    expect(ymd(r.from)).toBe('2026-09-29');
    expect(r.from.getHours()).toBe(0);
    expect(ymd(r.to)).toBe('2026-09-29');
    expect(r.to.getHours()).toBe(23);
  });
  it('last workday on Monday spans Friday to Sunday', () => {
    const r = resolveRange('lastWorkday', mon)!;
    expect(ymd(r.from)).toBe('2026-09-25');
    expect(ymd(r.to)).toBe('2026-09-27');
    const w = resolveRange('lastWorkday', wed)!;
    expect([ymd(w.from), ymd(w.to)]).toEqual(['2026-09-29', '2026-09-29']);
  });
  it('weeks are Monday-based', () => {
    expect(ymd(resolveRange('thisWeek', wed)!.from)).toBe('2026-09-28');
    const lw = resolveRange('lastWeek', wed)!;
    expect([ymd(lw.from), ymd(lw.to)]).toEqual(['2026-09-21', '2026-09-27']);
    expect(ymd(resolveRange('last7', wed)!.from)).toBe('2026-09-24');
  });
  it('custom ranges are validated and ordered', () => {
    expect(resolveRange('custom', wed, { from: 'x', to: '2026-09-01' })).toBeNull();
    expect(resolveRange('custom', wed, { from: '2026-02-30', to: '2026-09-01' })).toBeNull();
    const r = resolveRange('custom', wed, { from: '2026-09-10', to: '2026-09-01' })!;
    expect([ymd(r.from), ymd(r.to)]).toEqual(['2026-09-01', '2026-09-10']);
  });
});

describe('export formats', () => {
  const md = '# Activity summary\n_Tue 29 Sep_\n\n## Work\n### Completed (1)\n- **Fix** login';
  it('plain text', () => {
    expect(plainText(md)).toBe('Activity summary\nTue 29 Sep\n\nWork\nCompleted (1)\n• Fix login');
  });
  it('slack', () => {
    expect(slackText(md)).toBe('*Activity summary*\n_Tue 29 Sep_\n\n*Work*\n*Completed (1)*\n• *Fix* login');
  });
  it('links', () => {
    expect(resolveLinks('- [[c123abc]] done', (id) => (id === 'c123abc' ? 'Fix login' : id))).toBe('- Fix login done');
  });
});
