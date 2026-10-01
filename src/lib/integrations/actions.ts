// User-facing integration actions (used by commands, the panel, the card
// menu and the editor header). Every remote write goes through `runGated`.

import { rpc, RpcError } from '$lib/backend/rpc';
import type { Parent, RemoteInfo } from '$lib/backend/types';
import { boards, openBoard } from '$lib/state/boards.svelte';
import { registry } from '$lib/state/registry.svelte';
import { activeBoard, activeCard, openExternal, pickFile } from '$lib/app/helpers';
import { openCard } from '$lib/app/open';
import { quickPick, pickOne, inputBox, steps, BACK, type QuickItem } from '$lib/quickinput/qi.svelte';
import { confirm } from '$lib/state/dialogs.svelte';
import { toast } from '$lib/state/toasts.svelte';
import { settings } from '$lib/settings/store.svelte';
import { t } from '$lib/i18n/index.svelte';
import { runGated, errorText } from './gate';
import { integ, loadAccounts, loadLinks, loadMirrors, isIssueAccount, isJiraAccount, accountById, searchModeOf, setSearchMode } from './state.svelte';
import { capKeys, emptySelection, prune, MAX_LINK_MANY } from './resultSelection';
import type { Account, IdName, LinkManyResult, MirrorSource, ProviderKind, RemoteIssue, SavedQuery, SearchMode, Transition } from './types';
import type { RemoteUser } from '$lib/backend/types';

export interface CardRef {
  boardId: string;
  id: string;
}

export interface RemoteCtx {
  boardId: string;
  id: string;
  remote: RemoteInfo;
}

/** The remote-linked card the user is acting on. */
export function currentRemote(at?: CardRef): RemoteCtx | null {
  const c = at ? (boards.get(at.boardId) ? { board: boards.get(at.boardId)!, id: at.id } : null) : activeCard();
  const remote = c?.board.remote.get(c.id);
  return c && remote ? { boardId: c.board.id, id: c.id, remote } : null;
}

function needRemote(at?: CardRef): RemoteCtx | null {
  const r = currentRemote(at);
  if (!r) toast.info(t('integrations.needLinkedCard'));
  return r;
}

async function busy<T>(label: string, fn: () => Promise<T>): Promise<T | null> {
  integ.searching = true;
  try {
    return await fn();
  } catch (e) {
    toast.error(t('integrations.failed', { action: label, message: errorText(e) }));
    return null;
  } finally {
    integ.searching = false;
  }
}

// --- connect flows -------------------------------------------------------------

/** Accepts `acme.atlassian.net` or a full URL; returns an error message or null. */
export function validateSite(v: string, allowHttp: boolean): string | null {
  const s = v.trim();
  if (!s) return t('integrations.connect.siteRequired');
  let u: URL;
  try {
    u = new URL(s.includes('://') ? s : `https://${s}`);
  } catch {
    return t('integrations.connect.siteInvalid');
  }
  if (u.username || u.password) return t('integrations.connect.siteInvalid');
  if (u.protocol === 'http:' && !allowHttp) return t('integrations.connect.httpsOnly');
  if (u.protocol !== 'https:' && u.protocol !== 'http:') return t('integrations.connect.siteInvalid');
  return null;
}

async function finishConnect(req: Record<string, unknown>): Promise<Account | null> {
  const a = await busy(t('integrations.connect.testing'), () => rpc<Account>('integrations.connect', req));
  if (!a) return null;
  await loadAccounts();
  if (a.provider !== 'slack') integ.account = a.id;
  toast.success(t('integrations.connect.connected', { name: a.userName ?? a.label, service: a.label }));
  if (!a.persisted) toast.warn(t('integrations.connect.memoryOnly'));
  return a;
}

