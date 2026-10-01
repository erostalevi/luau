// Filter-UI dropdowns for the search panel. Each menu reads the parsed query
// and writes the edited query back to the text (two-way sync, see sync.ts).

import type { MenuItem } from '$lib/state/menu.svelte';
import { rpc } from '$lib/backend/rpc';
import { registry } from '$lib/state/registry.svelte';
import { boards } from '$lib/state/boards.svelte';
import { inputBox, steps } from '$lib/quickinput/qi.svelte';
import { t } from '$lib/i18n/index.svelte';
import { parse, hasValue, toggleValue, getRange, setRange, type Query } from './query';
import { rewrite } from './sync';

export type FilterKind = 'board' | 'lane' | 'tag' | 'is' | 'has' | 'priority' | 'mention' | 'due' | 'updated';

export const FILTER_KINDS: FilterKind[] = ['board', 'lane', 'tag', 'is', 'has', 'priority', 'mention', 'due', 'updated'];

const IS_VALUES = ['open', 'done', 'archived', 'group', 'doc', 'jira'];
const HAS_VALUES = ['image', 'attachment', 'tasks', 'links', 'due', 'tags'];
const PRIORITIES = ['urgent', 'high', 'medium', 'low'];

const iso = (d: Date) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
const addDays = (n: number) => {
  const d = new Date();
  d.setDate(d.getDate() + n);
  return iso(d);
};

type Setter = (text: string) => void;

function toggler(text: string, set: Setter, key: string, values: string[], labels?: (v: string) => string): MenuItem[] {
  const q = parse(text);
  return values.map((v) => ({
    label: labels ? labels(v) : v,
    checked: hasValue(q, key, v),
    run: () => set(rewrite(text, toggleValue(parse(text), key, v))),
  }));
}

function laneNames(): string[] {
  const seen = new Map<string, string>();
  for (const b of boards.values()) for (const l of b.lanes) if (!l.archived && l.name.trim()) seen.set(l.name.trim().toLowerCase(), l.name.trim());
  return [...seen.values()].sort((a, b) => a.localeCompare(b));
}

async function customRange(text: string, set: Setter, key: 'due' | 'updated') {
  const cur = getRange(parse(text), key);
  const valid = (s: string) => (s === '' || (/^\d{4}-\d{2}-\d{2}$/.test(s) && !Number.isNaN(Date.parse(s))) ? null : t('validation.date'));
  const r = await steps<[string, string]>([
    () => inputBox({ title: t(`searchPanel.filters.${key}`), prompt: t('searchPanel.rangeFrom'), placeholder: 'YYYY-MM-DD', value: cur.from ?? '', validate: valid, step: 1, totalSteps: 2 }),
    () => inputBox({ title: t(`searchPanel.filters.${key}`), prompt: t('searchPanel.rangeTo'), placeholder: 'YYYY-MM-DD', value: cur.to ?? '', validate: valid, step: 2, totalSteps: 2 }),
  ]);
  if (!r) return;
  const [from, to] = r;
  set(rewrite(text, setRange(parse(text), key, { preset: null, from: from || null, to: to || null })));
}

function rangeMenu(text: string, set: Setter, key: 'due' | 'updated'): MenuItem[] {
  const cur = getRange(parse(text), key);
  const put = (r: { preset: string | null; from: string | null; to: string | null }) => set(rewrite(text, setRange(parse(text), key, r)));
  const today = addDays(0);
  const presets: { label: string; r: { preset: string | null; from: string | null; to: string | null } }[] =
    key === 'due'
      ? [
          { label: t('searchPanel.dates.overdue'), r: { preset: 'overdue', from: null, to: null } },
          { label: t('searchPanel.dates.today'), r: { preset: 'today', from: null, to: null } },
          { label: t('searchPanel.dates.next7'), r: { preset: null, from: today, to: addDays(7) } },
          { label: t('searchPanel.dates.next30'), r: { preset: null, from: today, to: addDays(30) } },
        ]
      : [
          { label: t('searchPanel.dates.today'), r: { preset: 'today', from: null, to: null } },
          { label: t('searchPanel.dates.last7'), r: { preset: null, from: addDays(-7), to: null } },
          { label: t('searchPanel.dates.last30'), r: { preset: null, from: addDays(-30), to: null } },
        ];
  const same = (a: typeof cur, b: typeof cur) => a.preset === b.preset && a.from === b.from && a.to === b.to;
  const any = !!(cur.preset || cur.from || cur.to);
  return [
    ...presets.map((p) => ({ label: p.label, checked: same(cur, p.r), run: () => put(p.r) })),
    { separator: true },
    { label: t('searchPanel.dates.custom'), run: () => void customRange(text, set, key) },
    ...(any ? [{ label: t('searchPanel.dates.clear'), run: () => put({ preset: null, from: null, to: null }) }] : []),
  ];
}

