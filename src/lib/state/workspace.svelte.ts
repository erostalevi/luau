// Tabs and split panes for this window. A board can be open in only one tab
// across the whole app (enforced with backend claims for multi-window).

import { rpc } from '$lib/backend/rpc';
import { ctx } from '$lib/commands/context.svelte';
import { uiGet, uiSet, uiPushRecent } from './persist.svelte';
import { openBoard, boards } from './boards.svelte';
import { toast } from './toasts.svelte';
import { t } from '$lib/i18n/index.svelte';

export type TabKind = 'board' | 'doc' | 'start' | 'settings' | 'keybindings' | 'savedSearch' | 'summary';

export interface Tab {
  id: string;
  kind: TabKind;
  boardId?: string;
  cardId?: string;
  pinned?: boolean;
  /** Extra payload (e.g. saved search query). */
  payload?: Record<string, unknown>;
}

export interface Pane {
  id: string;
  tabs: Tab[];
  active: string | null;
  mru: string[];
}

const rid = () => Math.random().toString(36).slice(2, 9);

export const ws = $state({
  windowLabel: 'main',
  panes: [{ id: 'p1', tabs: [], active: null, mru: [] }] as Pane[],
  activePane: 'p1',
  closed: [] as Tab[],
  loaded: false,
});

export function activePane(): Pane {
  return ws.panes.find((p) => p.id === ws.activePane) ?? ws.panes[0];
}

export function activeTab(): Tab | null {
  const p = activePane();
  return p.tabs.find((tb) => tb.id === p.active) ?? null;
}

export function allTabs(): Tab[] {
  return ws.panes.flatMap((p) => p.tabs);
}

function syncCtx() {
  const tb = activeTab();
  ctx.tabKind = tb?.kind ?? '';
  const b = tb?.boardId ? boards.get(tb.boardId) : undefined;
  ctx.boardType = b?.kind ?? '';
  ctx.boardReadOnly = !!b?.header.readOnly;
}

export function persistWorkspace() {
  const all = uiGet<Record<string, unknown>>('windows', {});
  all[ws.windowLabel] = {
    panes: ws.panes.map((p) => ({ ...p, tabs: p.tabs.map((x) => ({ ...x })), mru: [...p.mru] })),
    activePane: ws.activePane,
  };
  uiSet('windows', all);
  syncCtx();
}

export function loadWorkspace(label: string) {
  ws.windowLabel = label;
  const saved = uiGet<Record<string, any>>('windows', {})[label];
  if (saved?.panes?.length) {
    ws.panes = saved.panes;
    ws.activePane = saved.activePane ?? saved.panes[0].id;
  }
  ws.loaded = true;
  syncCtx();
}

export function focusTab(tabId: string) {
  for (const p of ws.panes) {
    if (p.tabs.some((x) => x.id === tabId)) {
      p.active = tabId;
      p.mru = [tabId, ...p.mru.filter((x) => x !== tabId)];
      ws.activePane = p.id;
      persistWorkspace();
      return;
    }
  }
}

function findTab(pred: (tb: Tab) => boolean): Tab | undefined {
  return allTabs().find(pred);
}

function addTab(tab: Tab, paneId?: string): Tab {
  const pane = ws.panes.find((p) => p.id === (paneId ?? ws.activePane)) ?? activePane();
  const i = pane.tabs.findIndex((x) => x.id === pane.active);
  pane.tabs.splice(i >= 0 ? i + 1 : pane.tabs.length, 0, tab);
  focusTab(tab.id);
  return tab;
}

/** Open (or focus) a board tab. */
export async function openBoardTab(boardId: string, paneId?: string): Promise<Tab | null> {
  const existing = findTab((x) => x.kind === 'board' && x.boardId === boardId);
  if (existing) {
    focusTab(existing.id);
    return existing;
  }
  const owner = await rpc<string | null>('board.claim', { id: boardId }).catch(() => null);
  if (owner && owner !== ws.windowLabel) {
    await rpc('window.focus', { label: owner });
    toast.info(t('tabs.openInOtherWindow'));
    return null;
  }
  try {
    const m = await openBoard({ id: boardId });
    uiPushRecent('recentBoards', { id: boardId, name: m.header.name }, (x) => x.id === boardId);
  } catch (e) {
    toast.error(t('errors.openBoard', { message: (e as Error).message }));
    return null;
  }
  return addTab({ id: rid(), kind: 'board', boardId }, paneId);
}

/** Open (or focus) a document tab (files boards only). */
export async function openDocTab(boardId: string, cardId: string, paneId?: string): Promise<Tab | null> {
  const existing = findTab((x) => x.kind === 'doc' && x.boardId === boardId && x.cardId === cardId);
  if (existing) {
    focusTab(existing.id);
    return existing;
  }
  await openBoard({ id: boardId });
  uiPushRecent('recentDocs', { boardId, cardId }, (x) => x.boardId === boardId && x.cardId === cardId);
  return addTab({ id: rid(), kind: 'doc', boardId, cardId }, paneId);
}