export async function connectJira(): Promise<Account | null> {
  const total = 4;
  const r = await steps<[ProviderKind, string, string, string]>([
    () =>
      pickOne<ProviderKind>(
        [
          { label: t('integrations.connect.jiraCloud'), description: 'atlassian.net', value: 'jiraCloud' },
          { label: t('integrations.connect.jiraServer'), description: t('integrations.connect.jiraServerDesc'), value: 'jiraServer' },
        ],
        { title: t('integrations.connect.jiraTitle'), step: 1, totalSteps: total },
      ),
    (res) =>
      inputBox({
        title: t('integrations.connect.jiraTitle'),
        prompt: t('integrations.connect.sitePrompt'),
        placeholder: res[0] === 'jiraCloud' ? 'acme.atlassian.net' : 'https://jira.example.com',
        step: 2,
        totalSteps: total,
        validate: (v) => validateSite(v, res[0] === 'jiraServer'),
      }),
    (res) =>
      res[0] === 'jiraCloud'
        ? inputBox({
            title: t('integrations.connect.jiraTitle'),
            prompt: t('integrations.connect.emailPrompt'),
            placeholder: 'you@company.com',
            step: 3,
            totalSteps: total,
            validate: (v) => (/^\S+@\S+\.\S+$/.test(v.trim()) ? null : t('integrations.connect.emailInvalid')),
          })
        : Promise.resolve('-'),
    (res) =>
      inputBox({
        title: t('integrations.connect.jiraTitle'),
        prompt: res[0] === 'jiraCloud' ? t('integrations.connect.apiTokenPrompt') : t('integrations.connect.patPrompt'),
        password: true,
        step: 4,
        totalSteps: total,
        validate: (v) => (v.trim() ? null : t('integrations.connect.tokenRequired')),
      }),
  ]);
  if (!r) return null;
  const [provider, site, email, token] = r;
  let insecure = false;
  if (provider === 'jiraServer' && site.trim().toLowerCase().startsWith('http://')) {
    insecure = await confirm({
      title: t('integrations.connect.httpTitle'),
      message: t('integrations.connect.httpWarning'),
      confirmLabel: t('integrations.connect.httpConfirm'),
      danger: true,
      cancelFocused: true,
    });
    if (!insecure) return null;
  }
  let clientCertPath: string | null = null;
  if (provider === 'jiraServer') {
    const m = await pickOne<'none' | 'cert'>(
      [
        { label: t('integrations.connect.noClientCert'), value: 'none' },
        { label: t('integrations.connect.clientCert'), description: 'PEM', value: 'cert' },
      ],
      { title: t('integrations.connect.mtlsTitle') },
    );
    if (m === undefined || m === BACK) return null;
    if (m === 'cert') {
      const f = await pickFile(t('integrations.connect.clientCert'), [{ name: 'PEM', extensions: ['pem', 'crt', 'key'] }]);
      clientCertPath = f?.[0] ?? null;
    }
  }
  return finishConnect({ provider, baseUrl: site.trim(), user: provider === 'jiraCloud' ? email.trim() : null, token, insecureHttp: insecure, clientCertPath });
}

export async function connectTrello(): Promise<Account | null> {
  const r = await steps<[string, string]>([
    () =>
      inputBox({
        title: t('integrations.connect.trelloTitle'),
        prompt: t('integrations.connect.trelloKeyPrompt'),
        step: 1,
        totalSteps: 2,
        validate: (v) => (/^[a-f0-9]{20,64}$/i.test(v.trim()) ? null : t('integrations.connect.keyInvalid')),
      }),
    () =>
      inputBox({
        title: t('integrations.connect.trelloTitle'),
        prompt: t('integrations.connect.trelloTokenPrompt'),
        password: true,
        step: 2,
        totalSteps: 2,
        validate: (v) => (v.trim() ? null : t('integrations.connect.tokenRequired')),
      }),
  ]);
  if (!r) return null;
  return finishConnect({ provider: 'trello', user: r[0].trim(), token: r[1] });
}

export async function connectSlack(): Promise<Account | null> {
  const token = await inputBox({
    title: t('integrations.connect.slackTitle'),
    prompt: t('integrations.connect.slackPrompt'),
    placeholder: 'xoxb-…',
    password: true,
    validate: (v) => (/^xox[bp]-[A-Za-z0-9-]+$/.test(v.trim()) ? null : t('integrations.connect.slackInvalid')),
  });
  if (typeof token !== 'string') return null;
  return finishConnect({ provider: 'slack', token });
}

export async function removeAccount(a: Account) {
  const ok = await confirm({
    title: t('integrations.removeTitle', { name: a.label }),
    message: t('integrations.removeMessage'),
    confirmLabel: t('integrations.remove'),
    danger: true,
    cancelFocused: true,
  });
  if (!ok) return;
  await busy(t('integrations.remove'), () => rpc('integrations.remove', { account: a.id }));
  await loadAccounts();
}

