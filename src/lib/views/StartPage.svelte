<script lang="ts">
  import { Plus, FolderOpen, FilePlus2, Download, Command, LayoutGrid, FileText, Diamond, Clock, Plug, MessageSquare, SquareKanban } from '@lucide/svelte';
  import { rpc } from '$lib/backend/rpc';
  import type { JournalEntry } from '$lib/backend/types';
  import { runCommand, getCommand } from '$lib/commands/registry.svelte';
  import { registry } from '$lib/state/registry.svelte';
  import { uiGet } from '$lib/state/persist.svelte';
  import { boards } from '$lib/state/boards.svelte';
  import { openBoardTab } from '$lib/state/workspace.svelte';
  import { showSection } from '$lib/state/ui.svelte';
  import { openCard } from '$lib/app/open';
  import Kbd from '$lib/components/Kbd.svelte';
  import { t, relTime } from '$lib/i18n/index.svelte';
  import { dayPart } from './start/greeting';

  const part = dayPart(new Date().getHours());
  const actions = [
    { icon: Plus, label: 'start.newBoard', command: 'board.new' },
    { icon: FolderOpen, label: 'start.openFolder', command: 'board.open' },
    { icon: FilePlus2, label: 'start.newCard', command: 'card.new' },
    { icon: Download, label: 'start.import', command: 'board.import' },
    { icon: Command, label: 'start.palette', command: 'palette.commands' },
  ];

  const recentBoards = $derived.by(() => {
    const ids = uiGet<{ id: string }[]>('recentBoards', []).map((r) => r.id);
    const byRecent = registry.data.boards
      .filter((b) => !b.hidden && !b.missing)
      .sort((a, b) => {
        const ia = ids.indexOf(a.id);
        const ib = ids.indexOf(b.id);
        if (ia >= 0 || ib >= 0) return (ia < 0 ? 999 : ia) - (ib < 0 ? 999 : ib);
        return (b.lastOpened ?? 0) - (a.lastOpened ?? 0);
      });
    return byRecent.slice(0, 8);
  });
  const recentCards = $derived(
    uiGet<{ boardId: string; cardId: string; at: number }[]>('recentCards', [])
      .concat(uiGet('recentDocs', []))
      .sort((a, b) => (b.at ?? 0) - (a.at ?? 0))
      .slice(0, 8),
  );

  let activity = $state<JournalEntry[]>([]);
  $effect(() => {
    void rpc<JournalEntry[]>('history.queryAll', { filter: { limit: 12 } })
      .then((r) => (activity = r))
      .catch(() => (activity = []));
  });

  function cardTitle(boardId: string, cardId: string) {
    return boards.get(boardId)?.node(cardId)?.title || t('common.untitled');
  }

  function entryTitle(e: JournalEntry): string {
    const d = e.details as { after?: { items?: { title?: string }[] }; title?: string; titles?: string[] } | undefined;
    return d?.after?.items?.[0]?.title ?? d?.title ?? d?.titles?.[0] ?? '';
  }

  function connect(cmd: string) {
    if (getCommand(cmd)) void runCommand(cmd);
    else showSection('integrations');
  }
</script>

