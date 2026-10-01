<script lang="ts">
  import { History, SlidersHorizontal, RefreshCw, Sparkles, X, Search } from '@lucide/svelte';
  import { rpc, onCoreEvent } from '$lib/backend/rpc';
  import type { HistoryFilter, JournalEntry } from '$lib/backend/types';
  import { boards } from '$lib/state/boards.svelte';
  import { openMenu, type MenuItem } from '$lib/state/menu.svelte';
  import { openCard } from '$lib/app/open';
  import { commandsVersion, getCommand, runCommand } from '$lib/commands/registry.svelte';
  import { tip } from '$lib/components/tooltip';
  import { t, fmtDate, i18n } from '$lib/i18n/index.svelte';
  import { hist, openDiff, showCardHistory, clearCardFocus, showHistoryView } from './historyState.svelte';
  import { boardName, knownBoards } from './actions';
  import { KIND_GROUPS, describeEntry, entryCardId, filterEntries, groupByDay, hasVersions, kindGroup, rangeFrom, sourceOf, type RangePreset } from './model';
  import { entryIcon } from './icons';

  let entries = $state<JournalEntry[]>([]);
  let loading = $state(false);
  let failed = $state(false);
  let hasMore = $state(false);
  let seq = 0;

  const RANGES: RangePreset[] = ['week', 'month', 'today', 'all'];
  const SUMMARY_COMMANDS = ['activity.summarize', 'summary.activity', 'summary.create'];

  const summarizeCmd = $derived.by(() => {
    void commandsVersion.v;
    return SUMMARY_COMMANDS.find((id) => getCommand(id)) ?? null;
  });

  function anyTitle(id: string): string | null {
    for (const b of boards.values()) {
      const n = b.node(id);
      if (n) return n.title;
    }
    return null;
  }

  async function load(board: string, cardId: string | null, range: RangePreset, limit: number) {
    const my = ++seq;
    loading = true;
    const filter: HistoryFilter = { limit: limit + 1 };
    if (cardId) filter.ids = [cardId];
    else {
      const from = rangeFrom(range);
      if (from) filter.from = from;
    }
    try {
      // A card's history spans boards (cross-board moves), so card focus searches everywhere.
      const res = await rpc<JournalEntry[]>('history.queryAll', { boards: board && !cardId ? [board] : [], filter });
      if (my !== seq) return;
      hasMore = res.length > limit;
      entries = res.slice(0, limit);
      failed = false;
    } catch {
      if (my === seq) failed = true;
    } finally {
      if (my === seq) loading = false;
    }
  }

  $effect(() => {
    void hist.refresh;
    void load(hist.board, hist.card?.id ?? null, hist.range, hist.limit);
  });

  // Live updates: board changes are journaled; reload shortly after (edits are journaled lazily).
  $effect(() => {
    let timer: ReturnType<typeof setTimeout> | null = null;
    const off = onCoreEvent((e) => {
      if (e.type !== 'boardDelta' && e.type !== 'externalChange') return;
      if (timer) clearTimeout(timer);
      timer = setTimeout(() => hist.refresh++, 1200);
    });
    return () => {
      off();
      if (timer) clearTimeout(timer);
    };
  });

  const visible = $derived(filterEntries(entries, { groups: hist.groups, sources: hist.sources, text: hist.text }, boardName, anyTitle));
  const days = $derived(groupByDay(visible));
  const sourcesSeen = $derived([...new Set(['you', 'external', ...entries.map((e) => sourceOf(e))])]);
  const filtered = $derived(hist.groups.length > 0 || hist.sources.length > 0 || !!hist.text.trim());
  const showBoard = $derived(!hist.board || !!hist.card);

  function sourceLabel(s: string): string {
    const k = `history.sources.${s}`;
    const v = t(k);
    return v === k ? s : v;
  }

  function describe(e: JournalEntry): string {
    const d = describeEntry(e, boardName, (id) => boards.get(e.board)?.node(id)?.title ?? anyTitle(id));
    const params = { ...d.params };
    if (typeof params.service === 'string') params.service = sourceLabel(params.service);
    if (params.title === '') params.title = t('common.untitled');
    return t(`history.desc.${d.key}`, params);
  }

  function time(ts: string): string {
    return new Date(ts).toLocaleTimeString(i18n.locale, { hour: '2-digit', minute: '2-digit' });
  }

  function dayLabel(key: string, rel: string): string {
    if (rel === 'today') return t('common.today');
    if (rel === 'yesterday') return t('common.yesterday');
    return fmtDate(key, { weekday: 'long', month: 'short', day: 'numeric' });
  }

  function charsOf(e: JournalEntry): number | null {
    const d = e.details as { chars?: unknown } | null | undefined;
    return d && typeof d.chars === 'number' && d.chars !== 0 ? d.chars : null;
  }

  function open(e: JournalEntry) {
    const card = entryCardId(e);
    const siblings = card && hasVersions(e) ? entries.filter((x) => hasVersions(x) && entryCardId(x) === card) : [];
    openDiff(e, siblings);
  }

  function toggle<T>(list: T[], v: T): T[] {
    return list.includes(v) ? list.filter((x) => x !== v) : [...list, v];
  }

  function filterMenu(ev: MouseEvent) {
    const items: MenuItem[] = [
      ...sourcesSeen.map((s) => ({ label: sourceLabel(s), checked: hist.sources.includes(s), run: () => (hist.sources = toggle(hist.sources, s)) })),
      { separator: true },
      ...KIND_GROUPS.map((g) => ({ label: t(`history.groups.${g}`), checked: hist.groups.includes(g), run: () => (hist.groups = toggle(hist.groups, g)) })),
      { separator: true },
      { label: t('history.clearFilters'), icon: X, disabled: !filtered, run: () => ((hist.groups = []), (hist.sources = []), (hist.text = '')) },
    ];
    openMenu(ev, items);
  }

  function rowMenu(ev: MouseEvent, e: JournalEntry) {
    ev.preventDefault();
    const card = entryCardId(e);
    const onBoard = card ? boards.get(e.board)?.node(card) : undefined;
    const items: MenuItem[] = [
      { label: t('history.actions.compare'), disabled: !hasVersions(e), run: () => open(e) },
      { label: t('history.actions.openCard'), disabled: !card, run: () => card && void openCard(e.board, card) },
      {
        label: t('history.actions.showCard'),
        icon: History,
        disabled: !card,
        run: () => card && showCardHistory(e.board, card, onBoard?.title ?? describe(e)),
      },
    ];
    if (e.kind === 'trash') items.push({ separator: true }, { label: t('history.actions.goToTrash'), run: () => showHistoryView('trash', e.board) });
    openMenu(ev, items);
  }

  function listKey(ev: KeyboardEvent) {
    if (ev.key !== 'ArrowDown' && ev.key !== 'ArrowUp') return;
    const list = (ev.currentTarget as HTMLElement).closest('.list');
    const rows = [...(list?.querySelectorAll<HTMLElement>('[data-entry]') ?? [])];
    const i = rows.indexOf(document.activeElement as HTMLElement);
    const next = rows[Math.max(0, Math.min(rows.length - 1, i + (ev.key === 'ArrowDown' ? 1 : -1)))];
    if (next) {
      ev.preventDefault();
      next.focus();
    }
  }
