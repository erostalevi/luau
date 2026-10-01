// Quick-change menus for the property chips in the editor footer
// (priority, due/start dates, labels, assignees). Every menu ends with
// "Edit as text", which drops the cursor into the raw footer.

import { rpc } from '$lib/backend/rpc';
import { openMenuAt, type MenuItem } from '$lib/state/menu.svelte';
import { parseFooter, setFooterFields } from '$lib/markdown/meta';
import { iso } from '$lib/util/naturalDate';
import { pickDate, pickPerson, pickTag } from '$lib/board/pickers';
import { t, fmtDate } from '$lib/i18n/index.svelte';

export const PRIORITIES = ['urgent', 'high', 'medium', 'low'] as const;
const PRIORITY_COLOR: Record<string, string> = { urgent: 'var(--danger)', high: '#e49a5a', medium: 'var(--warn)', low: 'var(--info)' };
const MAX_SUGGESTED = 8;

const splitList = (v: string) =>
  v
    .split(',')
    .map((s) => s.trim().replace(/^[@#]/, '').trim())
    .filter(Boolean);

/** Set (or with '' remove) one footer field, keeping the order of the others. */
export function withField(content: string, key: string, value: string): string {
  const fields = parseFooter(content.replace(/\r\n/g, '\n').split('\n')).fields.map(([k, v]) => [k, v] as [string, string]);
  const i = fields.findIndex(([k]) => k === key);
  if (i >= 0) fields[i] = [key, value];
  else fields.push([key, value]);
  return setFooterFields(content, fields);
}

const addDays = (n: number) => {
  const d = new Date();
  d.setDate(d.getDate() + n);
  return iso(d);
};

function dateItems(key: string, current: string, set: (v: string) => void): MenuItem[] {
  const nextMonday = addDays((8 - new Date().getDay()) % 7 || 7);
  const presets: [string, string][] = [
    ['dates.today', addDays(0)],
    ['dates.tomorrow', addDays(1)],
    ['dates.nextMonday', nextMonday],
    ['dates.inAWeek', addDays(7)],
  ];
  return [
    ...presets.map(([label, d]) => ({
      label: `${t(label)} · ${fmtDate(d, { weekday: 'short', month: 'short', day: 'numeric' })}`,
      checked: current.slice(0, 10) === d,
      run: () => set(d),
    })),
    { separator: true },
    {
      label: t('dates.custom'),
      run: async () => {
        const d = await pickDate(t(`properties.${key}`));
        if (d !== undefined) set(d ?? '');
      },
    },
    { label: t('dates.clear'), disabled: !current, run: () => set('') },
  ];
}

async function listItems(key: 'labels' | 'assignees', current: string, set: (v: string) => void): Promise<MenuItem[]> {
  const cur = splitList(current);
  const people = key === 'assignees';
  const known = people
    ? await rpc<string[]>('search.people').catch(() => [] as string[])
    : (await rpc<[string, number][]>('search.tags', { boards: [] }).catch(() => [] as [string, number][])).map(([tag]) => tag);
  const has = (x: string) => cur.some((c) => c.toLowerCase() === x.toLowerCase());
  const write = (list: string[]) => set(list.map((x) => (people ? `@${x}` : x)).join(', '));
  const prefix = people ? '@' : '#';
  const others = known.filter((x) => !has(x)).slice(0, MAX_SUGGESTED);
  return [
    ...cur.map((x) => ({ label: `${prefix}${x}`, checked: true, run: () => write(cur.filter((c) => c !== x)) })),
    ...others.map((x) => ({ label: `${prefix}${x}`, checked: false, run: () => write([...cur, x]) })),
    { separator: true },
    {
      label: people ? t('cards.assign') : t('cards.addTag'),
      run: async () => {
        const v = people ? await pickPerson() : await pickTag();
        if (v && !has(v)) write([...cur, v]);
      },
    },
    { label: t('propertyMenu.clear'), disabled: !cur.length, run: () => set('') },
  ];
}

/**
 * Open the quick-change menu for `key` under `anchor`. `read` returns the
 * current document; `write` receives the full new document.
 */
export async function openPropertyMenu(anchor: HTMLElement, key: string, read: () => string, write: (doc: string) => void, editRaw: () => void) {
  const current = parseFooter(read().split('\n')).fields.find(([k]) => k === key)?.[1] ?? '';
  const set = (v: string) => write(withField(read(), key, v));
  let items: MenuItem[] = [];
  if (key === 'priority') {
    const p = current.trim().toLowerCase();
    items = [
      ...PRIORITIES.map((x) => ({ label: t(`priority.${x}`), color: PRIORITY_COLOR[x], checked: p === x, run: () => set(x) })),
      { separator: true },
      { label: t('priority.none'), disabled: !p, run: () => set('') },
    ];
  } else if (key === 'due' || key === 'start') items = dateItems(key, current, set);
  else if (key === 'labels' || key === 'assignees') items = await listItems(key, current, set);
  items.push({ separator: true }, { label: t('propertyMenu.editText'), run: editRaw });
  openMenuAt(anchor, items);
}
