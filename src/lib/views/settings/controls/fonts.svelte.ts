// Installed font families (loaded once, shared by every font control).

import { rpc } from '$lib/backend/rpc';

export const fonts = $state<{ list: string[]; loaded: boolean; loading: boolean }>({ list: [], loaded: false, loading: false });

export function loadFonts() {
  if (fonts.loaded || fonts.loading) return;
  fonts.loading = true;
  rpc<string[]>('fonts.list')
    .then((l) => (fonts.list = Array.isArray(l) ? [...new Set(l)].sort((a, b) => a.localeCompare(b)) : []))
    .catch(() => (fonts.list = []))
    .finally(() => {
      fonts.loaded = true;
      fonts.loading = false;
    });
}
