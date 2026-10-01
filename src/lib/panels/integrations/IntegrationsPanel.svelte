<script lang="ts">
  // Integrations panel: accounts, connect flows, remote search (JQL / Trello
  // query) with saved queries, a draggable issue list, mirrors with watch toggles.
  import { Plug, Search, Bookmark, X, MoreHorizontal, RefreshCw, Trash2, Copy, Eye, Send, ShieldAlert, Diamond, LoaderCircle } from '@lucide/svelte';
  import { rpc } from '$lib/backend/rpc';
  import { t, relTime } from '$lib/i18n/index.svelte';
  import { tip } from '$lib/components/tooltip';
  import Toggle from '$lib/components/Toggle.svelte';
  import { openMenuAt } from '$lib/state/menu.svelte';
  import { settings } from '$lib/settings/store.svelte';
  import { toast } from '$lib/state/toasts.svelte';
  import { openBoardTab } from '$lib/state/workspace.svelte';
  import { openExternal } from '$lib/app/helpers';
  import { startDrag } from '$lib/board/dnd.svelte';
  import { integ, accountById, isIssueAccount, loadMirrors } from '$lib/integrations/state.svelte';
  import * as A from '$lib/integrations/actions';
  import { errorText, serviceName } from '$lib/integrations/gate';
  import type { Account, RemoteIssue } from '$lib/integrations/types';

  const issueAccounts = $derived(integ.accounts.filter(isIssueAccount));
  const slackAccounts = $derived(integ.accounts.filter((a) => a.provider === 'slack'));
  const current = $derived(accountById(integ.account));
  const isJira = $derived(current?.provider === 'jiraCloud' || current?.provider === 'jiraServer');
  const allowPush = $derived(settings.get<boolean>('integrations.allowPush') === true);
  const allowPull = $derived(settings.get<boolean>('integrations.allowPull') !== false);
  let lastSearched = '';

  $effect(() => {
    const id = integ.account;
    if (id && id !== lastSearched) {
      lastSearched = id;
      integ.query = '';
      void A.search();
    }
  });

  function connectMenu(e: MouseEvent) {
    openMenuAt(e.currentTarget as HTMLElement, [
      { label: t('commands.integrations.connectJira'), command: 'integrations.connectJira' },
      { label: t('commands.integrations.connectTrello'), command: 'integrations.connectTrello' },
      { label: t('commands.integrations.connectSlack'), command: 'integrations.connectSlack' },
    ]);
  }

  function accountMenu(e: MouseEvent, a: Account) {
    e.stopPropagation();
    openMenuAt(e.currentTarget as HTMLElement, [
      { label: t('integrations.testConnection'), run: () => void testAccount(a) },
      ...(a.provider === 'slack'
        ? [{ label: t('commands.slack.post'), command: 'slack.post' }]
        : [
            { label: t('integrations.defaultQuery'), run: () => void A.setDefaultQuery() },
            { label: t('commands.integrations.mirrorBoard'), run: () => void A.mirrorBoard(a.id) },
          ]),
      { separator: true },
      { label: t('integrations.remove'), danger: true, run: () => void A.removeAccount(a) },
    ]);
  }

  async function testAccount(a: Account) {
    try {
      const r = await rpc<Account>('integrations.test', { account: a.id });
      toast.success(t('integrations.testOk', { name: r.userName ?? r.label }));
    } catch (e) {
      toast.error(errorText(e));
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      void A.search();
    }
  }

  function drag(e: PointerEvent, i: RemoteIssue) {
    if (!current) return;
    startDrag(e, { kind: 'external', payload: { type: 'remoteIssue', account: current.id, key: i.key }, label: `${i.key} · ${i.summary}` }, null);
  }

  function issueMenu(e: MouseEvent, i: RemoteIssue) {
    e.preventDefault();
    openMenuAt(e.currentTarget as HTMLElement, [
      { label: t('commands.remote.openInBrowser'), run: () => void openExternal(i.url) },
      { label: t('integrations.copyKey'), run: () => void navigator.clipboard?.writeText(i.key) },
    ]);
  }

  async function syncMirror(id: string) {
    try {
      await rpc('remote.mirror.sync', { board: id });
      await loadMirrors();
    } catch (e) {
      toast.error(errorText(e));
    }
  }

  const initials = (n: string) =>
    n
      .split(/\s+/)
      .filter(Boolean)
      .slice(0, 2)
      .map((w) => w[0]?.toUpperCase())
      .join('');
