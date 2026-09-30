// Settings search (VS Code style). Free words must all match the label,
// description or key. Filters:
//   @modified            only settings changed from their default
//   @ai                  shorthand for @category:ai (any known category id works: @editor)
//   @category:<id>       one category
//   @ext:<id> / @source:<id>   settings contributed by a feature/extension
//   @id:<key>            exact key (prefix match with a trailing *)

import type { SettingDef } from '$lib/settings/schema';

export interface SettingsQuery {
  words: string[];
  modified: boolean;
  categories: string[];
  sources: string[];
  ids: string[];
}

export interface SettingText {
  label: string;
  desc: string;
}

export function parseSettingsQuery(q: string, knownCategories: readonly string[] = []): SettingsQuery {
  const out: SettingsQuery = { words: [], modified: false, categories: [], sources: [], ids: [] };
  const known = new Set(knownCategories.map((c) => c.toLowerCase()));
  for (const raw of q.trim().split(/\s+/).filter(Boolean)) {
    const w = raw.toLowerCase();
    if (w === '@modified') out.modified = true;
    else if (w.startsWith('@category:')) out.categories.push(w.slice(10));
    else if (w.startsWith('@ext:')) out.sources.push(w.slice(5));
    else if (w.startsWith('@source:')) out.sources.push(w.slice(8));
    else if (w.startsWith('@id:')) out.ids.push(raw.slice(4));
    else if (w === '@ai') out.categories.push('ai');
    else if (w.startsWith('@') && known.has(w.slice(1))) out.categories.push(w.slice(1));
    else out.words.push(w);
  }
  return out;
}

export function isFiltering(q: SettingsQuery): boolean {
  return !!(q.words.length || q.modified || q.categories.length || q.sources.length || q.ids.length);
}

export function filterSettings(defs: SettingDef[], q: SettingsQuery, text: (d: SettingDef) => SettingText, isModified: (key: string) => boolean): SettingDef[] {
  return defs.filter((d) => {
    if (d.hidden) return false;
    if (q.modified && !isModified(d.key)) return false;
    if (q.categories.length && !q.categories.includes(String(d.category).toLowerCase())) return false;
    if (q.sources.length && !q.sources.includes((d.source ?? 'core').toLowerCase())) return false;
    if (q.ids.length && !q.ids.some((id) => (id.endsWith('*') ? d.key.startsWith(id.slice(0, -1)) : d.key === id))) return false;
    if (!q.words.length) return true;
    const tx = text(d);
    const hay = `${tx.label} ${tx.desc} ${d.key} ${(d.options ?? []).join(' ')}`.toLowerCase();
    return q.words.every((w) => hay.includes(w));
  });
}

/** Categories in display order: known ones first, then any registered by features. */
export function categoryOrder(defs: SettingDef[], known: readonly string[]): string[] {
  const out = [...known];
  for (const d of defs) if (!d.hidden && !out.includes(d.category)) out.push(d.category);
  return out;
}

/** "editor.autoPair" → "Auto pair" (fallback when no translation exists). */
export function humanizeKey(key: string): string {
  const last = key.split('.').pop() ?? key;
  const words = last
    .replace(/([a-z0-9])([A-Z])/g, '$1 $2')
    .replace(/[_-]+/g, ' ')
    .toLowerCase()
    .trim();
  return words.charAt(0).toUpperCase() + words.slice(1);
}