// --- search ------------------------------------------------------------------------

/** Server message of a rejected query (`remote_bad_request:<message>`), or null. */
function badRequestDetail(e: unknown): string | null {
  const raw = e instanceof Error ? e.message : String(e);
  const m = /remote_bad_request:(.*)$/s.exec(raw);
  return m ? m[1].trim() : null;
}

export async function search(more = false) {
  const a = accountById(integ.account);
  if (!a) return;
  const mode = searchModeOf(a);
  integ.searching = true;
  integ.error = '';
  try {
    const page = await rpc<{ issues: RemoteIssue[]; next: string | null }>('remote.search', {
      account: a.id,
      query: integ.query,
      mode,
      next: more ? integ.next : null,
    });
    integ.results = more ? [...integ.results, ...page.issues] : page.issues;
    integ.next = page.next;
    if (!more) {
      integ.resultsMode = mode;
      integ.resultsQuery = integ.query;
    }
    integ.selection = more
      ? prune(
          integ.results.map((i) => i.key),
          integ.selection,
        )
      : emptySelection();
  } catch (e) {
    const detail = badRequestDetail(e);
    integ.error =
      detail && isJiraAccount(a) && integ.query.trim()
        ? t(mode === 'jql' ? 'integrations.invalidJql' : 'integrations.searchRejected', { detail })
        : errorText(e);
    if (!more) {
      integ.results = [];
      integ.selection = emptySelection();
    }
  } finally {
    integ.searching = false;
  }
}

/** Switch the panel between JQL and plain-text search (Jira only) and re-run it. */
export function setMode(mode: SearchMode) {
  const a = accountById(integ.account);
  if (!a || !isJiraAccount(a) || searchModeOf(a) === mode) return;
  setSearchMode(a.id, mode);
  if (integ.query.trim()) void search();
}

export async function saveQuery() {
  const a = accountById(integ.account);
  if (!a || !integ.query.trim()) return;
  const name = await inputBox({ title: t('integrations.saveQuery'), prompt: t('integrations.saveQueryPrompt'), value: integ.query.slice(0, 40) });
  if (typeof name !== 'string' || !name.trim()) return;
  const saved = [...a.savedQueries.filter((q) => q.name !== name.trim()), { name: name.trim(), query: integ.query, mode: searchModeOf(a) }];
  await busy(t('integrations.saveQuery'), () => rpc('integrations.update', { account: a.id, patch: { savedQueries: saved } }));
  await loadAccounts();
}

/** Run a saved query in the mode it was saved with. */
export function runSaved(q: SavedQuery) {
  const a = accountById(integ.account);
  if (!a) return;
  if (isJiraAccount(a)) setSearchMode(a.id, q.mode ?? 'jql');
  integ.query = q.query;
  void search();
}

export async function removeQuery(name: string) {
  const a = accountById(integ.account);
  if (!a) return;
  await busy(t('integrations.saveQuery'), () =>
    rpc('integrations.update', { account: a.id, patch: { savedQueries: a.savedQueries.filter((q) => q.name !== name) } }),
  );
  await loadAccounts();
}

export async function setDefaultQuery() {
  const a = accountById(integ.account);
  if (!a) return;
  const v = await inputBox({ title: t('integrations.defaultQuery'), prompt: t('integrations.defaultQueryPrompt'), value: a.defaultQuery ?? '' });
  if (typeof v !== 'string') return;
  await busy(t('integrations.defaultQuery'), () => rpc('integrations.update', { account: a.id, patch: { defaultQuery: v } }));
  await loadAccounts();
}

// --- linked copies -------------------------------------------------------------------

export async function linkIssue(account: string, key: string, board: string, parent: Parent, before: string | null) {
  try {
    const id = await rpc<string>('remote.link', { account, key, board, parent, before });
    toast.success(t('integrations.linked', { key }), { action: { label: t('integrations.open'), run: () => void openCard(board, id) } });
  } catch (e) {
    const m = e instanceof RpcError ? /already_linked:(c[a-z0-9]{6})/.exec(e.message) : null;
    if (m) {
      toast.warn(t('integrations.alreadyOnBoard', { key }), { action: { label: t('integrations.jumpToExisting'), run: () => void openCard(board, m[1]) } });
    } else toast.error(t('integrations.failed', { action: key, message: errorText(e) }));
  }
}

