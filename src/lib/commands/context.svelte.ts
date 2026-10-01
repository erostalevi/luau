// Context keys used by `when` clauses. Components update these as focus moves.

import { settings } from '$lib/settings/store.svelte';

export const ctx = $state<Record<string, unknown>>({
  boardFocus: false,
  editorFocus: false,
  editorOpen: false,
  inputFocus: false,
  /** A button/link/select has focus: Enter/Space/Tab must keep their native meaning. */
  buttonFocus: false,
  /** A drag is in progress: shortcuts are suspended (Escape cancels the drag). */
  dragging: false,
  paletteOpen: false,
  modalOpen: false,
  explorerFocus: false,
  cardSelected: false,
  laneFocus: false,
  multiSelect: false,
  boardType: '',
  tabKind: '',
  editorHasSelection: false,
  cardIsRemote: false,
  boardReadOnly: false,
  isMac: false,
  isWindows: false,
  isLinux: false,
  isWeb: false,
});

export function ctxGet(key: string): unknown {
  if (key.startsWith('config.')) return settings.get(key.slice(7));
  return ctx[key];
}

/** Track DOM focus to maintain inputFocus/editorFocus automatically. */
export function trackFocus() {
  const update = () => {
    const el = document.activeElement as HTMLElement | null;
    const inEditor = !!el?.closest('.cm-editor');
    const isInput = !!el && (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA' || el.isContentEditable) && !inEditor;
    ctx.editorFocus = inEditor;
    ctx.inputFocus = isInput || inEditor;
    ctx.buttonFocus = !!el && !el.closest('[data-card]') && (el.tagName === 'BUTTON' || el.tagName === 'A' || el.tagName === 'SELECT' || el.getAttribute('role') === 'button');
  };
  document.addEventListener('focusin', update);
  document.addEventListener('focusout', () => setTimeout(update, 0));
  update();
}
