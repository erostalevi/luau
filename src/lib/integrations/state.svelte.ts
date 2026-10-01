// Integrations state: accounts, mirrors, card links (fed into each BoardModel's
// `remote` map so the card face / editor / filters see live remote info).

import { rpc, onCoreEvent } from '$lib/backend/rpc';
import type { RemoteInfo } from '$lib/backend/types';
import { boards } from '$lib/state/boards.svelte';
import { activeCard } from '$lib/app/helpers';
import { ctx } from '$lib/commands/context.svelte';
import { integrationStatus } from './status.svelte';
import { uiGet, uiSet } from '$lib/state/persist.svelte';
import { emptySelection, type ListSelection } from './resultSelection';
import { setCollapsed } from './grouping';
import type { Account, IdName, Mirror, RemoteIssue, SearchMode } from './types';

export const integ = $state({
  accounts: [] as Account[],
  mirrors: [] as Mirror[],
  loaded: false,
  /** Panel: selected account id. */
  account: '' as string,
  query: '' as string,
  results: [] as RemoteIssue[],
  next: null as string | null,
  searching: false,
  error: '' as string,
  lastSync: '' as string,
  /** Panel: selected result keys (list order) + range anchor. */
  selection: emptySelection() as ListSelection,
  /** Panel: mode + query that produced `results` (keys the collapsed groups). */
  resultsMode: 'jql' as SearchMode,
  resultsQuery: '' as string,
  /** Trello board names by account (group headers), loaded on demand. */
  boardNames: {} as Record<string, Record<string, string>>,
});

export function accountById(id: string | undefined | null): Account | undefined {
  return integ.accounts.find((a) => a.id === id);
}

export const isIssueAccount = (a: Account) => a.provider !== 'slack';
export const isJiraAccount = (a: Account | undefined) => a?.provider === 'jiraCloud' || a?.provider === 'jiraServer';

const MODE_KEY = 'integrations.searchMode';

/** Search mode of an account (remembered per account in ui-state; Trello is always text). */
export function searchModeOf(a: Account | undefined): SearchMode {
  if (!a || !isJiraAccount(a)) return 'text';
  return uiGet<Record<string, SearchMode>>(MODE_KEY, {})[a.id] === 'text' ? 'text' : 'jql';
}

export function setSearchMode(accountId: string, mode: SearchMode) {
  uiSet(MODE_KEY, { ...uiGet<Record<string, SearchMode>>(MODE_KEY, {}), [accountId]: mode });
}

const COLLAPSED_KEY = 'integrations.collapsedGroups';

/** Whether a result group (account + query + project) is collapsed; persisted in ui-state. */
export function isGroupCollapsed(key: string): boolean {
  return uiGet<Record<string, true>>(COLLAPSED_KEY, {})[key] === true;
}

export function setGroupCollapsed(key: string, collapsed: boolean) {
  uiSet(COLLAPSED_KEY, setCollapsed(uiGet<Record<string, true>>(COLLAPSED_KEY, {}), key, collapsed));
}

const boardNamesLoading = new Set<string>();

/** Trello: fetch board names once per account so groups show names, not ids. */
export async function loadBoardNames(a: Account) {
  if (a.provider !== 'trello' || integ.boardNames[a.id] || boardNamesLoading.has(a.id)) return;
  boardNamesLoading.add(a.id);
  try {
    const list = await rpc<IdName[]>('remote.projects', { account: a.id });
    integ.boardNames[a.id] = Object.fromEntries(list.map((b) => [b.id, b.name]));
  } catch {
    /* offline: headers fall back to board ids */
  } finally {
    boardNamesLoading.delete(a.id);
  }
}

export async function loadAccounts() {
  try {
    integ.accounts = await rpc<Account[]>('integrations.accounts');
  } catch {
    integ.accounts = [];
  }
  if (!integ.accounts.some((a) => a.id === integ.account)) integ.account = integ.accounts.find(isIssueAccount)?.id ?? '';
  integ.loaded = true;
}

export async function loadMirrors() {
  try {
    integ.mirrors = await rpc<Mirror[]>('integrations.mirrors');
  } catch {
    integ.mirrors = [];
  }
}

function setLinks(boardId: string, links: Record<string, RemoteInfo>) {
  const b = boards.get(boardId);
  if (!b) return;
  for (const id of [...b.remote.keys()]) if (!(id in links)) b.remote.delete(id);
  for (const [id, info] of Object.entries(links)) b.remote.set(id, info);
}

const loadedBoards = new Set<string>();

export async function loadLinks(boardId: string) {
  try {
    setLinks(boardId, await rpc<Record<string, RemoteInfo>>('remote.links', { board: boardId }));
  } catch {
    /* board closed */
  }
}

export function initIntegrationState() {
  void loadAccounts();
  void loadMirrors();
  onCoreEvent((e) => {
    if (e.type !== 'custom') return;
    const p = (e.payload ?? {}) as Record<string, unknown>;
    if (e.name === 'integrations.links') setLinks(String(p.boardId), (p.links ?? {}) as Record<string, RemoteInfo>);
    else if (e.name === 'integrations.accounts') void loadAccounts();
    else if (e.name === 'integrations.mirrors') void loadMirrors();
    else if (e.name === 'integrations.status') {
      integ.lastSync = String(p.at ?? '');
      integrationStatus.offline = p.offline ? integ.lastSync : '';
    }
  });
  $effect.root(() => {
    // Fetch links once per opened board.
    $effect(() => {
      for (const id of boards.keys()) {
        if (!loadedBoards.has(id)) {
          loadedBoards.add(id);
          void loadLinks(id);
        }
      }
    });
    // `cardIsRemote` context key for `when` clauses and keybindings.
    $effect(() => {
      const c = activeCard();
      ctx.cardIsRemote = !!(c && c.board.remote.get(c.id));
    });
  });
}