/** Linked copies of several issues at one drop position, in list order, as
 *  one undo step (`remote.linkMany` → one `Op::Batch`). Capped at 100. */
export async function linkMany(account: string, keys: string[], board: string, parent: Parent, before: string | null) {
  const { keys: list, capped } = capKeys(keys);
  if (capped) toast.warn(t('integrations.tooMany', { max: MAX_LINK_MANY }));
  if (!list.length) return;
  try {
    const r = await rpc<LinkManyResult>('remote.linkMany', { account, keys: list, board, parent, before });
    if (r.created.length)
      toast.success(t('integrations.linkedMany', { count: r.created.length }), {
        action: { label: t('integrations.open'), run: () => void openCard(board, r.created[0]) },
      });
    if (r.skipped.length) {
      const first = r.skipped[0];
      toast.warn(t('integrations.skippedMany', { count: r.skipped.length, keys: r.skipped.map((s) => s.key).join(', ') }), {
        action: { label: t('integrations.jumpToExisting'), run: () => void openCard(board, first.card) },
      });
    }
    if (r.missing.length) toast.warn(t('integrations.missingMany', { count: r.missing.length, keys: r.missing.join(', ') }));
  } catch (e) {
    toast.error(t('integrations.failed', { action: t('integrations.addSelected'), message: errorText(e) }));
  }
}

/** Keyboard alternative to dragging: pick a board and a lane (or top level),
 *  then add the selected results at the end. */
export async function addSelectedTo() {
  const a = accountById(integ.account);
  const keys = integ.selection.keys;
  if (!a || !keys.length) return void toast.info(t('integrations.selectFirst'));
  const r = await steps<[string, Parent]>([
    () =>
      pickOne(
        registry.data.boards.filter((x) => !x.hidden && !x.missing && !x.mirror).map((x) => ({ label: x.name, value: x.id })),
        { title: t('integrations.addSelectedTitle', { count: keys.length }), step: 1, totalSteps: 2 },
      ),
    async ([boardId]) => {
      const b = boards.get(boardId) ?? (await openBoard({ id: boardId }));
      const items: QuickItem<Parent>[] =
        b.kind === 'files'
          ? [{ label: t('cards.topLevel'), value: { kind: 'root' } }]
          : b.lanes.filter((l) => !l.archived).map((l) => ({ label: l.name, value: { kind: 'lane', id: l.id } as Parent }));
      return pickOne(items, { title: t('cards.pickDestination'), step: 2, totalSteps: 2 });
    },
  ]);
  if (!r) return;
  await linkMany(a.id, keys, r[0], r[1], null);
}

export async function copyFromMirror(from: string, ids: string[], to: string, parent: Parent, before: string | null) {
  try {
    const out = await rpc<string[]>('remote.copyFromMirror', { from, ids, to, parent, before });
    toast.success(t('integrations.copied', { count: out.length }));
  } catch (e) {
    const m = e instanceof RpcError ? /already_linked:(c[a-z0-9]{6})/.exec(e.message) : null;
    if (m)
      toast.warn(t('integrations.alreadyOnBoard', { key: '' }), { action: { label: t('integrations.jumpToExisting'), run: () => void openCard(to, m[1]) } });
    else toast.error(errorText(e));
  }
}

// --- gated actions -------------------------------------------------------------------

export async function pull() {
  const r = currentRemote();
  if (r) {
    if ((await runGated({ kind: 'pull', board: r.boardId, card: r.remote.mirror ? null : r.id })) !== null) toast.success(t('integrations.pulled'));
    return;
  }
  const b = activeBoard();
  if (b?.header.readOnly?.startsWith('mirror')) {
    if ((await runGated({ kind: 'pull', board: b.id })) !== null) toast.success(t('integrations.pulled'));
    return;
  }
  toast.info(t('integrations.needLinkedCard'));
}

export async function push() {
  const r = needRemote();
  if (!r) return;
  if (r.remote.mirror) return void toast.info(t('integrations.mirrorNoPush'));
  if ((await runGated({ kind: 'push', board: r.boardId, card: r.id })) !== null) {
    toast.success(t('integrations.pushed', { key: r.remote.key }));
    void loadLinks(r.boardId);
  }
}

