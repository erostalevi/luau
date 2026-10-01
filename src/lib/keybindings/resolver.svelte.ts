// Keybinding resolver: layered bindings (default → preset → extensions → user),
// two-step chords, when-clause evaluation. Installed in the capture phase so it
// sees keys before editors; unmatched keys fall through untouched.

import { rpc } from '$lib/backend/rpc';
import { evaluateWhen } from '$lib/commands/when';
import { ctxGet } from '$lib/commands/context.svelte';
import { getCommand, runCommand, isEnabled } from '$lib/commands/registry.svelte';
import { settings } from '$lib/settings/store.svelte';
import { BASE, PRESETS, type Keybinding } from './defaults';
import { eventToStroke, normalizeKey } from './keys';

export const kb = $state({
  user: [] as Keybinding[],
  extensions: [] as Keybinding[],
  /** First stroke of a pending chord (shown in the top bar). */
  pending: null as string | null,
  /** Recording mode (keybinding editor) suspends dispatch. */
  recording: false,
  version: 0,
});

let pendingTimer: ReturnType<typeof setTimeout> | null = null;

/** Effective list, lowest priority first (later entries win). */
export function effectiveBindings(): Keybinding[] {
  void kb.version;
  const preset = settings.get<string>('keyboard.preset') || 'vscode';
  const layers: Keybinding[] = [
    ...BASE.map((b) => ({ ...b, source: 'default' })),
    ...(PRESETS[preset] ?? []).map((b) => ({ ...b, source: `preset:${preset}` })),
    ...kb.extensions,
    ...kb.user.map((b) => ({ ...b, source: 'user' })),
  ];
  const out: Keybinding[] = [];
  for (const b of layers) {
    if (b.command.startsWith('-')) {
      // Removal rule: drop earlier bindings for command (and key if given).
      const cmd = b.command.slice(1);
      const key = b.key ? normalizeKey(b.key) : null;
      for (let i = out.length - 1; i >= 0; i--) {
        if (out[i].command === cmd && (!key || normalizeKey(out[i].key) === key)) out.splice(i, 1);
      }
      continue;
    }
    out.push({ ...b, key: normalizeKey(b.key) });
  }
  return out;
}

export function bindingsFor(command: string): Keybinding[] {
  return effectiveBindings().filter((b) => b.command === command);
}

/** Primary shortcut label for a command (used in menus/palette). */
export function primaryKey(command: string): string | null {
  const list = bindingsFor(command);
  if (!list.length) return null;
  // User bindings win; otherwise prefer the canonical modifier shortcut over
  // single keys and alternates (F1, n…).
  const user = list.filter((b) => b.source === 'user');
  if (user.length) return user[user.length - 1].key;
  const withMod = list.find((b) => b.key.split(' ')[0].includes('+') && !/^f\d+$/.test(b.key));
  return (withMod ?? list[0]).key;
}

export async function loadKeybindings() {
  const v = await rpc<Keybinding[] | null>('keybindings.get');
  kb.user = Array.isArray(v) ? v : [];
  kb.version++;
}

export async function saveUserKeybindings(list: Keybinding[]) {
  kb.user = list.map(({ key, command, when, args }) => ({ key, command, ...(when ? { when } : {}), ...(args !== undefined ? { args } : {}) }));
  kb.version++;
  await rpc('keybindings.set', { value: $state.snapshot(kb.user) });
}

function clearPending() {
  kb.pending = null;
  if (pendingTimer) clearTimeout(pendingTimer);
  pendingTimer = null;
}

function match(stroke: string): { exact?: Keybinding; prefix: boolean } {
  const all = effectiveBindings();
  const full = kb.pending ? `${kb.pending} ${stroke}` : stroke;
  let exact: Keybinding | undefined;
  let prefix = false;
  for (let i = all.length - 1; i >= 0; i--) {
    const b = all[i];
    if (!evaluateWhen(b.when, ctxGet)) continue;
    const cmd = getCommand(b.command);
    if (!cmd || !isEnabled(cmd)) continue;
    if (b.key === full) {
      if (!exact) exact = b;
    } else if (!kb.pending && b.key.startsWith(full + ' ')) {
      prefix = true;
    }
  }
  return { exact, prefix };
}

export function handleKeydown(e: KeyboardEvent) {
  if (kb.recording || e.isComposing || ctxGet('dragging')) return;
  // Dialogs, first-run, cheat sheet, color picker: they own the keyboard
  // (a Backspace behind a confirm must not delete the selected card).
  if (document.querySelector('[data-overlay]')) return;
  const stroke = eventToStroke(e);
  if (!stroke) return;
  const { exact, prefix } = match(stroke);
  if (kb.pending) {
    clearPending();
    if (exact) {
      e.preventDefault();
      e.stopPropagation();
      void runCommand(exact.command, exact.args);
    } else {
      // Unknown chord: swallow the second stroke like VS Code.
      e.preventDefault();
      e.stopPropagation();
    }
    return;
  }
  if (prefix && !exact) {
    e.preventDefault();
    e.stopPropagation();
    kb.pending = stroke;
    pendingTimer = setTimeout(clearPending, 2500);
    return;
  }
  if (exact) {
    e.preventDefault();
    e.stopPropagation();
    void runCommand(exact.command, exact.args);
  }
}

export function installKeybindings() {
  window.addEventListener('keydown', handleKeydown, { capture: true });
}
