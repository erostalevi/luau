// Autocomplete for search bars: looks at the token under the caret and
// suggests filter keys, their values, #tags, @people and dates.
//
//   pri|           → priority:
//   priority:h|    → priority:high
//   is:|           → is:open, is:done, …
//   due:|          → due:today, due:overdue, due:<=2026-10-08 (next 7 days), …
//   due:next fri|  → due:2026-10-09 (natural dates, en/es/pt)
//   #ba|           → tag:backend   (or #backend where the bar takes #tags)
//   @an|           → @ana

import { parseNaturalDate, iso } from '$lib/util/naturalDate';

export type SuggestKind = 'key' | 'value' | 'tag' | 'person' | 'date';

export interface Suggestion {
  /** What the list shows. */
  label: string;
  /** Replaces the token under the caret. */
  insert: string;
  /** Secondary text (e.g. the date a preset resolves to). */
  detail?: string;
  kind: SuggestKind;
}

export interface SuggestResult {
  from: number;
  to: number;
  items: Suggestion[];
}

export interface SuggestSources {
  tags: string[];
  people: string[];
  boards?: string[];
  lanes?: string[];
  statuses?: string[];
}

export interface SuggestConfig {
  /** Filter keys this bar understands. */
  keys: readonly string[];
  /** How a picked tag is written: `#tag` or `tag:tag`. */
  tagStyle: 'hash' | 'key';
  /** Translations for the date presets (`today`, `next7`, …); falls back to English. */
  label?: (id: string) => string | undefined;
}

export const IS_VALUES = ['open', 'done', 'archived', 'group', 'card', 'doc', 'jira'];
export const HAS_VALUES = ['image', 'attachment', 'tasks', 'links', 'due', 'start', 'tags'];
export const PRIORITY_VALUES = ['urgent', 'high', 'medium', 'low'];
export const DATE_KEYS = new Set(['due', 'start', 'started', 'updated', 'created']);
const PEOPLE_KEYS = new Set(['mention', 'assignee']);
const MAX = 8;

const EN: Record<string, string> = {
  today: 'Today',
  overdue: 'Overdue',
  next7: 'Next 7 days',
  next30: 'Next 30 days',
  last7: 'Last 7 days',
  last30: 'Last 30 days',
  before: 'Before today',
  after: 'After today',
};

function addDays(now: Date, n: number): string {
  const d = new Date(now);
  d.setDate(d.getDate() + n);
  return iso(d);
}

function datePresets(key: string, now: Date): { id: string; value: string }[] {
  const today = iso(now);
  // Due dates look ahead; the others (started, updated, created) look back.
  if (key === 'due')
    return [
      { id: 'today', value: 'today' },
      { id: 'overdue', value: 'overdue' },
      { id: 'next7', value: `<=${addDays(now, 7)}` },
      { id: 'next30', value: `<=${addDays(now, 30)}` },
      { id: 'after', value: `>${today}` },
    ];
  return [
    { id: 'today', value: 'today' },
    { id: 'last7', value: `>=${addDays(now, -7)}` },
    { id: 'last30', value: `>=${addDays(now, -30)}` },
    { id: 'before', value: `<${today}` },
  ];
}

/** Bounds of the whitespace-separated token around `caret` (quotes keep spaces). */
export function tokenAt(text: string, caret: number): { from: number; to: number } {
  let inQuote = false;
  let start = 0;
  for (let i = 0; i < caret; i++) {
    const c = text[i];
    if (c === '"') inQuote = !inQuote;
    else if (/\s/.test(c) && !inQuote) start = i + 1;
  }
  let end = caret;
  while (end < text.length && (inQuote || !/\s/.test(text[end]))) {
    if (text[end] === '"') inQuote = !inQuote;
    end++;
  }
  return { from: start, to: end };
}

function rank(values: string[], typed: string): string[] {
  const q = typed.toLowerCase();
  const seen = new Set<string>();
  const starts: string[] = [];
  const contains: string[] = [];
  for (const v of values) {
    const k = v.toLowerCase();
    if (seen.has(k)) continue;
    seen.add(k);
    if (!q || k.startsWith(q)) starts.push(v);
    else if (k.includes(q)) contains.push(v);
  }
  return [...starts, ...contains];
}

