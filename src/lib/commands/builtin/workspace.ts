import { PanelLeft, FolderTree, Search, History, Plug, Blocks, Columns2, AppWindow, ZoomIn, ZoomOut, Undo2, Redo2, X, RotateCcw } from '@lucide/svelte';
import type { Command } from '../registry.svelte';
import { rpc } from '$lib/backend/rpc';
import { ui, showSection, persistUi, boardZoom, setBoardZoom } from '$lib/state/ui.svelte';
import { activeTab, closeTab, reopenClosed, cycleTab, gotoTab, splitRight, allTabs, ws } from '$lib/state/workspace.svelte';
import { undo, redo } from '$lib/state/boards.svelte';
import { settings } from '$lib/settings/store.svelte';
import { activeBoard, tabBoard } from '$lib/app/helpers';
import { toast } from '$lib/state/toasts.svelte';
import { t } from '$lib/i18n/index.svelte';

function zoom(delta: number | null) {
  const b = tabBoard();
  if (b && activeTab()?.kind === 'board') {
    setBoardZoom(b.id, delta === null ? 1 : boardZoom(b.id) + delta);
    return;
  }
  const z = settings.get<number>('appearance.zoom') || 1;
  settings.set('appearance.zoom', delta === null ? 1 : Math.round(Math.min(1.8, Math.max(0.6, z + delta)) * 100) / 100);
}

async function undoRedo(which: 'undo' | 'redo') {
  const b = activeBoard();
  if (!b) return;
  const r = which === 'undo' ? await undo(b.id) : await redo(b.id);
  if (r?.done && r.label) toast.info(t(which === 'undo' ? 'toasts.undone' : 'toasts.redone', { label: r.label }), { timeout: 1600 });
}

export const workspaceCommands: Command[] = [
  { id: 'panel.toggle', title: 'commands.panel.toggle', category: 'view', icon: PanelLeft, run: () => ((ui.left.visible = !ui.left.visible), persistUi()) },
  { id: 'panel.explorer', title: 'commands.panel.explorer', category: 'view', icon: FolderTree, run: () => showSection('explorer') },
  {
    id: 'panel.search',
    title: 'commands.panel.search',
    category: 'view',
    icon: Search,
    run: () => {
      ui.left.visible = true;
      ui.left.section = 'search';
      ui.searchFocus++;
      persistUi();
    },
  },
  { id: 'search.focus', title: 'commands.search.focus', category: 'search', hidden: true, run: () => ((ui.left.visible = true), (ui.left.section = 'search'), ui.searchFocus++) },
  { id: 'panel.history', title: 'commands.panel.history', category: 'view', icon: History, run: () => showSection('history') },
  { id: 'panel.integrations', title: 'commands.panel.integrations', category: 'view', icon: Plug, run: () => showSection('integrations') },
  { id: 'panel.extensions', title: 'commands.panel.extensions', category: 'view', icon: Blocks, run: () => showSection('extensions') },
  { id: 'tab.close', title: 'commands.tab.close', category: 'tabs', icon: X, run: () => { const tab = activeTab(); if (tab) closeTab(tab.id, true); } },
  { id: 'tab.reopen', title: 'commands.tab.reopen', category: 'tabs', icon: RotateCcw, run: () => reopenClosed() },
  { id: 'tab.next', title: 'commands.tab.next', category: 'tabs', run: () => cycleTab(1) },
  { id: 'tab.previous', title: 'commands.tab.previous', category: 'tabs', run: () => cycleTab(-1) },
  { id: 'tab.goto', title: 'commands.tab.goto', category: 'tabs', hidden: true, run: (n?: number) => gotoTab(Number(n ?? 0)) },
  {
    id: 'tab.closeAll',
    title: 'commands.tab.closeAll',
    category: 'tabs',
    run: () => {
      for (const tb of allTabs()) if (!tb.pinned) closeTab(tb.id);
    },
  },
  { id: 'view.splitRight', title: 'commands.view.splitRight', category: 'view', icon: Columns2, run: () => splitRight() },
  {
    id: 'window.new',
    title: 'commands.window.new',
    category: 'view',
    icon: AppWindow,
    run: () => rpc('window.new', {}),
  },
  {
    id: 'tab.moveToNewWindow',
    title: 'commands.tab.moveToNewWindow',
    category: 'tabs',
    run: async (tabId?: string) => {
      const tab = tabId ? allTabs().find((x) => x.id === tabId) : activeTab();
      if (!tab) return;
      const query = tab.kind === 'board' && tab.boardId ? `?board=${encodeURIComponent(tab.boardId)}` : '';
      closeTab(tab.id, true);
      if (tab.kind === 'board' && tab.boardId) await rpc('board.release', { id: tab.boardId });
      await rpc('window.new', { query });
      void ws;
    },
  },
  { id: 'view.zoomIn', title: 'commands.view.zoomIn', category: 'view', icon: ZoomIn, run: () => zoom(0.1) },
  { id: 'view.zoomOut', title: 'commands.view.zoomOut', category: 'view', icon: ZoomOut, run: () => zoom(-0.1) },
  { id: 'view.zoomReset', title: 'commands.view.zoomReset', category: 'view', run: () => zoom(null) },
  { id: 'edit.undo', title: 'commands.edit.undo', category: 'edit', icon: Undo2, run: () => undoRedo('undo') },
  { id: 'edit.redo', title: 'commands.edit.redo', category: 'edit', icon: Redo2, run: () => undoRedo('redo') },
];
