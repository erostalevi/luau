// App-local UI state persisted to ui-state.json (tabs, panels, zoom, expanded
// explorer nodes, recents). Never stored inside boards.

import { rpc } from '$lib/backend/rpc';

export const uiState = $state<{ data: Record<string, any>; loaded: boolean }>({ data: {}, loaded: false });
let timer: ReturnType<typeof setTimeout> | null = null;

export async function loadUiState() {
  const v = await rpc<Record<string, any> | null>('uiState.get').catch(() => null);
  uiState.data = v && typeof v === 'object' ? v : {};
  uiState.loaded = true;
}

export function saveUiState() {
  if (!uiState.loaded) return;
  if (timer) clearTimeout(timer);
  timer = setTimeout(() => {
    timer = null;
    void rpc('uiState.set', { value: $state.snapshot(uiState.data) });
  }, 400);
}

export function uiGet<T>(key: string, fallback: T): T {
  const v = uiState.data[key];
  return (v === undefined ? fallback : v) as T;
}

export function uiSet(key: string, value: unknown) {
  uiState.data[key] = value;
  saveUiState();
}

/** Push to a most-recent-first list, capped. */
export function uiPushRecent(key: string, item: Record<string, unknown>, match: (x: any) => boolean, cap = 30) {
  const list: any[] = (uiState.data[key] ?? []).filter((x: any) => !match(x));
  list.unshift({ ...item, at: Date.now() });
  uiState.data[key] = list.slice(0, cap);
  saveUiState();
}