</script>

<div class="toolbar">
  <label class="search">
    <Search size={14} strokeWidth={1.8} />
    <input class="bare" type="search" placeholder={t('history.searchPlaceholder')} bind:value={hist.text} />
  </label>
  <button class="icon-btn" class:active={filtered} onclick={filterMenu} use:tip={t('history.filters')} aria-label={t('history.filters')}>
    <SlidersHorizontal size={15} strokeWidth={1.8} />
  </button>
  <button
    class="icon-btn"
    onclick={() => hist.refresh++}
    use:tip={{ text: t('history.refresh'), command: 'history.refresh' }}
    aria-label={t('history.refresh')}
  >
    <RefreshCw size={15} strokeWidth={1.8} class={loading ? 'spin' : ''} />
  </button>
  {#if summarizeCmd}
    <button
      class="icon-btn"
      onclick={() => void runCommand(summarizeCmd)}
      use:tip={{ text: t('history.summarize'), command: summarizeCmd }}
      aria-label={t('history.summarize')}
    >
      <Sparkles size={15} strokeWidth={1.8} />
    </button>
  {/if}
</div>

{#if hist.card}
  <div class="focus">
    <History size={14} strokeWidth={1.8} />
    <span class="grow">{t('history.card.label', { title: hist.card.title || t('common.untitled') })}</span>
    <button class="icon-btn sm" onclick={clearCardFocus} use:tip={t('history.card.clear')} aria-label={t('history.card.clear')}><X size={14} /></button>
  </div>
{:else}
  <div class="selects">
    <select class="field slim" bind:value={hist.board} aria-label={t('history.board')}>
      <option value="">{t('history.allBoards')}</option>
      {#each knownBoards() as b (b.id)}<option value={b.id}>{b.name}</option>{/each}
    </select>
    <select class="field slim" bind:value={hist.range} aria-label={t('history.range.all')}>
      {#each RANGES as r (r)}<option value={r}>{t(`history.range.${r}`)}</option>{/each}
    </select>
  </div>
{/if}

<div class="list">
  {#if failed}
    <div class="empty">{t('errors.opFailed', { message: '' })}</div>
  {:else if !days.length && !loading}
    <div class="empty">
      <div class="empty-icon"><History size={20} strokeWidth={1.8} /></div>
      <strong>{filtered ? t('history.empty.filtered') : t('history.empty.title')}</strong>
      {#if !filtered}<span class="hint">{t('history.empty.hint')}</span>{/if}
    </div>
  {/if}
  {#each days as day (day.key)}
    <div class="day section-title">{dayLabel(day.key, day.rel)}</div>
    {#each day.entries as e, i (e.ts + e.board + e.kind + i)}
      {@const group = kindGroup(e)}
      {@const Icon = entryIcon(group, e.kind)}
      {@const src = sourceOf(e)}
      {@const chars = charsOf(e)}
      <button class="entry" data-entry onkeydown={listKey} onclick={() => open(e)} oncontextmenu={(ev) => rowMenu(ev, e)}>
        <span class="ic g-{group}"><Icon size={13} strokeWidth={1.9} /></span>
        <span class="body">
          <span class="text">{describe(e)}</span>
          <span class="meta">
            <span>{time(e.ts)}</span>
            {#if showBoard && boardName(e.board)}<span class="dot">·</span><span class="ellipsis">{boardName(e.board)}</span>{/if}
            {#if src !== 'you'}<span class="chip src s-{src === 'external' ? 'external' : 'remote'}">{sourceLabel(src)}</span>{/if}
            {#if chars !== null}<span class="delta" class:neg={chars < 0}
                >{chars > 0 ? t('history.chars.add', { count: chars }) : t('history.chars.del', { count: -chars })}</span
              >{/if}
          </span>
        </span>
      </button>
    {/each}
  {/each}
  {#if hasMore}
    <button class="btn ghost sm more" onclick={() => (hist.limit += 200)}>{t('history.loadMore')}</button>
  {/if}
</div>

<style>
  .toolbar {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 0 10px 6px;
  }
  .search {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 6px;
    height: 28px;
    padding: 0 8px;
    border-radius: var(--r-sm);
    background: var(--bg-hover);
    color: var(--ink-3);
    margin-right: 4px;
  }
  .search:focus-within {
    box-shadow: 0 0 0 3px var(--focus);
  }
  .bare {
    flex: 1;
    min-width: 0;
    border: none;
    outline: none;
    background: transparent;
    color: var(--ink);
    font: inherit;
    font-size: var(--fs-sm);
  }
  .selects {
    display: flex;
    gap: 6px;
    padding: 0 10px 8px;
  }
  .field.slim {
    flex: 1;
    min-width: 0;
    height: 28px;
    padding-top: 0;
    padding-bottom: 0;
    font-size: var(--fs-sm);
  }
  .focus {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 10px 8px;
    padding: 4px 4px 4px 10px;
    border-radius: var(--r-sm);
    background: var(--primary-soft);
    color: var(--primary-strong);
    font-size: var(--fs-sm);
    font-weight: var(--fw-medium);
  }
  .grow {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 0 6px 16px;
  }
  .day {
    position: sticky;
    top: 0;
    z-index: 1;
    padding: 10px 8px 4px;
    background: color-mix(in srgb, var(--bg-sunken) 88%, transparent);
    backdrop-filter: blur(8px);
  }
  .entry {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    width: 100%;
    padding: 7px 8px;
    border: none;
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--ink-2);
    text-align: left;
    font: inherit;
    cursor: default;
    transition: background var(--dur-fast) var(--ease);
  }
  .entry:hover {
    background: var(--bg-hover);
  }
  .entry:focus-visible {
    outline: none;
    background: var(--primary-softer);
    box-shadow: inset 0 0 0 1px var(--primary-ring);
  }
  .ic {
    flex-shrink: 0;
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    margin-top: 1px;
    border-radius: 7px;
    background: var(--bg-hover);
    color: var(--ink-3);
  }
  .g-created {
    background: var(--ok-soft);
    color: var(--ok);
  }
  .g-moved {
    background: var(--primary-soft);
    color: var(--primary);
  }
  .g-edited {
    background: var(--info-soft);
    color: var(--info);
  }
  .g-deleted {
    background: var(--danger-soft);
    color: var(--danger);
  }
  .g-archived {
    background: var(--warn-soft);
    color: var(--warn);
  }
  .g-external,
  .g-integrations {
    background: var(--secondary);
    color: var(--secondary-ink);
  }
  .body {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .text {
    font-size: var(--fs-md);
    color: var(--ink);
    line-height: 1.35;
    overflow-wrap: anywhere;
  }
  .meta {
    display: flex;
    align-items: center;
    gap: 5px;
    min-width: 0;
    font-size: var(--fs-xs);
    color: var(--ink-3);
  }
  .ellipsis {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .dot {
    color: var(--ink-4);
  }
  .chip.src {
    height: 16px;
    line-height: 16px;
    padding: 0 6px;
    font-size: 10.5px;
  }
  .s-external {
    background: var(--warn-soft);
    color: var(--warn);
  }
  .s-remote {
    background: var(--info-soft);
    color: var(--info);
  }
  .delta {
    color: var(--ok);
    font-variant-numeric: tabular-nums;
  }
  .delta.neg {
    color: var(--danger);
  }
  .hint {
    font-size: var(--fs-sm);
    max-width: 220px;
  }
  .more {
    display: block;
    margin: 8px auto 0;
  }
</style>
