// Side effects for settings that need more than a stored value
// (shared by the Settings view and the first-launch setup).

import { settings } from '$lib/settings/store.svelte';
import { isTauri } from '$lib/backend/rpc';
import { setLocale, detectLocale, type Locale } from '$lib/i18n/index.svelte';
import { toast } from '$lib/state/toasts.svelte';
import { t } from '$lib/i18n/index.svelte';

async function applyAutostart(on: boolean) {
  if (!isTauri) return;
  try {
    const al = await import('@tauri-apps/plugin-autostart');
    if (on) await al.enable();
    else await al.disable();
    const actual = await al.isEnabled();
    if (actual !== on) settings.set('app.launchAtLogin', actual);
  } catch (e) {
    settings.set('app.launchAtLogin', !on);
    toast.warn(t('prefs.settings.autostartFailed', { message: String((e as Error)?.message ?? e) }));
  }
}

/** Set a setting and run its side effect (locale, autostart). */
export function setSetting(key: string, value: unknown) {
  settings.set(key, value);
  if (key === 'general.language') {
    const v = String(value);
    void setLocale(v === 'auto' || !v ? detectLocale() : (v as Locale));
  } else if (key === 'app.launchAtLogin') {
    void applyAutostart(Boolean(value));
  }
}

export function resetSetting(key: string) {
  const def = settings.def(key);
  if (def) setSetting(key, def.default);
  settings.reset(key);
}
