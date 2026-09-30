// Tiny reactive i18n: dictionaries are nested objects; keys are dot paths.
// `t('board.newCard')`, `t('trash.restored', { count: 2 })`.
// Plurals: a value `{ one: '…', other: '…' }` selected by `params.count`.

import enBase from './en';

type PartModule = { default: Dict };
/** Feature dictionaries: `parts/<feature>.<locale>.ts` are merged into the locale. */
const partModules = import.meta.glob<PartModule>('./parts/*.ts', { eager: true });

function deepMerge(a: Dict, b: Dict): Dict {
  const out: Dict = { ...a };
  for (const [k, v] of Object.entries(b)) {
    const cur = out[k];
    out[k] = typeof v === 'object' && typeof cur === 'object' ? deepMerge(cur, v) : v;
  }
  return out;
}

function withParts(locale: string, base: Dict): Dict {
  let d = base;
  for (const [path, mod] of Object.entries(partModules)) {
    if (path.endsWith(`.${locale}.ts`)) d = deepMerge(d, mod.default);
  }
  return d;
}

const en = withParts('en', enBase as unknown as Dict);

export type Dict = { [k: string]: string | Dict };
export type Locale = 'en' | 'es' | 'pt';

export const LOCALES: { id: Locale; name: string }[] = [
  { id: 'en', name: 'English' },
  { id: 'es', name: 'Español' },
  { id: 'pt', name: 'Português' },
];

const dicts: Partial<Record<Locale, Dict>> = { en };
const loaders: Record<Locale, () => Promise<{ default: Dict }>> = {
  en: async () => ({ default: en }),
  es: async () => ({ default: withParts('es', (await import('./es')).default as unknown as Dict) }),
  pt: async () => ({ default: withParts('pt', (await import('./pt')).default as unknown as Dict) }),
};

export const i18n = $state({ locale: 'en' as Locale, ready: 0 });

/** Narrow any stored/typed value to a supported locale ('es-CL' → 'es', unknown → 'en'). */
export function normalizeLocale(value: unknown): Locale {
  const id = String(value ?? '').slice(0, 2).toLowerCase();
  return LOCALES.some((l) => l.id === id) ? (id as Locale) : 'en';
}

export async function setLocale(value: Locale) {
  // settings.json is user-editable: an unknown value must not crash startup.
  const locale = normalizeLocale(value);
  if (!dicts[locale]) dicts[locale] = (await loaders[locale]()).default;
  i18n.locale = locale;
  i18n.ready++;
  document.documentElement.lang = locale;
}

/** First supported language in the OS/browser preference list, else English. */
export function detectLocale(): Locale {
  const prefs = navigator.languages?.length ? navigator.languages : [navigator.language || 'en'];
  for (const p of prefs) {
    const id = p.slice(0, 2).toLowerCase();
    if (LOCALES.some((l) => l.id === id)) return id as Locale;
  }
  return 'en';
}

/** Dot-path lookup that also matches object keys containing dots (setting ids). */
function lookup(d: Dict | undefined, key: string): string | Dict | undefined {
  const parts = key.split('.');
  let cur: string | Dict | undefined = d;
  let i = 0;
  while (i < parts.length) {
    if (cur === undefined || typeof cur === 'string') return undefined;
    let found = false;
    for (let j = parts.length; j > i; j--) {
      const k = parts.slice(i, j).join('.');
      if (k in cur) {
        cur = cur[k];
        i = j;
        found = true;
        break;
      }
    }
    if (!found) return undefined;
  }
  return cur;
}

const pluralRules = new Map<string, Intl.PluralRules>();

export function t(key: string, params?: Record<string, unknown>): string {
  // Touch reactive state so templates re-render on locale change.
  const locale = i18n.locale;
  void i18n.ready;
  let v = lookup(dicts[locale], key) ?? lookup(en, key);
  if (v === undefined) return key;
  if (typeof v !== 'string') {
    const count = Number(params?.count ?? 0);
    let pr = pluralRules.get(locale);
    if (!pr) pluralRules.set(locale, (pr = new Intl.PluralRules(locale)));
    const cat = pr.select(count);
    const alt = v[cat] ?? v.other ?? v.one;
    v = typeof alt === 'string' ? alt : key;
  }
  if (!params) return v;
  return v.replace(/\{(\w+)\}/g, (_, k) => (params[k] !== undefined ? String(params[k]) : `{${k}}`));
}

/** Locale-aware relative time ("3 min ago"). */
export function relTime(ts: number | string): string {
  const d = typeof ts === 'number' ? ts : Date.parse(ts);
  // RelativeTimeFormat.format throws a RangeError on NaN (missing/invalid timestamps).
  if (!Number.isFinite(d)) return '';
  const diff = (d - Date.now()) / 1000;
  const rtf = new Intl.RelativeTimeFormat(i18n.locale, { numeric: 'auto' });
  const abs = Math.abs(diff);
  if (abs < 45) return rtf.format(Math.round(diff), 'second');
  if (abs < 2700) return rtf.format(Math.round(diff / 60), 'minute');
  if (abs < 64800) return rtf.format(Math.round(diff / 3600), 'hour');
  if (abs < 518400) return rtf.format(Math.round(diff / 86400), 'day');
  return new Date(d).toLocaleDateString(i18n.locale, { month: 'short', day: 'numeric', year: 'numeric' });
}

export function fmtDate(d: string | number | Date, opts: Intl.DateTimeFormatOptions = { month: 'short', day: 'numeric' }): string {
  const date = d instanceof Date ? d : new Date(typeof d === 'string' && /^\d{4}-\d{2}-\d{2}$/.test(d) ? d + 'T00:00:00' : d);
  if (Number.isNaN(date.getTime())) return typeof d === 'string' ? d : '';
  return date.toLocaleDateString(i18n.locale, opts);
}
