<script lang="ts">
  // A saved search rendered as a read-mostly board: lanes are groups (board,
  // lane or tag); cards are the real cards — opening, dragging and the context
  // menu act on them. Results refresh when any open board changes.
  import { tick, untrack } from 'svelte';
  import { Search, RefreshCw, BookmarkPlus, Pencil, Loader, LayoutGrid, Layers, Hash, FileText, Diamond, CalendarDays, Flag, Archive } from '@lucide/svelte';
  import type { SearchHit } from '$lib/backend/types';
  import type { Tab } from '$lib/state/workspace.svelte';
  import { persistWorkspace } from '$lib/state/workspace.svelte';
  import { boards, openBoard } from '$lib/state/boards.svelte';
  import { boardEntry, registry } from '$lib/state/registry.svelte';
  import { settings } from '$lib/settings/store.svelte';
  import { openMenu } from '$lib/state/menu.svelte';
  import { startDrag } from '$lib/board/dnd.svelte';
  import Segmented from '$lib/components/Segmented.svelte';
  import TagChip from '$lib/components/TagChip.svelte';
  import { tip } from '$lib/components/tooltip';
  import { t, fmtDate } from '$lib/i18n/index.svelte';
  import { parse, highlightTerms } from '$lib/panels/search/query';
  import { effectiveCase, effectiveText } from '$lib/panels/search/sync';
  import {
    runSearch,
    groupByBoard,
    groupByLane,
    groupByTag,
    highlightSegments,
    snippetSegments,
    search,
    type GroupBy,
  } from '$lib/panels/search/searchState.svelte';
  import { openHit, hitMenu, focusSearch, setSavedGroupBy, saveCurrent } from '$lib/panels/search/searchActions';

  let { tab }: { tab: Tab } = $props();

  const q = $derived(String(tab.payload?.q ?? ''));
  const name = $derived(String(tab.payload?.name ?? q));
  const savedId = $derived(tab.payload?.id ? String(tab.payload.id) : undefined);
  const groupBy = $derived((['board', 'lane', 'tag'].includes(String(tab.payload?.groupBy)) ? tab.payload!.groupBy : 'lane') as GroupBy);

  let hits = $state<SearchHit[]>([]);
  let busy = $state(true);
  let failed = $state('');
  let scroller: HTMLDivElement | undefined = $state();

  const caseDefault = $derived(settings.get<boolean>('search.caseSensitive'));
  const terms = $derived(highlightTerms(parse(q)));
  const caseOn = $derived(effectiveCase(q, caseDefault));
  const groups = $derived(
    groupBy === 'board' ? groupByBoard(hits) : groupBy === 'tag' ? groupByTag(hits, t('vboard.noTag')) : groupByLane(hits, t('searchPanel.noLane')),
  );
  const boardsVersion = $derived([...boards.values()].reduce((n, b) => n + b.version, 0));

  let seq = 0;
  let timer: ReturnType<typeof setTimeout> | null = null;
  $effect(() => {
    const text = effectiveText(q, caseDefault);
    void boardsVersion;
    void registry.indexed.done;
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => void run(text), untrack(() => hits.length) ? 250 : 0);
    return () => {
      if (timer) clearTimeout(timer);
    };
  });

  async function run(text: string) {
    const my = ++seq;
    busy = true;
    try {
      const r = await runSearch(text, { limit: 500, includeArchived: settings.get<boolean>('search.includeArchived') });
      if (my !== seq) return;
      hits = r;
      failed = '';
      // Load the boards behind the hits so faces show due dates, priorities and menus.
      for (const b of new Set(r.map((h) => h.board))) if (!boards.get(b)) void openBoard({ id: b }).catch(() => undefined);
    } catch (e) {
      if (my === seq) failed = (e as Error).message;
    } finally {
      if (my === seq) busy = false;
    }
  }

  function setGroupBy(v: GroupBy) {
    tab.payload = { ...tab.payload, groupBy: v };
    setSavedGroupBy(savedId, v);
    persistWorkspace();
  }

  async function save() {
    search.q = q;
    const s = await saveCurrent();
    if (s) {
      tab.payload = { ...tab.payload, id: s.id, name: s.name, groupBy };
      setSavedGroupBy(s.id, groupBy);
      persistWorkspace();
    }
  }

  function node(h: SearchHit) {
    return boards.get(h.board)?.node(h.id);
  }

  function isMirror(h: SearchHit) {
    return !!boardEntry(h.board)?.mirror || !!boards.get(h.board)?.header.readOnly?.startsWith('mirror');
  }

  function onpointerdown(e: PointerEvent, h: SearchHit) {
    if (e.button !== 0 || !boards.get(h.board)) return;
    startDrag(
      e,
      { kind: 'cards', boardId: h.board, ids: [h.id], copyOnly: isMirror(h), label: h.title },
      e.currentTarget as HTMLElement,
      scroller ? [scroller] : [],
    );
  }

  async function oncontextmenu(e: MouseEvent, h: SearchHit) {
    e.preventDefault();
    const at = { x: e.clientX, y: e.clientY };
    openMenu(at, await hitMenu(h));
  }

  // Keyboard: ↑/↓ within a lane, ←/→ across lanes, Enter opens.
  function onkeydown(e: KeyboardEvent) {
    const cur = (document.activeElement as HTMLElement | null)?.closest<HTMLElement>('[data-vcard]');
    const lanes = [...(scroller?.querySelectorAll<HTMLElement>('[data-vlane]') ?? [])];
    const cardsOf = (l: HTMLElement) => [...l.querySelectorAll<HTMLElement>('[data-vcard]')];
    if (!['ArrowDown', 'ArrowUp', 'ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(e.key)) return;
    e.preventDefault();
    if (!cur) {
      lanes
        .map(cardsOf)
        .find((c) => c.length)?.[0]
        ?.focus();
      return;
    }
    const lane = cur.closest<HTMLElement>('[data-vlane]')!;
    const li = lanes.indexOf(lane);
    const list = cardsOf(lane);
    const ci = list.indexOf(cur);
    let next: HTMLElement | undefined;
    if (e.key === 'ArrowDown') next = list[ci + 1];
    else if (e.key === 'ArrowUp') next = list[ci - 1];
    else if (e.key === 'Home') next = list[0];
    else if (e.key === 'End') next = list[list.length - 1];
    else {
      const d = e.key === 'ArrowRight' ? 1 : -1;
      for (let j = li + d; j >= 0 && j < lanes.length && !next; j += d) {
        const other = cardsOf(lanes[j]);
        next = other[Math.min(ci, other.length - 1)];
      }
    }
    next?.focus();
    next?.scrollIntoView({ block: 'nearest', inline: 'nearest' });
  }

  $effect(() => {
    void groupBy;
    void tick().then(() => scroller?.scrollTo({ left: 0 }));
  });

  const today = new Date().toISOString().slice(0, 10);
  const groupIcon = $derived(groupBy === 'board' ? LayoutGrid : groupBy === 'tag' ? Hash : Layers);
</script>

<div class="vboard">
  <header class="vhead">
    <div class="titlebox">
      <span class="badge"><Search size={14} strokeWidth={2} /></span>
      <div class="tt">
        <h1>{name}</h1>
        <code class="qtext" title={q}>{q}</code>
      </div>
    </div>
    <div class="tools">
      {#if busy}<Loader size={14} class="spin muted" />{/if}
      <span class="count">{t('searchPanel.results', { count: hits.length })}</span>
      <Segmented
        size="sm"
        options={[
          { value: 'board' as GroupBy, label: t('searchPanel.byBoard') },
          { value: 'lane' as GroupBy, label: t('searchPanel.byLane') },
          { value: 'tag' as GroupBy, label: t('vboard.byTag') },
        ]}
        value={groupBy}
        onchange={setGroupBy}
      />
      <button class="icon-btn" aria-label={t('vboard.refresh')} use:tip={t('vboard.refresh')} onclick={() => void run(effectiveText(q, caseDefault))}
        ><RefreshCw size={15} /></button
      >
      <button class="icon-btn" aria-label={t('vboard.editQuery')} use:tip={t('vboard.editQuery')} onclick={() => focusSearch(q)}><Pencil size={15} /></button>
      {#if !savedId}
        <button class="icon-btn" aria-label={t('commands.search.saveCurrent')} use:tip={t('commands.search.saveCurrent')} onclick={() => void save()}
          ><BookmarkPlus size={16} /></button
        >
      {/if}
    </div>
  </header>

  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="scroll" bind:this={scroller} data-autoscroll {onkeydown}>
    {#if failed}
      <div class="empty"><strong>{t('searchPanel.failed')}</strong><span>{failed}</span></div>
    {:else if !hits.length && !busy}
      <div class="empty">
        <div class="empty-icon"><Search size={20} /></div>
        <strong>{t('searchPanel.noResults')}</strong>
        <span>{t('vboard.emptyHint')}</span>
      </div>
    {:else}
      <div class="lanes">
        {#each groups as g (g.key)}
          {@const GIcon = groupIcon}
          <section class="vlane" data-vlane aria-label={g.label}>
            <header class="lhead">
              <GIcon size={13} strokeWidth={1.9} />
              <span class="lname">{g.label}</span>
              <span class="lcount">{g.hits.length}</span>
            </header>
            <div class="cards">
              {#each g.hits as h (h.board + h.id)}
                {@const n = node(h)}
                {@const due = n?.footer.due ?? null}
                {@const title = highlightSegments(h.title || t('common.untitled'), terms, caseOn)}
                {@const snip = h.snippet.includes('\u0002') ? snippetSegments(h.snippet) : highlightSegments(h.snippet, terms, caseOn)}
                <button
                  class="vcard"
                  class:archived={h.archived}
                  data-vcard
                  onpointerdown={(e) => onpointerdown(e, h)}
                  onclick={(e) => void openHit(h, e.metaKey || e.ctrlKey)}
                  oncontextmenu={(e) => void oncontextmenu(e, h)}
                  onkeydown={(e) => {
                    if (e.key === 'Enter') {
                      e.preventDefault();
                      void openHit(h, e.metaKey || e.ctrlKey);
                    }
                  }}
                >
                  <span class="ct">
                    {#if isMirror(h)}<Diamond size={11} strokeWidth={2} class="muted" />{:else if h.kind === 'doc'}<FileText
                        size={12}
                        strokeWidth={1.8}
                        class="muted"
                      />{/if}
                    {#if h.archived}<Archive size={11} strokeWidth={2} class="muted" />{/if}
                    <span
                      >{#each title as s, i (i)}{#if s.hit}<mark>{s.text}</mark>{:else}{s.text}{/if}{/each}</span
                    >
                  </span>
                  {#if h.snippet.trim() && h.snippet.trim() !== h.title}
                    <span class="cs"
                      >{#each snip as s, i (i)}{#if s.hit}<mark>{s.text}</mark>{:else}{s.text}{/if}{/each}</span
                    >
                  {/if}
                  {#if h.tags.length}
                    <span class="ctags"
                      >{#each h.tags.slice(0, 4) as tag (tag)}<TagChip {tag} />{/each}</span
                    >
                  {/if}
                  <span class="cm">
                    {#if groupBy !== 'board'}<span class="where"
                        >{h.boardName}{#if h.laneName && groupBy !== 'lane'}
                          · {h.laneName}{/if}</span
                      >{:else if h.laneName}<span class="where">{h.laneName}</span>{/if}
                    {#if h.remoteKey}<span class="key">{h.remoteKey}</span>{/if}
                    {#if n?.footer.priority}<span class="prio"><Flag size={10} strokeWidth={2} />{t(`priority.${n.footer.priority}`)}</span>{/if}
                    {#if due}<span class="due" class:overdue={due < today}><CalendarDays size={10} strokeWidth={2} />{fmtDate(due)}</span>{/if}
                    {#if n && n.tasks.total}<span class="tasks">{n.tasks.done}/{n.tasks.total}</span>{/if}
                  </span>
                </button>
              {/each}
            </div>
          </section>
        {/each}
      </div>
    {/if}
  </div>
</div>

<style>
  .vboard {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .vhead {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 14px 20px 10px 24px;
  }
  .titlebox {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
    flex: 1;
  }
  .badge {
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    border-radius: var(--r-sm);
    background: var(--primary-soft);
    color: var(--primary-strong);
    flex-shrink: 0;
  }
  .tt {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  h1 {
    margin: 0;
    font-size: var(--fs-xl);
    font-weight: var(--fw-semibold);
    letter-spacing: -0.01em;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .qtext {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--ink-4);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .tools {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .count {
    font-size: var(--fs-sm);
    color: var(--ink-3);
    margin-right: 4px;
  }
  .tools :global(.muted),
  .vcard :global(.muted) {
    color: var(--ink-4);
    flex-shrink: 0;
  }
  .scroll {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 4px 20px 24px;
  }
  .lanes {
    display: flex;
    gap: 14px;
    align-items: flex-start;
    width: max-content;
    min-width: 100%;
  }
  .vlane {
    width: var(--lane-w);
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    border-radius: var(--r-lg);
    background: var(--bg-lane);
    padding: 6px 8px 10px;
  }
  .lhead {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 34px;
    padding: 0 6px;
    color: var(--ink-3);
  }
  .lname {
    flex: 1;
    min-width: 0;
    font-weight: var(--fw-semibold);
    color: var(--ink);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .lcount {
    font-size: var(--fs-xs);
    color: var(--ink-4);
  }
  .cards {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .vcard {
    display: flex;
    flex-direction: column;
    gap: 6px;
    width: 100%;
    padding: 10px 12px;
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    background: var(--bg-card);
    box-shadow: var(--shadow-1);
    color: var(--ink-2);
    text-align: left;
    transition:
      box-shadow var(--dur-fast) var(--ease),
      transform var(--dur-fast) var(--ease);
  }
  .vcard:hover {
    box-shadow: var(--shadow-2);
  }
  .vcard:focus-visible {
    outline: none;
    box-shadow: 0 0 0 3px var(--focus);
  }
  .vcard.archived {
    opacity: 0.6;
  }
  .ct {
    display: flex;
    align-items: baseline;
    gap: 5px;
    color: var(--ink);
    font-weight: var(--fw-medium);
    line-height: 1.35;
  }
  .cs {
    font-size: var(--fs-sm);
    color: var(--ink-3);
    line-height: 1.45;
    display: -webkit-box;
    -webkit-line-clamp: 3;
    line-clamp: 3;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .ctags {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .cm {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    font-size: 10.5px;
    color: var(--ink-4);
  }
  .cm > span {
    display: inline-flex;
    align-items: center;
    gap: 3px;
  }
  .where {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 100%;
  }
  .key {
    font-weight: var(--fw-semibold);
  }
  .due.overdue {
    color: var(--danger);
  }
</style>