const quote = (v: string) => (/\s/.test(v) ? `"${v}"` : v);

/**
 * Suggestions for the token under the caret, or null when there is nothing
 * useful to offer (empty token, plain word that matches no key, or the token
 * is already a complete value).
 */
export function suggest(text: string, caret: number, src: SuggestSources, cfg: SuggestConfig, now = new Date()): SuggestResult | null {
  const { from, to } = tokenAt(text, caret);
  const typed = text.slice(from, caret);
  if (!typed) return null;
  const neg = typed.startsWith('-') && typed.length > 1 ? '-' : '';
  const body = typed.slice(neg.length);
  const label = (id: string) => cfg.label?.(id) ?? EN[id] ?? id;
  const keys = new Set(cfg.keys);
  let items: Suggestion[] = [];

  const exact = (insert: string) => insert.toLowerCase() === text.slice(from, to).toLowerCase();

  if (body.startsWith('#')) {
    const tags = rank(src.tags, body.slice(1));
    items = tags.map((tag) => ({
      label: `#${tag}`,
      insert: cfg.tagStyle === 'hash' ? `${neg}#${tag}` : `${neg}tag:${quote(tag)}`,
      kind: 'tag' as const,
    }));
  } else if (body.startsWith('@')) {
    items = rank(src.people, body.slice(1)).map((p) => ({ label: `@${p}`, insert: `${neg}@${p}`, kind: 'person' as const }));
  } else if (body.includes(':')) {
    const colon = body.indexOf(':');
    const key = body.slice(0, colon).toLowerCase();
    if (!keys.has(key)) return null;
    const raw = body.slice(colon + 1);
    const op = /^(<=|>=|<|>)/.exec(raw)?.[0] ?? '';
    const val = raw.slice(op.length).replace(/^"|"$/g, '');
    const head = `${neg}${key}:`;
    const values = (list: string[], kind: SuggestKind = 'value') => rank(list, val).map((v) => ({ label: v, insert: `${head}${op}${quote(v)}`, kind }));
    if (key === 'is') items = values(IS_VALUES);
    else if (key === 'has') items = values(HAS_VALUES);
    else if (key === 'priority') items = values(PRIORITY_VALUES);
    else if (key === 'tag' || key === 'label') items = values(src.tags, 'tag');
    else if (PEOPLE_KEYS.has(key)) items = values(src.people, 'person');
    else if (key === 'board') items = values(src.boards ?? []);
    else if (key === 'lane') items = values(src.lanes ?? []);
    else if (key === 'status') items = values(src.statuses ?? []);
    else if (DATE_KEYS.has(key)) {
      const natural = val ? parseNaturalDate(val, now) : null;
      if (natural) items.push({ label: natural, insert: `${head}${op}${natural}`, detail: val, kind: 'date' });
      if (!op) {
        for (const p of datePresets(key, now)) {
          const name = label(p.id);
          if (val && !name.toLowerCase().includes(val.toLowerCase()) && !p.value.startsWith(val)) continue;
          items.push({ label: name, insert: `${head}${p.value}`, detail: p.value === p.id ? undefined : p.value, kind: 'date' });
        }
      }
    }
  } else if (/^[a-z]+$/i.test(body)) {
    const k = body.toLowerCase();
    items = cfg.keys.filter((key) => key.startsWith(k) && key !== k).map((key) => ({ label: `${key}:`, insert: `${neg}${key}:`, kind: 'key' as const }));
  }

  items = items.filter((s) => !exact(s.insert)).slice(0, MAX);
  return items.length ? { from, to, items } : null;
}

/** Apply a suggestion: returns the new text and caret. Values get a trailing space, keys don't. */
export function applySuggestion(text: string, r: SuggestResult, s: Suggestion): { text: string; caret: number } {
  const space = s.kind === 'key' ? '' : ' ';
  const tail = text.slice(r.to);
  const rest = space && tail.startsWith(' ') ? tail.slice(1) : tail;
  return { text: text.slice(0, r.from) + s.insert + space + rest, caret: r.from + s.insert.length + space.length };
}