export async function transitionCard(boardId: string, id: string, tr: Transition) {
  const remote = boards.get(boardId)?.remote.get(id);
  if (!remote) return;
  if ((await runGated({ kind: 'transition', board: boardId, card: id, id: tr.id, to: tr.to })) !== null) {
    toast.success(
      t('integrations.transitioned', { key: remote.key, status: tr.to }),
      remote.status ? { action: { label: t('integrations.revert'), run: () => void revertTransition(boardId, id, remote.status!) } } : undefined,
    );
  }
}

async function revertTransition(boardId: string, id: string, status: string) {
  const remote = boards.get(boardId)?.remote.get(id);
  if (!remote) return;
  const list = await rpc<Transition[]>('remote.transitions', { account: remote.account, key: remote.key }).catch(() => []);
  const back = list.find((x) => x.to === status);
  if (back) await transitionCard(boardId, id, back);
  else toast.warn(t('integrations.noTransitions'));
}

export async function transition(at?: CardRef) {
  const r = needRemote(at);
  if (!r) return;
  const list = await busy(t('commands.remote.transition'), () => rpc<Transition[]>('remote.transitions', { account: r.remote.account, key: r.remote.key }));
  if (!list) return;
  if (!list.length) return void toast.info(t('integrations.noTransitions'));
  const tr = await pickOne<Transition>(
    list.map((x) => ({ label: x.to, description: x.name !== x.to ? x.name : undefined, value: x })),
    { title: t('integrations.transitionTitle', { key: r.remote.key }), placeholder: r.remote.status ?? '' },
  );
  if (!tr || tr === BACK) return;
  await transitionCard(r.boardId, r.id, tr);
}

export async function assign(at?: CardRef) {
  const r = needRemote(at);
  if (!r) return;
  const NONE = { id: '', name: '' } as RemoteUser;
  const u = await pickOne<RemoteUser>([], {
    title: t('integrations.assignTitle', { key: r.remote.key }),
    placeholder: t('integrations.searchUsers'),
    selfFiltered: true,
    onValue: async (q) => {
      const head: QuickItem<RemoteUser>[] = [{ label: t('integrations.unassigned'), value: NONE, alwaysShow: true }];
      if (q.trim().length < 2) return head;
      const users = await rpc<RemoteUser[]>('remote.users', { account: r.remote.account, q }).catch(() => []);
      return [...head, ...users.map((x) => ({ label: x.name, value: x }))];
    },
  });
  if (!u || u === BACK) return;
  if ((await runGated({ kind: 'assign', board: r.boardId, card: r.id, user: u.id ? u : null })) !== null)
    toast.success(t('integrations.assigned', { key: r.remote.key }));
}

export async function comment(at?: CardRef) {
  const r = needRemote(at);
  if (!r) return;
  const body = await inputBox({
    title: t('integrations.commentTitle', { key: r.remote.key }),
    prompt: t('integrations.commentPrompt'),
    validate: (v) => (v.trim() ? null : t('integrations.commentRequired')),
  });
  if (typeof body !== 'string' || !body.trim()) return;
  if ((await runGated({ kind: 'comment', board: r.boardId, card: r.id, body })) !== null) toast.success(t('integrations.commented', { key: r.remote.key }));
}

