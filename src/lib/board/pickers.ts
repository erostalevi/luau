// Quick pickers shared by card commands and the editor's property chips.

import { rpc } from '$lib/backend/rpc';
import { quickPick, pickOne, inputBox, BACK, type QuickItem } from '$lib/quickinput/qi.svelte';
import { t, fmtDate } from '$lib/i18n/index.svelte';

export async function pickTag(): Promise<string | undefined> {
  const tags = await rpc<[string, number][]>('search.tags', { boards: [] }).catch(() => []);
  const r = await quickPick(
    tags.map(([tag, n]) => ({ label: `#${tag}`, value: tag, description: String(n) })),
    {
      title: t('cards.addTag'),
      placeholder: t('cards.tagPlaceholder'),
      allowCustom: (text) => {
        const clean = text.replace(/^#/, '').trim().replace(/\s+/g, '-');
        return clean ? { label: t('cards.createTag', { tag: clean }), value: clean } : null;
      },
    },
  );
  return typeof r === 'string' ? r : undefined;
}

function isoDate(d: Date): string {
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
}

export async function pickDate(title: string): Promise<string | null | undefined> {
  const today = new Date();
  const add = (n: number) => {
    const d = new Date(today);
    d.setDate(d.getDate() + n);
    return isoDate(d);
  };
  const nextMonday = add((8 - today.getDay()) % 7 || 7);
  const items: QuickItem<string>[] = [
    { label: t('dates.today'), description: fmtDate(add(0), { weekday: 'short', month: 'short', day: 'numeric' }), value: add(0) },
    { label: t('dates.tomorrow'), description: fmtDate(add(1), { weekday: 'short', month: 'short', day: 'numeric' }), value: add(1) },
    { label: t('dates.nextMonday'), description: fmtDate(nextMonday, { weekday: 'short', month: 'short', day: 'numeric' }), value: nextMonday },
    { label: t('dates.inAWeek'), description: fmtDate(add(7), { weekday: 'short', month: 'short', day: 'numeric' }), value: add(7) },
    { label: t('dates.custom'), value: 'custom' },
    { label: t('dates.clear'), value: '' },
  ];
  const r = await pickOne(items, { title });
  if (r === undefined || r === BACK) return undefined;
  if (r !== 'custom') return r;
  const v = await inputBox({
    title,
    placeholder: 'YYYY-MM-DD',
    value: add(0),
    validate: (s) => (/^\d{4}-\d{2}-\d{2}$/.test(s) && !Number.isNaN(Date.parse(s)) ? null : t('validation.date')),
  });
  return typeof v === 'string' ? v : undefined;
}

/** Pick someone to assign (known people, or a new name). */
export async function pickPerson(): Promise<string | undefined> {
  const people = await rpc<string[]>('search.people').catch(() => []);
  const who = await quickPick(
    people.map((p) => ({ label: `@${p}`, value: p })),
    {
      title: t('cards.assign'),
      allowCustom: (s) => (s.trim() ? { label: t('cards.assignNew', { name: s.trim().replace(/^@/, '') }), value: s.trim().replace(/^@/, '') } : null),
    },
  );
  return typeof who === 'string' ? who : undefined;
}