</script>

<div class="panel">
  <header class="top">
    <h2 class="section-title">{t('panels.integrations')}</h2>
    <span class="grow"></span>
    <button class="icon-btn sm" onclick={connectMenu} use:tip={t('integrations.connectService')} aria-label={t('integrations.connectService')}
      ><Plug size={15} /></button
    >
  </header>

  {#if integ.loaded && !integ.accounts.length}
    <div class="empty welcome">
      <div class="empty-icon"><Plug size={22} /></div>
      <p>{t('integrations.emptyTitle')}</p>
      <p class="muted small">{t('integrations.emptyHint')}</p>
      <div class="connect">
        <button class="btn soft" onclick={() => A.connectJira()}>Jira</button>
        <button class="btn soft" onclick={() => A.connectTrello()}>Trello</button>
        <button class="btn soft" onclick={() => A.connectSlack()}>Slack</button>
      </div>
    </div>
  {:else}
    <div class="accounts" role="tablist" aria-label={t('integrations.accounts')}>
      {#each issueAccounts as a (a.id)}
        <div class="acct" class:active={a.id === integ.account}>
          <button role="tab" aria-selected={a.id === integ.account} class="acct-btn" onclick={() => (integ.account = a.id)} use:tip={a.baseUrl}>
            <span class="svc {a.provider}">{serviceName(a.provider.startsWith('jira') ? 'jira' : a.provider)}</span>
            <span class="grow">{a.label}</span>
            {#if a.insecureHttp}<span use:tip={t('integrations.insecureTip')}><ShieldAlert size={13} class="warn" /></span>{/if}
          </button>
          <button class="icon-btn sm" onclick={(e) => accountMenu(e, a)} aria-label={t('common.more')}><MoreHorizontal size={14} /></button>
        </div>
      {/each}
      {#each slackAccounts as a (a.id)}
        <div class="acct">
          <span class="acct-btn static"><span class="svc slack">Slack</span><span class="grow">{a.label}</span></span>
          <button class="icon-btn sm" onclick={() => A.postToSlack()} use:tip={t('commands.slack.post')}><Send size={13} /></button>
          <button class="icon-btn sm" onclick={(e) => accountMenu(e, a)} aria-label={t('common.more')}><MoreHorizontal size={14} /></button>
        </div>
      {/each}
    </div>

    {#if current}
      <section class="search">
        <div class="qbox">
          <Search size={14} class="muted" />
          <textarea
            class="q"
            rows="1"
            bind:value={integ.query}
            onkeydown={onKey}
            placeholder={isJira ? current.defaultQuery || t('integrations.jqlPlaceholder') : t('integrations.trelloPlaceholder')}
            aria-label={isJira ? 'JQL' : t('integrations.search')}
            spellcheck="false"></textarea>
          {#if integ.query}
            <button
              class="icon-btn sm"
              onclick={() => {
                integ.query = '';
                void A.search();
              }}
              aria-label={t('integrations.clear')}><X size={13} /></button
            >
          {/if}
          <button class="icon-btn sm" onclick={() => A.saveQuery()} disabled={!integ.query.trim()} use:tip={t('integrations.saveQuery')}
            ><Bookmark size={13} /></button
          >
        </div>
        <div class="saved">
          <button
            class="chip"
            class:on={!integ.query}
            onclick={() => {
              integ.query = '';
              void A.search();
            }}>{isJira ? t('integrations.myOpenIssues') : t('integrations.allCards')}</button
          >
          {#each current.savedQueries as q (q.name)}
            <span class="chip saved-q" class:on={integ.query === q.query}>
              <button
                onclick={() => {
                  integ.query = q.query;
                  void A.search();
                }}
                use:tip={q.query}>{q.name}</button
              >
              <button class="x" onclick={() => A.removeQuery(q.name)} aria-label={t('common.remove')}><X size={10} /></button>
            </span>
          {/each}
        </div>
      </section>

      <section class="results" aria-busy={integ.searching}>
        {#if integ.error}
          <p class="err small">{integ.error}</p>
        {:else if !integ.results.length && !integ.searching}
          <p class="muted small pad">{t('integrations.noResults')}</p>
        {/if}
        {#each integ.results as i (i.key)}
          <button
            class="issue"
            onpointerdown={(e) => drag(e, i)}
            onclick={() => openExternal(i.url)}
            oncontextmenu={(e) => issueMenu(e, i)}
            use:tip={t('integrations.dragHint')}
          >
            <span class="dot {i.statusCategory}"></span>
            <span class="key">{i.key}</span>
            <span class="sum grow">{i.summary}</span>
            {#if i.assignee}<span class="avatar" title={i.assignee.name}>{initials(i.assignee.name)}</span>{/if}
          </button>
        {/each}
        {#if integ.searching}<div class="pad muted small"><LoaderCircle size={13} class="spin" /> {t('integrations.searching')}</div>{/if}
        {#if integ.next && !integ.searching}
          <button class="btn ghost sm more" onclick={() => A.search(true)}>{t('integrations.loadMore')}</button>
        {/if}
      </section>
    {/if}

    <section class="mirrors">
      <div class="sub">
        <h3 class="section-title"><Diamond size={11} /> {t('integrations.mirrors')}</h3>
        <span class="grow"></span>
        {#if issueAccounts.length}
          <button class="icon-btn sm" onclick={() => A.mirrorBoard(current?.id)} use:tip={t('commands.integrations.mirrorBoard')}><Copy size={13} /></button>
        {/if}
      </div>
      {#if !integ.mirrors.length}
        <p class="muted small pad">{t('integrations.noMirrors')}</p>
      {/if}
      {#each integ.mirrors as m (m.id)}
        <div class="mirror">
          <button class="mname grow" onclick={() => openBoardTab(m.id)}>
            <span>{m.name}</span>
            <span class="muted small">
              {#if m.lastError}<span class="err">{t('integrations.syncFailed')}</span>{:else if m.lastSync}{t('integrations.syncedAgo', {
                  when: relTime(m.lastSync),
                })}{/if}
            </span>
          </button>
          <span use:tip={t('integrations.mirror.watchTip')}><Eye size={12} class="muted" /></span>
          <Toggle checked={m.watch} label={t('integrations.mirror.watch')} onchange={(v) => A.setWatch(m.id, v)} />
          <button class="icon-btn sm" onclick={() => syncMirror(m.id)} use:tip={t('integrations.syncNow')}><RefreshCw size={13} /></button>
          <button class="icon-btn sm" onclick={() => A.removeMirror(m.id, m.name)} use:tip={t('integrations.remove')}><Trash2 size={13} /></button>
        </div>
      {/each}
    </section>

    <footer class="gates">
      <label class="gate"
        ><Toggle checked={allowPull} label={t('commands.remote.toggleAllowPull')} onchange={() => A.toggleAllow('pull')} />
        <span>{t('integrations.allowPull')}</span></label
      >
      <label class="gate"
        ><Toggle checked={allowPush} label={t('commands.remote.toggleAllowPush')} onchange={() => A.toggleAllow('push')} />
        <span>{t('integrations.allowPush')}</span></label
      >
      {#if integ.lastSync}<span class="muted small">{t('integrations.lastCheck', { when: relTime(integ.lastSync) })}</span>{/if}
    </footer>
  {/if}
</div>

<style>
  /* Plain buttons: no native chrome (zero specificity so shared classes win). */
  :where(.panel button) {
    border: none;
    background: transparent;
    padding: 0;
  }
  .panel {
    display: flex;
    flex-direction: column;
    gap: var(--sp-3);
    padding: var(--sp-2) var(--sp-3) var(--sp-4);
    height: 100%;
    overflow: auto;
  }
  .top,
  .sub {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
  }
  .grow {
    flex: 1;
    min-width: 0;
  }
  .muted {
    color: var(--ink-3);
  }
  .small {
    font-size: var(--fs-xs);
  }
  .pad {
    padding: var(--sp-2);
  }
  .err {
    color: var(--danger);
  }
  :global(.warn) {
    color: var(--warn);
  }
  .welcome {
    text-align: center;
  }
  .connect {
    display: flex;
    gap: var(--sp-2);
    justify-content: center;
    margin-top: var(--sp-3);
  }
  .accounts {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .acct {
    display: flex;
    align-items: center;
    gap: 2px;
    border-radius: var(--r-sm);
    transition: background var(--dur-fast) var(--ease);
  }
  .acct:hover {
    background: var(--bg-hover);
  }
  .acct.active {
    background: var(--primary-softer);
  }
  .acct-btn {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    padding: 6px var(--sp-2);
    font-size: var(--fs-sm);
    color: var(--ink);
    text-align: left;
  }
  .acct-btn.static {
    cursor: default;
  }
  .acct-btn .grow {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .svc {
    font-size: 10px;
    font-weight: var(--fw-semibold);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    padding: 1px 6px;
    border-radius: var(--r-pill);
    background: var(--primary-soft);
    color: var(--primary-strong);
  }
  .svc.trello {
    background: var(--info-soft);
    color: var(--info);
  }
  .svc.slack {
    background: var(--ok-soft);
    color: var(--ok);
  }
  .qbox {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    padding: 6px var(--sp-2);
    border-radius: var(--r-md);
    background: var(--bg-sunken);
    box-shadow: inset 0 0 0 1px var(--line);
  }
  .qbox:focus-within {
    box-shadow:
      inset 0 0 0 1px var(--primary),
      0 0 0 3px var(--primary-ring);
  }
  .q {
    flex: 1;
    min-width: 0;
    resize: none;
    border: 0;
    outline: 0;
    background: transparent;
    color: var(--ink);
    font: inherit;
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    line-height: 1.5;
    field-sizing: content;
    max-height: 120px;
  }
  .saved {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin-top: var(--sp-2);
  }
  .saved .chip {
    cursor: pointer;
  }
  .chip.on {
    background: var(--primary-soft);
    color: var(--primary-strong);
  }
  .saved-q {
    padding-right: 2px;
  }
  .saved-q .x {
    display: inline-grid;
    place-items: center;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    opacity: 0.6;
  }
  .saved-q .x:hover {
    opacity: 1;
    background: var(--bg-active);
  }
  .results {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .issue {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    min-height: 30px;
    padding: 4px var(--sp-2);
    border-radius: var(--r-xs);
    text-align: left;
    color: var(--ink-2);
    font-size: var(--fs-sm);
    cursor: grab;
    touch-action: none;
  }
  .issue:hover,
  .issue:focus-visible {
    background: var(--bg-hover);
  }
  .issue .key {
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    color: var(--ink-3);
    white-space: nowrap;
  }
  .issue .sum {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--ink);
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex: none;
    background: var(--ink-4);
  }
  .dot.inProgress {
    background: var(--info);
  }
  .dot.done {
    background: var(--ok);
  }
  .avatar {
    display: inline-grid;
    place-items: center;
    width: 20px;
    height: 20px;
    border-radius: 50%;
    background: var(--primary-soft);
    color: var(--primary-strong);
    font-size: 9px;
    font-weight: var(--fw-semibold);
    flex: none;
  }
  .more {
    align-self: center;
    margin-top: var(--sp-1);
  }
  .mirror {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 2px 0 2px var(--sp-1);
  }
  .mname {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 1px;
    padding: 4px;
    border-radius: var(--r-xs);
    text-align: left;
    font-size: var(--fs-sm);
    color: var(--ink);
    min-width: 0;
  }
  .mname:hover {
    background: var(--bg-hover);
  }
  .mname > span:first-child {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .gates {
    margin-top: auto;
    display: flex;
    flex-direction: column;
    gap: var(--sp-2);
    padding-top: var(--sp-3);
    border-top: 1px solid var(--line);
  }
  .gate {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    font-size: var(--fs-sm);
    color: var(--ink-2);
  }
</style>