export async function openInBrowser(at?: CardRef) {
  const r = needRemote(at);
  if (!r) return;
  if (!/^https?:\/\//i.test(r.remote.url)) return;
  await openExternal(r.remote.url);
}

async function pickAccount(filter: (a: Account) => boolean, title: string): Promise<Account | null> {
  const list = integ.accounts.filter(filter);
  if (!list.length) {
    toast.info(t('integrations.noAccounts'));
    return null;
  }
  if (list.length === 1) return list[0];
  const a = await pickOne<Account>(
    list.map((x) => ({ label: x.label, description: x.baseUrl.replace(/^https?:\/\//, ''), value: x })),
    { title },
  );
  return a && a !== BACK ? a : null;
}

export async function createIssue(at?: CardRef) {
  const c = at && boards.get(at.boardId) ? { board: boards.get(at.boardId)!, id: at.id } : activeCard();
  if (!c) return void toast.info(t('integrations.needCard'));
  if (c.board.remote.get(c.id)) return void toast.info(t('integrations.alreadyLinked'));
  if (c.board.header.readOnly) return;
  const a = await pickAccount(isIssueAccount, t('commands.remote.createIssue'));
  if (!a) return;
  const trello = a.provider === 'trello';
  const projects = await busy(t('commands.remote.createIssue'), () => rpc<IdName[]>('remote.projects', { account: a.id }));
  if (!projects) return;
  const r = await steps<[IdName, IdName]>([
    () =>
      pickOne<IdName>(
        projects.map((p) => ({ label: p.name, description: p.key ?? undefined, value: p })),
        { title: t(trello ? 'integrations.pickBoard' : 'integrations.pickProject'), step: 1, totalSteps: 2 },
      ),
    async (res) => {
      const kinds = await rpc<IdName[]>('remote.issueTypes', { account: a.id, project: trello ? res[0].id : (res[0].key ?? res[0].id) }).catch(() => []);
      return pickOne<IdName>(
        kinds.map((k) => ({ label: k.name, value: k })),
        { title: t(trello ? 'integrations.pickList' : 'integrations.pickType'), step: 2, totalSteps: 2 },
      );
    },
  ]);
  if (!r) return;
  const [project, kind] = r;
  const out = await runGated<{ key: string }>({
    kind: 'create',
    board: c.board.id,
    card: c.id,
    account: a.id,
    project: trello ? project.id : (project.key ?? project.id),
    issueType: trello ? null : kind.name,
    container: trello ? kind.id : null,
  });
  if (out) {
    toast.success(t('integrations.created', { key: out.key }));
    void loadLinks(c.board.id);
  }
}

export async function unlink() {
  const r = needRemote();
  if (!r || r.remote.mirror) return;
  await busy(t('commands.remote.unlink'), () => rpc('remote.unlink', { board: r.boardId, card: r.id }));
}

export async function refreshLinks() {
  const b = activeBoard();
  if (!b) return;
  if (b.header.readOnly?.startsWith('mirror')) {
    await busy(t('commands.remote.refresh'), () => rpc('remote.mirror.sync', { board: b.id }));
  } else {
    await busy(t('commands.remote.refresh'), () => rpc('remote.refresh', { board: b.id }));
  }
}

// --- mirrors ---------------------------------------------------------------------------

export async function mirrorBoard(accountId?: string, preset?: MirrorSource, presetName?: string) {
  const a = accountId ? accountById(accountId) : await pickAccount(isIssueAccount, t('commands.integrations.mirrorBoard'));
  if (!a) return;
  let source = preset;
  let name = presetName ?? '';
  if (!source) {
    const choices: QuickItem<'board' | 'query'>[] = [{ label: t('integrations.mirror.fromBoard'), value: 'board' }];
    // Plain-text panel searches are not JQL: only prefill a JQL query.
    const jql = searchModeOf(a) === 'jql' ? integ.query : '';
    if (a.provider !== 'trello') choices.push({ label: t('integrations.mirror.fromQuery'), description: jql || a.defaultQuery || '', value: 'query' });
    const how = choices.length > 1 ? await pickOne(choices, { title: t('commands.integrations.mirrorBoard') }) : 'board';
    if (!how || how === BACK) return;
    if (how === 'query') {
      const q = await inputBox({
        title: t('integrations.mirror.fromQuery'),
        value: jql || a.defaultQuery || '',
        validate: (v) => (v.trim() ? null : t('integrations.mirror.queryRequired')),
      });
      if (typeof q !== 'string') return;
      source = { kind: 'query', query: q };
    } else {
      const list = await busy(t('commands.integrations.mirrorBoard'), () => rpc<IdName[]>('remote.boards', { account: a.id }));
      if (!list) return;
      const b = await pickOne<IdName>(
        list.map((x) => ({ label: x.name, description: [x.key, x.detail].filter(Boolean).join(' · '), value: x })),
        { title: t('integrations.mirror.pickBoard'), placeholder: t('integrations.mirror.pickBoardHint') },
      );
      if (!b || b === BACK) return;
      source = a.provider === 'trello' ? { kind: 'trello', id: b.id } : { kind: 'board', id: b.id };
      name = b.name;
    }
  }
  if (integ.mirrors.some((m) => m.account === a.id && JSON.stringify(m.source) === JSON.stringify(source))) {
    const ok = await confirm({
      title: t('integrations.mirror.duplicateTitle'),
      message: t('integrations.mirror.duplicate'),
      confirmLabel: t('integrations.mirror.createAnyway'),
    });
    if (!ok) return;
  }
  const nm = await inputBox({
    title: t('integrations.mirror.nameTitle'),
    value: name,
    validate: (v) => (v.trim() ? null : t('integrations.mirror.nameRequired')),
  });
  if (typeof nm !== 'string') return;
  const id = await busy(t('commands.integrations.mirrorBoard'), () => rpc<string>('remote.mirror.create', { account: a.id, source, name: nm, watch: true }));
  if (!id) return;
  await loadMirrors();
  toast.success(t('integrations.mirror.created', { name: nm }));
  const { openBoardTab } = await import('$lib/state/workspace.svelte');
  void openBoardTab(id);
}

export async function setWatch(id: string, watch: boolean) {
  await busy(t('integrations.mirror.watch'), () => rpc('remote.mirror.watch', { board: id, watch }));
  await loadMirrors();
}

export async function removeMirror(id: string, name: string) {
  const ok = await confirm({
    title: t('integrations.mirror.removeTitle', { name }),
    message: t('integrations.mirror.removeMessage'),
    confirmLabel: t('integrations.remove'),
    danger: true,
    cancelFocused: true,
  });
  if (!ok) return;
  await busy(t('integrations.remove'), () => rpc('remote.mirror.remove', { board: id }));
  await loadMirrors();
}

// --- Slack -------------------------------------------------------------------------------

export async function postToSlack() {
  const a = await pickAccount((x) => x.provider === 'slack', t('commands.slack.post'));
  if (!a) return;
  const channels = await busy(t('commands.slack.post'), () => rpc<IdName[]>('slack.channels', { account: a.id }));
  if (!channels) return;
  const c = activeCard();
  const remote = c?.board.remote.get(c.id);
  const node = c?.board.node(c.id);
  const draft = node ? (remote ? `${node.title} — ${remote.url}` : node.title) : '';
  const r = await steps<[IdName, string]>([
    () =>
      pickOne<IdName>(
        channels.map((ch) => ({ label: `#${ch.name}`, description: ch.detail ? t('integrations.slack.private') : undefined, value: ch })),
        { title: t('integrations.slack.pickChannel'), step: 1, totalSteps: 2 },
      ),
    () =>
      inputBox({
        title: t('commands.slack.post'),
        prompt: t('integrations.slack.messagePrompt'),
        value: draft,
        step: 2,
        totalSteps: 2,
        validate: (v) => (v.trim() ? null : t('integrations.slack.messageRequired')),
      }),
  ]);
  if (!r) return;
  if ((await runGated({ kind: 'slackPost', account: a.id, channel: r[0].id, channelName: r[0].name, text: r[1] })) !== null)
    toast.success(t('integrations.slack.posted', { channel: r[0].name }));
}

// --- toggles -----------------------------------------------------------------------------

export function toggleAllow(which: 'push' | 'pull') {
  const key = which === 'push' ? 'integrations.allowPush' : 'integrations.allowPull';
  settings.toggle(key);
  const on = settings.get<boolean>(key);
  toast.info(t(`integrations.toggles.${which}${on ? 'On' : 'Off'}`));
}

export async function remoteActions() {
  const r = needRemote();
  if (!r) return;
  const items: QuickItem<string>[] = [
    { label: t('commands.remote.transition'), value: 'remote.transition' },
    { label: t('commands.remote.assign'), value: 'remote.assign' },
    { label: t('commands.remote.comment'), value: 'remote.comment' },
    { label: t('commands.remote.pull'), value: 'remote.pull' },
    ...(r.remote.mirror
      ? []
      : [
          { label: t('commands.remote.push'), value: 'remote.push' },
          { label: t('commands.remote.unlink'), value: 'remote.unlink' },
        ]),
    { label: t('commands.remote.openInBrowser'), value: 'remote.openInBrowser' },
  ];
  const v = await quickPick(items, { title: `${r.remote.key}` });
  if (typeof v === 'string') {
    const { runCommand } = await import('$lib/commands/registry.svelte');
    await runCommand(v);
  }
}