<div class="start">
  <div class="inner">
    <header>
      <h1>{t(`start.greeting.${part}`)}</h1>
      <p class="muted">{t('start.subtitle')}</p>
    </header>

    <section class="actions">
      {#each actions as a (a.command)}
        <button class="action" onclick={() => runCommand(a.command)}>
          <span class="ic"><a.icon size={18} strokeWidth={1.8} /></span>
          <span class="label">{t(a.label)}</span>
          <Kbd command={a.command} />
        </button>
      {/each}
    </section>

    <div class="grid">
      <section>
        <h2 class="section-title">{t('start.recentBoards')}</h2>
        {#each recentBoards as b (b.id)}
          <button class="row" onclick={() => openBoardTab(b.id)}>
            {#if b.mirror}<Diamond size={15} />{:else if b.kind === 'files'}<FileText size={15} />{:else}<LayoutGrid size={15} />{/if}
            <span class="grow">{b.name}</span>
            {#if b.lastOpened}<span class="muted small">{relTime(b.lastOpened)}</span>{/if}
          </button>
        {:else}
          <p class="muted small">{t('start.noBoards')}</p>
        {/each}

        <h2 class="section-title gap">{t('start.recentCards')}</h2>
        {#each recentCards as r (r.boardId + r.cardId)}
          <button class="row" onclick={() => openCard(r.boardId, r.cardId)}>
            <Clock size={15} />
            <span class="grow">{cardTitle(r.boardId, r.cardId)}</span>
            <span class="muted small">{boards.get(r.boardId)?.header.name ?? ''}</span>
          </button>
        {:else}
          <p class="muted small">{t('start.noCards')}</p>
        {/each}
      </section>

      <section>
        <h2 class="section-title">{t('start.activity')}</h2>
        {#each activity as e, i (e.ts + i)}
          <div class="row static">
            <span class="dot" class:ext={e.origin !== 'you'}></span>
            <span class="grow"
              >{e.label}{#if entryTitle(e)}
                · <em>{entryTitle(e)}</em>{/if}</span
            >
            <span class="muted small">{relTime(e.ts)}</span>
          </div>
        {:else}
          <p class="muted small">{t('start.noActivity')}</p>
        {/each}

        <h2 class="section-title gap">{t('start.integrations')}</h2>
        <div class="integrations">
          <button class="integ" onclick={() => connect('integrations.connectJira')}><Plug size={16} /> Jira</button>
          <button class="integ" onclick={() => connect('integrations.connectTrello')}><SquareKanban size={16} /> Trello</button>
          <button class="integ" onclick={() => connect('integrations.connectSlack')}><MessageSquare size={16} /> Slack</button>
        </div>
      </section>
    </div>
  </div>
</div>

<style>
  .start {
    flex: 1;
    overflow-y: auto;
  }
  .inner {
    max-width: 920px;
    margin: 0 auto;
    padding: 56px 32px 48px;
  }
  h1 {
    margin: 0;
    font-size: var(--fs-3xl);
    font-weight: 650;
    letter-spacing: -0.02em;
    background: var(--sunset);
    -webkit-background-clip: text;
    background-clip: text;
    color: transparent;
    width: fit-content;
  }
  header p {
    margin: 6px 0 0;
    font-size: var(--fs-lg);
  }
  .actions {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
    gap: 12px;
    margin: 32px 0 40px;
  }
  .action {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 10px;
    padding: 16px;
    border: none;
    border-radius: var(--r-lg);
    background: var(--bg-elev);
    box-shadow:
      var(--shadow-1),
      0 0 0 1px var(--line);
    text-align: left;
    transition:
      transform var(--dur) var(--ease-spring),
      box-shadow var(--dur-fast) var(--ease);
  }
  .action:hover {
    transform: translateY(-2px);
    box-shadow:
      var(--shadow-2),
      0 0 0 1px var(--line-strong);
  }
  .ic {
    width: 34px;
    height: 34px;
    display: grid;
    place-items: center;
    border-radius: var(--r-sm);
    background: var(--primary-soft);
    color: var(--primary-strong);
  }
  .label {
    font-weight: var(--fw-medium);
  }
  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 40px;
  }
  @media (max-width: 760px) {
    .grid {
      grid-template-columns: 1fr;
    }
  }
  .section-title {
    margin: 0 0 8px;
  }
  .section-title.gap {
    margin-top: 28px;
  }
  .row {
    width: 100%;
    border: none;
    background: transparent;
    text-align: left;
    min-height: 34px;
  }
  .row.static:hover {
    background: transparent;
  }
  .small {
    font-size: var(--fs-xs);
    white-space: nowrap;
  }
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--primary);
    flex-shrink: 0;
  }
  .dot.ext {
    background: var(--secondary-strong);
  }
  em {
    font-style: normal;
    color: var(--ink);
  }
  .integrations {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }
  .integ {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 10px 16px;
    border: none;
    border-radius: var(--r-md);
    background: var(--bg-elev);
    box-shadow: 0 0 0 1px var(--line);
    font-weight: var(--fw-medium);
  }
  .integ:hover {
    background: var(--primary-softer);
  }
</style>
