// Applies appearance settings to the document: theme, palette, fonts,
// transparency, motion, zoom.

import { settings } from '$lib/settings/store.svelte';
import { isTauri } from '$lib/backend/rpc';
import { hexToRgb, shade, withAlpha, lightness } from './color';

export const theme = $state({ dark: false, glass: false });

let media: MediaQueryList | null = null;

function resolveDark(): boolean {
  const mode = settings.get<string>('appearance.theme');
  if (mode === 'dark') return true;
  if (mode === 'light') return false;
  return media?.matches ?? false;
}

function fontStack(value: string, fallback: string): string {
  if (!value || value === 'default') return fallback;
  if (value === 'system') return "-apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, Ubuntu, sans-serif";
  return `'${value.replace(/'/g, '')}', ${fallback}`;
}

export function applyTheme() {
  const root = document.documentElement;
  const dark = resolveDark();
  theme.dark = dark;
  root.dataset.theme = dark ? 'dark' : 'light';
  root.dataset.transparency = settings.get('appearance.transparency') === 'reduced' ? 'reduced' : 'full';
  root.dataset.motion = String(settings.get('appearance.motion') ?? 'full');
  root.dataset.density = String(settings.get('appearance.density') ?? 'comfortable');
  const glass = isTauri && settings.get('appearance.transparency') !== 'reduced' && !/Linux/i.test(navigator.userAgent);
  theme.glass = glass;
  root.dataset.glass = glass ? 'on' : 'off';

  const p = settings.get<string>('appearance.primaryColor') || '#ef8a7c';
  const sec = settings.get<string>('appearance.secondaryColor') || '#fdeadc';
  const st = root.style;
  if (dark) {
    st.setProperty('--primary', shade(p, 0.72, 0.9));
    st.setProperty('--primary-strong', shade(p, 0.8, 0.8));
    st.setProperty('--primary-soft', withAlpha(shade(p, 0.72), 0.14));
    st.setProperty('--primary-softer', withAlpha(shade(p, 0.72), 0.07));
    st.setProperty('--primary-ring', withAlpha(shade(p, 0.72), 0.45));
    st.setProperty('--on-primary', shade(p, 0.18, 0.6));
    st.setProperty('--secondary', withAlpha(shade(sec, 0.8, 3), 0.12));
    st.setProperty('--secondary-strong', withAlpha(shade(sec, 0.8, 3), 0.24));
    st.setProperty('--secondary-ink', shade(sec, 0.86, 3));
  } else {
    st.setProperty('--primary', shade(p, 0.66));
    st.setProperty('--primary-strong', shade(p, 0.55, 1.1));
    st.setProperty('--primary-soft', shade(p, 0.955, 0.28));
    st.setProperty('--primary-softer', shade(p, 0.978, 0.16));
    st.setProperty('--primary-ring', withAlpha(shade(p, 0.66), 0.35));
    st.setProperty('--on-primary', lightness(p) > 0.8 ? shade(p, 0.25) : '#ffffff');
    const [r, g, b] = hexToRgb(sec);
    st.setProperty('--secondary', `rgb(${r} ${g} ${b})`);
    st.setProperty('--secondary-strong', shade(sec, Math.max(0.5, lightness(sec) - 0.08), 2));
    st.setProperty('--secondary-ink', shade(sec, 0.45, 3));
  }
  st.setProperty('--font-ui', fontStack(settings.get<string>('appearance.uiFont'), "'Inter Variable', Inter, system-ui, sans-serif"));
  st.setProperty('--font-editor', fontStack(settings.get<string>('appearance.editorFont'), 'var(--font-ui)'));
  st.setProperty('--font-mono', fontStack(settings.get<string>('appearance.monoFont'), "'JetBrains Mono Variable', ui-monospace, Menlo, monospace"));
  st.setProperty('--fs-md', `${settings.get<number>('appearance.fontSize')}px`);
  st.setProperty('--lane-w', `${settings.get<number>('board.laneWidth')}px`);
  st.setProperty('--card-w', `${settings.get<number>('board.cardWidth')}px`);
  st.setProperty('--editor-w', `${settings.get<number>('editor.width')}px`);
  st.setProperty('--editor-fs', `${settings.get<number>('editor.fontSize')}px`);
  st.setProperty('--editor-lh', `${settings.get<number>('editor.lineHeight')}`);
  const zoom = settings.get<number>('appearance.zoom') || 1;
  st.setProperty('zoom', zoom === 1 ? '' : String(zoom));
}

/** Call once; re-applies whenever settings or OS theme change. */
export function initTheme() {
  media = window.matchMedia('(prefers-color-scheme: dark)');
  media.addEventListener('change', applyTheme);
  $effect.root(() => {
    $effect(() => {
      // Track every appearance-related setting.
      for (const k of [
        'appearance.theme',
        'appearance.primaryColor',
        'appearance.secondaryColor',
        'appearance.uiFont',
        'appearance.editorFont',
        'appearance.monoFont',
        'appearance.fontSize',
        'appearance.transparency',
        'appearance.motion',
        'appearance.density',
        'appearance.zoom',
        'board.laneWidth',
        'board.cardWidth',
        'editor.width',
        'editor.fontSize',
        'editor.lineHeight',
      ])
        settings.get(k);
      applyTheme();
    });
  });
}
