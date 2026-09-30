// Key normalization. Bindings are strings like "mod+shift+p" or chords
// "mod+k mod+s". Keys are recorded by physical code so they work on any layout.

export const isMac = typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent);
export const isWindows = typeof navigator !== 'undefined' && /Win/.test(navigator.platform || navigator.userAgent);

const CODE_MAP: Record<string, string> = {
  Space: 'space',
  Enter: 'enter',
  NumpadEnter: 'enter',
  Escape: 'escape',
  Backspace: 'backspace',
  Delete: 'delete',
  Tab: 'tab',
  ArrowUp: 'up',
  ArrowDown: 'down',
  ArrowLeft: 'left',
  ArrowRight: 'right',
  Home: 'home',
  End: 'end',
  PageUp: 'pageup',
  PageDown: 'pagedown',
  Insert: 'insert',
  Comma: ',',
  Period: '.',
  Slash: '/',
  Backslash: '\\',
  BracketLeft: '[',
  BracketRight: ']',
  Minus: '-',
  Equal: '=',
  Backquote: '`',
  Quote: "'",
  Semicolon: ';',
  IntlBackslash: '\\',
  NumpadAdd: '=',
  NumpadSubtract: '-',
};

const MODIFIER_CODES = new Set(['ShiftLeft', 'ShiftRight', 'ControlLeft', 'ControlRight', 'AltLeft', 'AltRight', 'MetaLeft', 'MetaRight', 'OSLeft', 'OSRight']);

export function keyName(e: KeyboardEvent): string | null {
  if (MODIFIER_CODES.has(e.code)) return null;
  if (CODE_MAP[e.code]) return CODE_MAP[e.code];
  if (/^Key[A-Z]$/.test(e.code)) return e.code.slice(3).toLowerCase();
  if (/^Digit\d$/.test(e.code)) return e.code.slice(5);
  if (/^Numpad\d$/.test(e.code)) return e.code.slice(6);
  if (/^F\d{1,2}$/.test(e.code)) return e.code.toLowerCase();
  // Fallback for unusual layouts/devices.
  const k = e.key?.toLowerCase();
  return k && k.length === 1 ? k : null;
}

/** Normalize an event into "ctrl+alt+shift+cmd+key" (fixed modifier order). */
export function eventToStroke(e: KeyboardEvent): string | null {
  const k = keyName(e);
  if (!k) return null;
  const parts: string[] = [];
  if (e.ctrlKey) parts.push('ctrl');
  if (e.altKey) parts.push('alt');
  if (e.shiftKey) parts.push('shift');
  if (e.metaKey) parts.push('cmd');
  parts.push(k);
  return parts.join('+');
}

/** Resolve "mod" and sort modifiers so strings compare exactly. */
export function normalizeStroke(stroke: string): string {
  const parts = stroke.toLowerCase().split('+').filter(Boolean);
  const key = parts.pop() ?? '';
  const mods = new Set(
    parts.map((p) => {
      if (p === 'mod') return isMac ? 'cmd' : 'ctrl';
      if (p === 'meta' || p === 'super' || p === 'win' || p === 'command') return 'cmd';
      if (p === 'option' || p === 'opt') return 'alt';
      if (p === 'control') return 'ctrl';
      return p;
    }),
  );
  const out = ['ctrl', 'alt', 'shift', 'cmd'].filter((m) => mods.has(m));
  out.push(key === 'plus' ? '=' : key);
  return out.join('+');
}

export function normalizeKey(key: string): string {
  return key.trim().split(/\s+/).map(normalizeStroke).join(' ');
}

const SYMBOLS_MAC: Record<string, string> = { ctrl: '⌃', alt: '⌥', shift: '⇧', cmd: '⌘' };
const NAMES: Record<string, string> = {
  up: '↑',
  down: '↓',
  left: '←',
  right: '→',
  enter: '↵',
  escape: 'Esc',
  backspace: '⌫',
  delete: 'Del',
  tab: 'Tab',
  space: 'Space',
  pageup: 'PgUp',
  pagedown: 'PgDn',
};

/** Human label per platform: "⌘⇧P" on macOS, "Ctrl+Shift+P" elsewhere. */
export function formatKey(key: string): string[] {
  return normalizeKey(key)
    .split(' ')
    .map((stroke) => {
      const parts = stroke.split('+');
      const k = parts.pop()!;
      const kl = NAMES[k] ?? (k.length === 1 ? k.toUpperCase() : k.toUpperCase());
      if (isMac) return parts.map((p) => SYMBOLS_MAC[p]).join('') + kl;
      return [...parts.map((p) => (p === 'cmd' ? 'Win' : p[0].toUpperCase() + p.slice(1))), kl].join('+');
    });
}
