// Commands of the left-panel Explorer and Search sections and of files-board homes.
// `search.focus` already lives in `commands/builtin/workspace.ts` (palette entry point).

import { LocateFixed, ChevronsDownUp, Eye, FolderTree, BookmarkPlus, X, CaseSensitive, LayoutGrid, Filter, FilePlus2, Search } from '@lucide/svelte';
import type { Command } from '$lib/commands/registry.svelte';
import { ui, persistUi } from '$lib/state/ui.svelte';
import { explorer, collapseAll, revealActive } from './explorer/explorerState.svelte';
import { toggleShowHidden, registerExplorerDrop } from './explorer/explorerActions';
import { search } from './search/searchState.svelte';
import { focusSearch, clearSearch, toggleCase, saveCurrent, pickSaved, openSavedBoard } from './search/searchActions';
import { newDocument } from '$lib/files/filesActions';
import { files } from '$lib/files/filesState.svelte';

function focusExplorer() {
  ui.left.visible = true;
  ui.left.section = 'explorer';
  persistUi();
  explorer.focusTick++;
}

export const commands: Command[] = [
  {
    id: 'explorer.revealActive',
    title: 'commands.explorer.revealActive',
    category: 'explorer',
    icon: LocateFixed,
    run: () => revealActive(),
  },
  {
    id: 'explorer.collapseAll',
    title: 'commands.explorer.collapseAll',
    category: 'explorer',
    icon: ChevronsDownUp,
    run: () => collapseAll(),
  },
  {
    id: 'explorer.toggleHidden',
    title: 'commands.explorer.toggleHidden',
    category: 'explorer',
    icon: Eye,
    // Same setting as app.toggleHiddenBoards; kept for the explorer toolbar tooltip.
    hidden: true,
    run: () => toggleShowHidden(),
  },
  {
    id: 'explorer.focus',
    title: 'commands.explorer.focus',
    category: 'explorer',
    icon: FolderTree,
    run: focusExplorer,
  },
  {
    id: 'search.saveCurrent',
    title: 'commands.search.saveCurrent',
    category: 'search',
    icon: BookmarkPlus,
    run: () => saveCurrent(),
  },
  {
    id: 'search.clear',
    title: 'commands.search.clear',
    category: 'search',
    icon: X,
    run: () => clearSearch(),
  },
  {
    id: 'search.toggleCase',
    title: 'commands.search.toggleCase',
    category: 'search',
    icon: CaseSensitive,
    run: () => (toggleCase(), focusSearch()),
  },
  {
    id: 'search.toggleFilters',
    title: 'commands.search.toggleFilters',
    category: 'search',
    icon: Filter,
    run: () => ((search.filtersOpen = !search.filtersOpen), focusSearch()),
  },
  {
    id: 'search.openSaved',
    title: 'commands.search.openSaved',
    category: 'search',
    icon: LayoutGrid,
    run: async () => {
      const s = await pickSaved();
      if (s) openSavedBoard(s);
    },
  },
  {
    id: 'files.newDocument',
    title: 'commands.files.newDocument',
    category: 'board',
    icon: FilePlus2,
    run: (boardId?: unknown) => newDocument(typeof boardId === 'string' ? boardId : undefined),
  },
  {
    id: 'files.focusFilter',
    title: 'commands.files.focusFilter',
    category: 'board',
    icon: Search,
    when: 'boardKind == files',
    hidden: true,
    run: () => files.focusTick++,
  },
];

export function init() {
  registerExplorerDrop();
}
