import { describe, expect, it } from 'vitest';
import { applySuggestion, suggest, tokenAt, type SuggestConfig, type SuggestSources } from './suggest';
import { KEYS } from './query';

const src: SuggestSources = { tags: ['backend', 'design', 'bug'], people: ['ana', 'eros'], boards: ['Product Roadmap'], lanes: ['Doing'] };
const panel: SuggestConfig = { keys: KEYS, tagStyle: 'key' };
const bar: SuggestConfig = { keys: ['is', 'has', 'due', 'started', 'priority', 'tag'], tagStyle: 'hash' };
const now = new Date(2026, 9, 1); // Thu 1 Oct 2026

const at = (text: string, cfg = panel) => suggest(text, text.length, src, cfg, now);
const inserts = (text: string, cfg = panel) => at(text, cfg)?.items.map((s) => s.insert) ?? [];

describe('search autocomplete', () => {
  it('finds the token under the caret, quotes included', () => {
    expect(tokenAt('foo board:"Pro Ro', 17)).toEqual({ from: 4, to: 17 });
    expect(tokenAt('a bc d', 3)).toEqual({ from: 2, to: 4 });
  });

  it('completes keys from a prefix, without a trailing space', () => {
    expect(inserts('pri')).toEqual(['priority:']);
    expect(inserts('st')).toEqual(['status:', 'start:', 'started:']);
    const r = at('foo pri')!;
    expect(applySuggestion('foo pri', r, r.items[0])).toEqual({ text: 'foo priority:', caret: 13 });
  });

  it('completes values for is:, has:, priority: and keeps negation', () => {
    expect(inserts('is:')).toContain('is:open');
    expect(inserts('has:im')).toEqual(['has:image']);
    expect(inserts('-priority:u')).toEqual(['-priority:urgent', '-priority:medium']);
  });

  it('tags and people', () => {
    expect(inserts('#b')).toEqual(['tag:backend', 'tag:bug']);
    expect(inserts('#b', bar)).toEqual(['#backend', '#bug']);
    expect(inserts('@a')).toEqual(['@ana']);
    expect(inserts('tag:des')).toEqual(['tag:design']);
    expect(inserts('board:pro')).toEqual(['board:"Product Roadmap"']);
  });

  it('dates: presets per key and natural language', () => {
    expect(inserts('due:')).toEqual(['due:today', 'due:overdue', 'due:<=2026-10-08', 'due:<=2026-10-31', 'due:>2026-10-01']);
    expect(inserts('started:')).toContain('started:>=2026-09-24');
    expect(inserts('due:tomorrow')).toEqual(['due:2026-10-02']);
    expect(inserts('due:<friday')).toEqual(['due:<2026-10-02']);
  });

  it('stays quiet for plain words, unknown keys and finished values', () => {
    expect(at('login')).toBeNull();
    expect(at('foo:bar')).toBeNull();
    expect(at('is:open')).toBeNull();
    expect(at('')).toBeNull();
    expect(at('assignee:x', bar)).toBeNull();
  });

  it('values replace the whole token and add one space', () => {
    const text = 'is:op tag:x';
    const r = suggest(text, 5, src, panel, now)!;
    expect(applySuggestion(text, r, r.items[0])).toEqual({ text: 'is:open tag:x', caret: 8 });
  });
});