/** Menu items for one filter kind (async: tags/people come from the index). */
export async function filterMenu(kind: FilterKind, text: string, set: Setter): Promise<MenuItem[]> {
  switch (kind) {
    case 'board': {
      const list = registry.data.boards.filter((b) => !b.missing).sort((a, b) => a.name.localeCompare(b.name));
      return list.length ? toggler(text, set, 'board', list.map((b) => b.name)) : [{ label: t('searchPanel.nothing'), disabled: true }];
    }
    case 'lane': {
      const names = laneNames();
      return [
        ...toggler(text, set, 'lane', names),
        ...(names.length ? [{ separator: true } as MenuItem] : []),
        {
          label: t('searchPanel.otherLane'),
          run: async () => {
            const v = await inputBox({ title: t('searchPanel.filters.lane'), placeholder: t('searchPanel.lanePlaceholder') });
            if (typeof v === 'string' && v.trim()) set(rewrite(text, toggleValue(parse(text), 'lane', v.trim())));
          },
        },
      ];
    }
    case 'tag': {
      const tags = await rpc<[string, number][]>('search.tags', { boards: [] }).catch(() => [] as [string, number][]);
      const top = tags.slice(0, 40).map(([tag]) => tag);
      for (const v of parse(text).filters.filter((f) => f.key === 'tag' && !f.negate).map((f) => f.value)) if (!top.includes(v.toLowerCase())) top.push(v);
      return top.length ? toggler(text, set, 'tag', top, (v) => `#${v}`) : [{ label: t('searchPanel.noTags'), disabled: true }];
    }
    case 'mention': {
      const people = await rpc<string[]>('search.people').catch(() => [] as string[]);
      return people.length ? toggler(text, set, 'mention', people, (v) => `@${v}`) : [{ label: t('searchPanel.noPeople'), disabled: true }];
    }
    case 'is':
      return toggler(text, set, 'is', IS_VALUES, (v) => t(`searchPanel.is.${v}`));
    case 'has':
      return toggler(text, set, 'has', HAS_VALUES, (v) => t(`searchPanel.has.${v}`));
    case 'priority':
      return toggler(text, set, 'priority', PRIORITIES, (v) => t(`priority.${v}`));
    case 'due':
    case 'updated':
      return rangeMenu(text, set, kind);
  }
}

/** How many active filters of a kind (for the button badges). */
export function activeCount(q: Query, kind: FilterKind): number {
  return q.filters.filter((f) => f.key === kind || (kind === 'tag' && f.key === 'label') || (kind === 'mention' && f.key === 'assignee')).length;
}

const tOr = (key: string, fallback: string) => {
  const v = t(key);
  return v === key ? fallback : v;
};

const OPS: Record<string, string> = { eq: '', lt: '< ', le: '≤ ', gt: '> ', ge: '≥ ' };

/** Human label for a chip (`#backend`, `@ana`, `due ≤ 2026-10-10`, `is: open`). */
export function chipLabel(f: Query['filters'][number]): string {
  if (f.key === 'tag' || f.key === 'label') return `#${f.value}`;
  if (f.key === 'mention' || f.key === 'assignee') return `@${f.value}`;
  const key = tOr(`searchPanel.keys.${f.key}`, f.key);
  if (f.key === 'is') return tOr(`searchPanel.is.${f.value.toLowerCase()}`, `is: ${f.value}`);
  if (f.key === 'has') return `${key} ${tOr(`searchPanel.has.${f.value.toLowerCase()}`, f.value)}`;
  return `${key} ${OPS[f.cmp]}${f.value}`.replace(/\s+/g, ' ');
}
