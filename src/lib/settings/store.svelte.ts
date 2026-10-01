// Reactive settings store backed by settings.json (via the backend).

import { rpc } from '$lib/backend/rpc';
import { CORE_SETTINGS, type SettingDef } from './schema';

const defs = new Map<string, SettingDef>(CORE_SETTINGS.map((d) => [d.key, d]));
const values = $state<Record<string, unknown>>({});
let saveTimer: ReturnType<typeof setTimeout> | null = null;

export const settings = {
  get defs(): SettingDef[] {
    return [...defs.values()];
  },
  def(key: string): SettingDef | undefined {
    return defs.get(key);
  },
  /** Register settings contributed by an extension or module. */
  register(extra: SettingDef[]) {
    for (const d of extra) defs.set(d.key, d);
  },
  get<T = unknown>(key: string): T {
    const v = values[key];
    return (v === undefined ? defs.get(key)?.default : v) as T;
  },
  isModified(key: string): boolean {
    return values[key] !== undefined && JSON.stringify(values[key]) !== JSON.stringify(defs.get(key)?.default);
  },
  set(key: string, value: unknown) {
    const def = defs.get(key);
    if (def && JSON.stringify(value) === JSON.stringify(def.default)) {
      delete values[key];
    } else {
      values[key] = value;
    }
    schedule();
  },
  reset(key: string) {
    delete values[key];
    schedule();
  },
  resetAll() {
    for (const k of Object.keys(values)) delete values[k];
    schedule();
  },
  toggle(key: string) {
    this.set(key, !this.get<boolean>(key));
  },
  /** Raw user values (for export). */
  all(): Record<string, unknown> {
    return $state.snapshot(values) as Record<string, unknown>;
  },
  replaceAll(v: Record<string, unknown>) {
    for (const k of Object.keys(values)) delete values[k];
    Object.assign(values, v);
    schedule();
  },
};

function schedule() {
  if (saveTimer) clearTimeout(saveTimer);
  saveTimer = setTimeout(() => {
    saveTimer = null;
    void rpc('settings.set', { value: $state.snapshot(values) });
  }, 250);
}

export async function loadSettings() {
  const v = await rpc<Record<string, unknown> | null>('settings.get');
  if (v && typeof v === 'object') Object.assign(values, v);
}