/** Singleton tabs (start, settings, keybindings). */
export function openSingleton(kind: TabKind, payload?: Record<string, unknown>): Tab {
  const existing = findTab((x) => x.kind === kind);
  if (existing) {
    if (payload) existing.payload = payload;
    focusTab(existing.id);
    return existing;
  }
  return addTab({ id: rid(), kind, payload });
}

export function openTabOfKind(kind: TabKind, payload?: Record<string, unknown>): Tab {
  return addTab({ id: rid(), kind, payload });
}

export function closeTab(tabId: string, force = false) {
  for (const p of ws.panes) {
    const i = p.tabs.findIndex((x) => x.id === tabId);
    if (i < 0) continue;
    const tab = p.tabs[i];
    if (tab.pinned && !force) return;
    p.tabs.splice(i, 1);
    p.mru = p.mru.filter((x) => x !== tabId);
    ws.closed.push({ ...tab });
    if (ws.closed.length > 30) ws.closed.shift();
    if (tab.kind === 'board' && tab.boardId && !findTab((x) => x.kind === 'board' && x.boardId === tab.boardId)) {
      void rpc('board.release', { id: tab.boardId });
    }
    if (p.active === tabId) p.active = p.mru[0] ?? p.tabs[Math.min(i, p.tabs.length - 1)]?.id ?? null;
    if (p.tabs.length === 0 && ws.panes.length > 1) {
      ws.panes = ws.panes.filter((x) => x.id !== p.id);
      if (ws.activePane === p.id) ws.activePane = ws.panes[0].id;
    }
    persistWorkspace();
    return;
  }
}

export async function reopenClosed() {
  const tab = ws.closed.pop();
  if (!tab) return;
  if (tab.kind === 'board' && tab.boardId) await openBoardTab(tab.boardId);
  else if (tab.kind === 'doc' && tab.boardId && tab.cardId) await openDocTab(tab.boardId, tab.cardId);
  else addTab({ ...tab, id: rid() });
}

export function cycleTab(dir: 1 | -1) {
  const p = activePane();
  if (!p.tabs.length) return;
  const i = p.tabs.findIndex((x) => x.id === p.active);
  const next = p.tabs[(i + dir + p.tabs.length) % p.tabs.length];
  focusTab(next.id);
}

export function gotoTab(n: number) {
  const p = activePane();
  const tab = n >= 8 ? p.tabs[p.tabs.length - 1] : p.tabs[n];
  if (tab) focusTab(tab.id);
}

export function moveTab(tabId: string, toPane: string, index: number) {
  let tab: Tab | undefined;
  for (const p of ws.panes) {
    const i = p.tabs.findIndex((x) => x.id === tabId);
    if (i >= 0) {
      [tab] = p.tabs.splice(i, 1);
      p.mru = p.mru.filter((x) => x !== tabId);
      if (p.active === tabId) p.active = p.mru[0] ?? p.tabs[0]?.id ?? null;
    }
  }
  if (!tab) return;
  const target = ws.panes.find((p) => p.id === toPane) ?? activePane();
  target.tabs.splice(Math.min(index, target.tabs.length), 0, tab);
  ws.panes = ws.panes.filter((p) => p.tabs.length > 0 || p.id === target.id);
  focusTab(tab.id);
}

export function splitRight() {
  const cur = activePane();
  const tab = cur.tabs.find((x) => x.id === cur.active);
  const pane: Pane = { id: 'p' + rid(), tabs: [], active: null, mru: [] };
  const i = ws.panes.findIndex((p) => p.id === cur.id);
  ws.panes.splice(i + 1, 0, pane);
  // Boards can only be open once, so the new pane starts on the start page
  // unless the current tab is a movable singleton/doc.
  if (tab && tab.kind !== 'board' && cur.tabs.length > 1) moveTab(tab.id, pane.id, 0);
  else {
    ws.activePane = pane.id;
    openTabOfKind('start');
  }
  persistWorkspace();
}

export function togglePin(tabId: string) {
  const tab = findTab((x) => x.id === tabId);
  if (!tab) return;
  tab.pinned = !tab.pinned;
  for (const p of ws.panes) p.tabs.sort((a, b) => Number(!!b.pinned) - Number(!!a.pinned));
  persistWorkspace();
}

export function closeOthers(tabId: string) {
  const p = ws.panes.find((x) => x.tabs.some((tb) => tb.id === tabId));
  if (!p) return;
  for (const tb of [...p.tabs]) if (tb.id !== tabId && !tb.pinned) closeTab(tb.id);
}

/** Replace board ids of tabs after a board was deleted/renamed externally. */
export function closeTabsOfBoard(boardId: string) {
  for (const tb of allTabs()) if (tb.boardId === boardId) closeTab(tb.id, true);
}

export { syncCtx as syncWorkspaceCtx };
