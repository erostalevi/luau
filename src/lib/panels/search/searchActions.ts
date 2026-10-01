// Search-panel actions shared by the panel UI and the search commands.

import { ExternalLink, PanelRight, LocateFixed, Link, Pencil, Trash2, LayoutGrid, Copy } from '@lucide/svelte';
import type { SearchHit } from '$lib/backend/types';
import type { MenuItem } from '$lib/state/menu.svelte';
import { ui, persistUi } from '$lib/state/ui.svelte';
import { allTabs, focusTab, openTabOfKind, openDocTab, persistWorkspace } from '$lib/state/workspace.svelte';
import { boards, openBoard } from '$lib/state/boards.svelte';
import { inputBox, quickPick } from '$lib/quickinput/qi.svelte';
import { toast } from '$lib/state/toasts.svelte';
import { settings } from '$lib/settings/store.svelte';
import { openCard } from '$lib/app/open';
import { copyText } from '$lib/app/helpers';
import { cardMenu } from '$lib/board/cardMenu';
import { t } from '$lib/i18n/index.svelte';
import { revealCard, openRow } from '$lib/panels/explorer/explorerActions';
import {
  search,
  loadSaved,
  addSaved,
  renameSaved,
  removeSaved,
  restoreSaved,
  updateSaved,
  hasContent,
  type SavedSearch,
  type GroupBy,
} from './searchState.svelte';
import { effectiveCase, withCase } from './sync';

export function focusSearch(q?: string) {
  if (q !== undefined) ui.searchQuery = q;
  ui.left.visible = true;
  ui.left.section = 'search';
  ui.searchFocus++;
  persistUi();
}

export function clearSearch() {
  search.q = '';
  focusSearch();
}

export function toggleCase() {
  const on = effectiveCase(search.q, settings.get<boolean>('search.caseSensitive'));
  const dflt = settings.get<boolean>('search.caseSensitive');
  // Drop the explicit token when it would equal the default.
  search.q = withCase(search.q, !on === dflt ? null : !on);
}

export async function saveCurrent(): Promise<SavedSearch | null> {
  const q = search.q.trim();
  if (!hasContent(q)) {
    toast.info(t('searchPanel.nothingToSave'));
    return null;
  }
  loadSaved();
  const name = await inputBox({
    title: t('commands.search.saveCurrent'),
    prompt: q,
    value: q.length <= 40 ? q : '',
    placeholder: t('searchPanel.namePlaceholder'),
    validate: (v) => (v.trim() ? null : t('validation.required')),
  });
  if (typeof name !== 'string') return null;
  const s = addSaved(name, q);
  toast.success(t('searchPanel.saved', { name: s.name }), { action: { label: t('searchPanel.openAsBoard'), run: () => openSavedBoard(s) } });
  return s;
}

export async function renameSavedFlow(s: SavedSearch) {
  const name = await inputBox({ title: t('searchPanel.rename'), value: s.name, validate: (v) => (v.trim() ? null : t('validation.required')) });
  if (typeof name !== 'string') return;
  renameSaved(s.id, name);
  for (const tab of allTabs()) if (tab.kind === 'savedSearch' && tab.payload?.id === s.id) tab.payload = { ...tab.payload, name: name.trim() };
  persistWorkspace();
}

export function deleteSaved(s: SavedSearch) {
  const index = search.saved.findIndex((x) => x.id === s.id);
  removeSaved(s.id);
  toast.info(t('searchPanel.deleted', { name: s.name }), { action: { label: t('common.undo'), run: () => restoreSaved(s, index) } });
}

/** Open a saved search (or an ad-hoc query) as a virtual board tab. */
export function openSavedBoard(s: SavedSearch | { q: string; name?: string; groupBy?: GroupBy }) {
  const id = 'id' in s ? s.id : undefined;
  if (id) {
    const existing = allTabs().find((x) => x.kind === 'savedSearch' && x.payload?.id === id);
    if (existing) {
      focusTab(existing.id);
      return;
    }
  }
  openTabOfKind('savedSearch', { id, name: s.name || s.q, q: s.q, groupBy: s.groupBy ?? 'lane' });
}

export function setSavedGroupBy(id: string | undefined, groupBy: GroupBy) {
  if (id) updateSaved(id, { groupBy });
}

export async function pickSaved(): Promise<SavedSearch | null> {
  loadSaved();
  if (!search.saved.length) {
    toast.info(t('searchPanel.noSaved'));
    return null;
  }
  const r = await quickPick(
    search.saved.map((s) => ({ label: s.name, description: s.q, icon: LayoutGrid, value: s })),
    { placeholder: t('commands.search.openSaved'), matchOnDescription: true },
  );
  return r && typeof r === 'object' && !Array.isArray(r) ? (r as SavedSearch) : null;
}

export async function openHit(h: SearchHit, side = false) {
  if (side && h.kind === 'doc') {
    await openRow(
      { key: '', type: 'card', depth: 0, parentKey: null, boardId: h.board, id: h.id, label: h.title, expandable: false, expanded: false, kind: 'files' },
      true,
    );
    return;
  }
  if (h.kind === 'doc') await openDocTab(h.board, h.id);
  else await openCard(h.board, h.id);
}

export async function hitMenu(h: SearchHit): Promise<MenuItem[]> {
  const m = boards.get(h.board) ?? (await openBoard({ id: h.board }).catch(() => null));
  const extra = m?.node(h.id) ? cardMenu(h.board, h.id).filter((it) => it.label !== t('cards.open') && it.label !== t('cards.rename')) : [];
  return [
    { label: t('explorer.open'), icon: ExternalLink, run: () => void openHit(h) },
    ...(h.kind === 'doc' ? [{ label: t('explorer.openToSide'), icon: PanelRight, run: () => void openHit(h, true) }] : []),
    { label: t('searchPanel.revealInExplorer'), icon: LocateFixed, run: () => void revealCard(h.board, h.id) },
    { label: t('cards.copyLink'), icon: Link, run: () => void copyText(`[[${h.id}]]`) },
    ...(extra.length ? [{ separator: true } as MenuItem, ...extra] : []),
  ];
}

export function savedMenu(s: SavedSearch): MenuItem[] {
  return [
    { label: t('searchPanel.openAsBoard'), icon: LayoutGrid, run: () => openSavedBoard(s) },
    { label: t('searchPanel.load'), icon: ExternalLink, run: () => ((search.q = s.q), focusSearch()) },
    { separator: true },
    { label: t('searchPanel.rename'), icon: Pencil, run: () => void renameSavedFlow(s) },
    { label: t('searchPanel.copyQuery'), icon: Copy, run: () => void copyText(s.q) },
    { separator: true },
    { label: t('common.delete'), icon: Trash2, danger: true, run: () => deleteSaved(s) },
  ];
}
