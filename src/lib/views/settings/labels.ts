// i18n lookups for settings with readable fallbacks for settings registered
// by features that did not ship translations.

import type { SettingDef } from '$lib/settings/schema';
import { t } from '$lib/i18n/index.svelte';
import { humanizeKey } from './search';

function tr(key: string, fallback: string): string {
  const v = t(key);
  return v === key ? fallback : v;
}

export function settingLabel(d: SettingDef): string {
  return tr(`settings.keys.${d.key}.label`, humanizeKey(d.key));
}

export function settingDesc(d: SettingDef): string {
  return tr(`settings.keys.${d.key}.desc`, '');
}

export function optionLabel(d: SettingDef, opt: string): string {
  return tr(`settings.keys.${d.key}.options.${opt}`, humanizeKey(opt));
}

export function categoryTitle(id: string): string {
  return tr(`settings.categories.${id}`, humanizeKey(id));
}

export function sourceLabel(source: string | undefined): string {
  if (!source || source === 'core') return '';
  return tr(`prefs.sources.${source}`, source);
}
