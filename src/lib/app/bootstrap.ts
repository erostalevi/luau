// App startup: transport → settings → i18n → theme → keybindings → state.

import { initTransport, isTauri, onMenu, rpc } from '$lib/backend/rpc';
import type { AppInfo } from '$lib/backend/types';
import { loadSettings, settings } from '$lib/settings/store.svelte';
import { setLocale, detectLocale, type Locale } from '$lib/i18n/index.svelte';
import { initTheme } from '$lib/theme/theme.svelte';
import { installKeybindings, loadKeybindings } from '$lib/keybindings/resolver.svelte';
import { isMac, isWindows } from '$lib/keybindings/keys';
import { ctx, trackFocus } from '$lib/commands/context.svelte';
import { runCommand } from '$lib/commands/registry.svelte';
import { loadUiState } from '$lib/state/persist.svelte';
import { loadUi, ui } from '$lib/state/ui.svelte';
import { loadWorkspace, ws, openBoardTab, openDocTab, openSingleton, openTabOfKind, allTabs } from '$lib/state/workspace.svelte';
import { loadRegistry } from '$lib/state/registry.svelte';
import { initBoardEvents, openBoard } from '$lib/state/boards.svelte';
import { registerBuiltinCommands } from '$lib/commands/builtin';
import { openExternal } from '$lib/app/helpers';

export const app = { info: null as AppInfo | null };

/** Restore a tab moved here from another window (`?tab=<json>`). */
async function openCarriedTab(raw: string) {
  let t: { kind?: string; boardId?: string; cardId?: string; payload?: Record<string, unknown> };
  try {
    t = JSON.parse(raw);
  } catch {
    return;
  }
  const id = (v: unknown) => (typeof v === 'string' && /^[a-z][a-z0-9]{6}$/.test(v) ? v : undefined);
  const payload = t.payload && typeof t.payload === 'object' ? t.payload : undefined;
  switch (t.kind) {
    case 'board':
      if (id(t.boardId)) await openBoardTab(id(t.boardId)!);
      break;
    case 'doc':
      if (id(t.boardId) && id(t.cardId)) await openDocTab(id(t.boardId)!, id(t.cardId)!);
      break;
    case 'savedSearch':
    case 'summary':
      openTabOfKind(t.kind, payload);
      break;
    case 'start':
    case 'settings':
    case 'keybindings':
      openSingleton(t.kind, payload);
      break;
  }
}

export async function bootstrap() {
  await initTransport();
  const info = await rpc<AppInfo>('app.info');
  app.info = info;
  ctx.isMac = isMac;
  ctx.isWindows = isWindows;
  ctx.isLinux = !isMac && !isWindows;
  ctx.isWeb = !isTauri;
  document.documentElement.dataset.platform = isMac ? 'mac' : isWindows ? 'windows' : 'linux';

  await loadSettings();
  const lang = settings.get<string>('general.language');
  await setLocale(lang === 'auto' || !lang ? detectLocale() : (lang as Locale));
  initTheme();

  await Promise.all([loadKeybindings(), loadUiState()]);
  loadUi();
  initBoardEvents();
  await loadRegistry();
  registerBuiltinCommands();
  installKeybindings();
  trackFocus();
  onMenu((id) => void runCommand(id));
  // Links in rendered Markdown (summaries, embeds, previews) open in the
  // system browser; the webview itself never navigates away.
  document.addEventListener(
    'click',
    (e) => {
      const a = (e.target as Element | null)?.closest?.('a[href]');
      const href = a?.getAttribute('href') ?? '';
      if (!a || a.closest('[data-card]') || !/^(https?:|mailto:)/i.test(href)) return;
      e.preventDefault();
      void openExternal(href);
    },
    true,
  );

  loadWorkspace(info.window || 'main');
  const params = new URLSearchParams(location.search);
  const boardParam = params.get('board');
  const restore = settings.get<boolean>('general.restoreTabs');
  if (!restore || ws.windowLabel !== 'main') {
    for (const p of ws.panes) {
      p.tabs = [];
      p.active = null;
      p.mru = [];
    }
    ws.panes = [ws.panes[0]];
  }
  // Boards referenced by restored tabs are opened lazily; claim them now.
  for (const tab of allTabs()) {
    if ((tab.kind === 'board' || tab.kind === 'doc') && tab.boardId) {
      openBoard({ id: tab.boardId }).catch(() => undefined);
      if (tab.kind === 'board') void rpc('board.claim', { id: tab.boardId }).catch(() => undefined);
    }
  }
  if (boardParam) await openBoardTab(boardParam);
  const tabParam = params.get('tab');
  if (tabParam) await openCarriedTab(tabParam);
  if (!allTabs().length && settings.get('general.startPage') !== 'none') openSingleton('start');
  ui.firstRun = !settings.get<boolean>('general.firstRunDone');
  // Warm the editor bundle while idle so the first card opens instantly.
  const idle = (window as Window & { requestIdleCallback?: (cb: () => void) => number }).requestIdleCallback ?? ((cb: () => void) => setTimeout(cb, 1500));
  idle(() => void import('$lib/editor/cm/setup').catch(() => undefined));
  if (import.meta.env.DEV) {
    const [{ boards }, { dnd }, { selection }, { ws }, reg] = await Promise.all([
      import('$lib/state/boards.svelte'),
      import('$lib/board/dnd.svelte'),
      import('$lib/state/selection.svelte'),
      import('$lib/state/workspace.svelte'),
      import('$lib/commands/registry.svelte'),
    ]);
    (window as unknown as Record<string, unknown>).__luau = { boards, dnd, selection, ws, ui, settings, runCommand: reg.runCommand, rpc };
  }
}

export async function afterMount() {
  if (!isTauri) return;
  const { getCurrentWindow } = await import('@tauri-apps/api/window');
  const w = getCurrentWindow();
  await w.show();
  await w.setFocus();
  if (settings.get<boolean>('updates.checkOnStart')) {
    setTimeout(() => void runCommand('app.checkForUpdates', { silent: true }), 4000);
  }
}
