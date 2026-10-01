<script lang="ts">
  // Left-panel search: query box with syntax, filter UI kept in two-way sync
  // with the text, case toggle, grouped results with highlighted snippets,
  // keyboard navigation and saved searches (openable as virtual boards).
  import { onMount, tick, untrack } from 'svelte';
  import { SvelteSet } from 'svelte/reactivity';
  import {
    Search,
    X,
    CaseSensitive,
    SlidersHorizontal,
    ChevronRight,
    ChevronDown,
    LayoutGrid,
    FileText,
    Diamond,
    Bookmark,
    BookmarkPlus,
    MoreHorizontal,
    Loader,
    Archive,
    Layers,
    Type,
  } from '@lucide/svelte';
  import type { SearchHit } from '$lib/backend/types';
  import { ui } from '$lib/state/ui.svelte';
  import { boards } from '$lib/state/boards.svelte';
  import { registry, boardEntry } from '$lib/state/registry.svelte';
  import { settings } from '$lib/settings/store.svelte';
  import { openMenu, openMenuAt } from '$lib/state/menu.svelte';
  import { tip } from '$lib/components/tooltip';
  import Segmented from '$lib/components/Segmented.svelte';
  import { t, relTime } from '$lib/i18n/index.svelte';
  import { parse, removeFilter, highlightTerms, isEmpty } from './query';
  import { effectiveCase, effectiveText, rewrite } from './sync';
  import { search, loadSaved, runSearch, groupByBoard, groupByLane, navOrder, snippetSegments, highlightSegments, type Segment } from './searchState.svelte';
  import { FILTER_KINDS, filterMenu, activeCount, chipLabel, type FilterKind } from './filterMenus';
  import { saveCurrent, toggleCase, openHit, hitMenu, savedMenu, openSavedBoard } from './searchActions';

  let input: HTMLInputElement | undefined = $state();
  let listEl: HTMLDivElement | undefined = $state();
  let hits = $state<SearchHit[]>([]);
  let busy = $state(false);
  let failed = $state('');
  let active = $state(0);
  const collapsed = new SvelteSet<string>();

  loadSaved();

  const caseDefault = $derived(settings.get<boolean>('search.caseSensitive'));
  const parsed = $derived(parse(search.q));
  const caseOn = $derived(effectiveCase(search.q, caseDefault));
  const empty = $derived(isEmpty(parsed));
  const terms = $derived(highlightTerms(parsed));
  const groups = $derived(search.groupBy === 'lane' ? groupByLane(hits, t('searchPanel.noLane')) : groupByBoard(hits));
  const order = $derived(navOrder(groups, collapsed));
  const activeHit = $derived(order[Math.min(active, order.length - 1)]);

  // Opening the Search section focuses the query box (like VS Code).
  onMount(() => void tick().then(() => input?.focus()));

  // Requests from other features (palette "#tag", commands) and focus.
  $effect(() => {
    const tk = ui.searchFocus;
    if (tk === search.focusSeen) return;
    untrack(() => {
      search.focusSeen = tk;
      if (ui.searchQuery) {
        search.q = ui.searchQuery;
        ui.searchQuery = '';
        search.filtersOpen = search.filtersOpen || parse(search.q).filters.length > 0;
      }
      void tick().then(() => {
        input?.focus();
        input?.select();
      });
    });
  });

  // Run the query (debounced); re-run when boards change so results stay live.
  let seq = 0;
  let timer: ReturnType<typeof setTimeout> | null = null;
  const boardsVersion = $derived([...boards.values()].reduce((n, b) => n + b.version, 0));

  $effect(() => {
    const text = search.q;
    const dflt = caseDefault;
    void boardsVersion;
    void registry.indexed.done;
    if (timer) clearTimeout(timer);
    if (isEmpty(parse(text))) {
      seq++;
      hits = [];
      busy = false;
      failed = '';
      return;
    }
    timer = setTimeout(() => void run(effectiveText(text, dflt)), 160);
  });

  async function run(text: string) {
    const my = ++seq;
    busy = true;
    try {
      const r = await runSearch(text, { limit: search.limit, includeArchived: settings.get<boolean>('search.includeArchived') });
      if (my !== seq) return;
      hits = r;
      failed = '';
      if (active >= r.length) active = 0;
    } catch (e) {
      if (my === seq) failed = (e as Error).message;
    } finally {
      if (my === seq) busy = false;
    }
  }

  function setText(v: string) {
    search.q = v;
    active = 0;
  }

  async function showFilter(e: MouseEvent, kind: FilterKind) {
    const el = e.currentTarget as HTMLElement;
    const items = await filterMenu(kind, search.q, setText);
    openMenuAt(el, items);
  }

  function toggleTitleOnly() {
    setText(rewrite(search.q, { ...parsed, titleOnly: !parsed.titleOnly }));
  }

  function toggleNegate(i: number) {
    const q = { ...parsed, filters: parsed.filters.map((f, j) => (j === i ? { ...f, negate: !f.negate } : f)) };
    setText(rewrite(search.q, q));
  }

  function dropChip(i: number) {
    setText(rewrite(search.q, removeFilter(parsed, i)));
  }

  // --- keyboard --------------------------------------------------------------

  function scrollActive() {
    void tick().then(() => listEl?.querySelector('.hit.active')?.scrollIntoView({ block: 'nearest' }));
  }

  function move(d: number) {
    if (!order.length) return;
    active = Math.max(0, Math.min(order.length - 1, active + d));
    scrollActive();
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      move(1);
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      move(-1);
    } else if (e.key === 'PageDown' || e.key === 'PageUp') {
      e.preventDefault();
      move(e.key === 'PageDown' ? 10 : -10);
    } else if (e.key === 'Enter') {
      e.preventDefault();
      if ((e.metaKey || e.ctrlKey) && e.shiftKey) void saveCurrent();
      else if (activeHit) void openHit(activeHit, e.metaKey || e.ctrlKey);
    } else if (e.key === 'Escape') {
      if (search.q) {
        e.preventDefault();
        e.stopPropagation();
        setText('');
      } else input?.blur();
    }
  }

  function toggleGroup(key: string) {
    if (collapsed.has(key)) collapsed.delete(key);
    else collapsed.add(key);
  }

  async function onHitMenu(e: MouseEvent, h: SearchHit) {
    e.preventDefault();
    const at = { x: e.clientX, y: e.clientY };
    openMenu(at, await hitMenu(h));
  }

  function segs(h: SearchHit): { title: Segment[]; snippet: Segment[] } {
    const snip = h.snippet.includes('\u0002') ? snippetSegments(h.snippet) : highlightSegments(h.snippet, terms, caseOn);
    return { title: highlightSegments(h.title || t('common.untitled'), terms, caseOn), snippet: snip };
  }

  const EXAMPLES: [string, string][] = [['tag', 'tag:design'], ['board', 'board:"Product Roadmap"'], ['lane', 'lane:Doing'], ['state', 'is:open'], ['due', 'due:overdue'], ['priority', 'priority:high'], ['person', '@ana'], ['has', 'has:image'], ['updated', 'updated:>=2026-09-01'], ['phrase', '"exact phrase"'], ['exclude', '-tag:wip'], ['title', 'in:title'], ['case', 'case:yes']];

  function addExample(ex: string) {
    setText(search.q.trim() ? `${search.q.trim()} ${ex}` : ex);
    input?.focus();
  }

  const boardIcon = (id: string) => (boardEntry(id)?.mirror ? Diamond : boardEntry(id)?.kind === 'files' ? FileText : LayoutGrid);
</script>

<div class="search">
  <div class="qbox">
    <Search size={14} strokeWidth={1.9} class="qicon" />
    <input
      bind:this={input}
      class="q"
      type="text"
      spellcheck="false"
      autocomplete="off"
      placeholder={t('searchPanel.placeholder')}
      aria-label={t('panels.search')}
      aria-controls="search-results"
      aria-activedescendant={activeHit ? `sr-${activeHit.board}-${activeHit.id}` : undefined}
      value={search.q}
      oninput={(e) => setText((e.currentTarget as HTMLInputElement).value)}
      onkeydown={onKeydown}
    />
    {#if busy}<Loader size={13} class="spin busy" />{/if}
    {#if search.q}
      <button class="icon-btn sm" aria-label={t('commands.search.clear')} use:tip={{ text: t('commands.search.clear'), command: 'search.clear' }} onclick={() => (setText(''), input?.focus())}>
        <X size={13} />
      </button>
    {/if}
    <button class="icon-btn sm" class:active={caseOn} aria-pressed={caseOn} aria-label={t('commands.search.toggleCase')} use:tip={{ text: t('commands.search.toggleCase'), command: 'search.toggleCase' }} onclick={toggleCase}>
      <CaseSensitive size={16} strokeWidth={1.8} />
    </button>
    <button
      class="icon-btn sm"
      class:active={search.filtersOpen}
      aria-pressed={search.filtersOpen}
      aria-label={t('commands.search.toggleFilters')}
      use:tip={{ text: t('commands.search.toggleFilters'), command: 'search.toggleFilters' }}
      onclick={() => (search.filtersOpen = !search.filtersOpen)}
    >
      <SlidersHorizontal size={14} strokeWidth={1.8} />
      {#if parsed.filters.length && !search.filtersOpen}<span class="badge">{parsed.filters.length}</span>{/if}
    </button>
  </div>

  {#if search.filtersOpen}
    <div class="filters">
      {#each FILTER_KINDS as k (k)}
        {@const n = activeCount(parsed, k)}
        <button class="fbtn" class:on={n > 0} onclick={(e) => void showFilter(e, k)}>
          {t(`searchPanel.filters.${k}`)}{#if n}<span class="n">{n}</span>{/if}
          <ChevronDown size={11} strokeWidth={2} />
        </button>
      {/each}
      <button class="fbtn" class:on={parsed.titleOnly} aria-pressed={parsed.titleOnly} onclick={toggleTitleOnly}>
        <Type size={11} strokeWidth={2} />{t('searchPanel.titleOnly')}
      </button>
    </div>
  {/if}

  {#if parsed.filters.length}
    <div class="chips">
      {#each parsed.filters as f, i (i + f.key + f.value + f.cmp)}
        <span class="fchip" class:neg={f.negate}>
          <button class="fchip-l" use:tip={t('searchPanel.chipNegate')} onclick={() => toggleNegate(i)}>{#if f.negate}<span class="not">{t('searchPanel.not')}</span>{/if}{chipLabel(f)}</button>
          <button class="fchip-x" aria-label={t('common.remove')} onclick={() => dropChip(i)}><X size={11} strokeWidth={2.2} /></button>
        </span>
      {/each}
    </div>
  {/if}

  {#if empty}
    <div class="scroll">
      <div class="block">
        <div class="section-title">{t('searchPanel.savedTitle')}</div>
        {#if search.saved.length}
          <ul class="saved">
            {#each search.saved as s (s.id)}
              <li>
                <button class="row srow" onclick={() => (setText(s.q), input?.focus())} oncontextmenu={(e) => openMenu(e, savedMenu(s))}>
                  <Bookmark size={13} strokeWidth={1.8} />
                  <span class="grow">
                    <span class="sname">{s.name}</span>
                    <span class="sq">{s.q}</span>
                  </span>
                </button>
                <button class="icon-btn sm" aria-label={t('searchPanel.openAsBoard')} use:tip={t('searchPanel.openAsBoard')} onclick={() => openSavedBoard(s)}><LayoutGrid size={13} /></button>
                <button class="icon-btn sm" aria-label={t('common.more')} onclick={(e) => openMenuAt(e.currentTarget as HTMLElement, savedMenu(s))}><MoreHorizontal size={13} /></button>
              </li>
            {/each}
          </ul>
        {:else}
          <p class="hint">{t('searchPanel.savedEmpty')}</p>
        {/if}
      </div>
      <div class="block">
        <div class="section-title">{t('searchPanel.syntaxTitle')}</div>
        <p class="hint">{t('searchPanel.syntaxHint')}</p>
        <div class="examples">
          {#each EXAMPLES as [id, ex] (id)}
            <button class="ex" onclick={() => addExample(ex)}><code>{ex}</code><span>{t(`searchPanel.ex.${id}`)}</span></button>
          {/each}
        </div>
      </div>
    </div>
  {:else}
    <div class="rhead">
      <span class="rcount">
        {#if failed}{t('searchPanel.failed')}{:else}{t('searchPanel.results', { count: hits.length })}{/if}
      </span>
      <Segmented
        size="sm"
        options={[
          { value: 'board', label: t('searchPanel.byBoard') },
          { value: 'lane', label: t('searchPanel.byLane') },
        ]}
        bind:value={search.groupBy}
      />
      <button class="icon-btn sm" aria-label={t('searchPanel.openAsBoard')} use:tip={t('searchPanel.openAsBoard')} onclick={() => openSavedBoard({ q: search.q, groupBy: search.groupBy })}><LayoutGrid size={13} /></button>
      <button class="icon-btn sm" aria-label={t('commands.search.saveCurrent')} use:tip={{ text: t('commands.search.saveCurrent'), command: 'search.saveCurrent' }} onclick={() => void saveCurrent()}><BookmarkPlus size={14} /></button>
    </div>
    <div class="scroll results" id="search-results" role="listbox" aria-label={t('panels.search')} tabindex="0" bind:this={listEl} onkeydown={onKeydown}>
      {#if failed}
        <div class="empty small">{failed}</div>
      {:else if !hits.length && !busy}
        <div class="empty small">
          <div class="empty-icon"><Search size={18} strokeWidth={1.8} /></div>
          <span>{t('searchPanel.noResults')}</span>
        </div>
      {/if}
      {#each groups as g (g.key)}
        {@const GIcon = search.groupBy === 'board' ? boardIcon(g.key) : Layers}
        <div class="group">
          <button class="ghead" onclick={() => toggleGroup(g.key)} aria-expanded={!collapsed.has(g.key)}>
            <span class="chev" class:open={!collapsed.has(g.key)}><ChevronRight size={12} strokeWidth={2} /></span>
            <GIcon size={13} strokeWidth={1.8} />
            <span class="grow gl">{g.label}</span>
            <span class="count">{g.hits.length}</span>
          </button>
          {#if !collapsed.has(g.key)}
            {#each g.hits as h (h.board + h.id)}
              {@const s = segs(h)}
              {@const idx = order.indexOf(h)}
              <button
                id="sr-{h.board}-{h.id}"
                class="hit"
                class:active={idx === active}
                role="option"
                aria-selected={idx === active}
                tabindex="-1"
                onclick={(e) => ((active = idx), void openHit(h, e.metaKey || e.ctrlKey))}
                onauxclick={(e) => e.button === 1 && void openHit(h, true)}
                oncontextmenu={(e) => void onHitMenu(e, h)}
              >
                <span class="ht">
                  {#if h.archived}<Archive size={11} strokeWidth={2} class="arch" />{/if}
                  {#each s.title as seg, si (si)}{#if seg.hit}<mark>{seg.text}</mark>{:else}{seg.text}{/if}{/each}
                </span>
                {#if s.snippet.length && h.snippet.trim() && h.snippet.trim() !== h.title}
                  <span class="hs">{#each s.snippet as seg, si (si)}{#if seg.hit}<mark>{seg.text}</mark>{:else}{seg.text}{/if}{/each}</span>
                {/if}
                <span class="hm">
                  {#if search.groupBy === 'lane'}<span>{h.boardName}</span>{:else if h.laneName}<span>{h.laneName}</span>{/if}
                  {#if h.remoteKey}<span class="key">{h.remoteKey}</span>{/if}
                  {#each h.tags.slice(0, 3) as tag (tag)}<span class="tg">#{tag}</span>{/each}
                  <span class="when">{relTime(h.mtime)}</span>
                </span>
              </button>
            {/each}
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .search {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .qbox {
    display: flex;
    align-items: center;
    gap: 2px;
    margin: 0 10px 6px;
    padding: 0 4px 0 10px;
    height: 34px;
    border-radius: var(--r-sm);
    border: 1px solid var(--line-strong);
    background: var(--bg-elev);
    transition:
      border-color var(--dur-fast) var(--ease),
      box-shadow var(--dur-fast) var(--ease);
  }
  .qbox:focus-within {
    border-color: var(--primary);
    box-shadow: 0 0 0 3px var(--focus);
  }
  .qbox :global(.qicon) {
    color: var(--ink-4);
    flex-shrink: 0;
  }
  .q {
    flex: 1;
    min-width: 0;
    height: 100%;
    padding: 0 6px;
    border: none;
    outline: none;
    background: transparent;
    color: var(--ink);
    font-size: var(--fs-md);
  }
  .q::placeholder {
    color: var(--ink-4);
  }
  .qbox :global(.busy) {
    color: var(--ink-4);
    margin-right: 2px;
  }
  .icon-btn {
    position: relative;
  }
  .badge {
    position: absolute;
    top: -3px;
    right: -3px;
    min-width: 14px;
    height: 14px;
    padding: 0 3px;
    border-radius: var(--r-pill);
    background: var(--primary);
    color: var(--on-primary);
    font-size: 9.5px;
    line-height: 14px;
    font-weight: var(--fw-semibold);
  }
  .filters {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    padding: 0 10px 6px;
  }
  .fbtn {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    height: 22px;
    padding: 0 7px 0 8px;
    border: 1px solid var(--line-strong);
    border-radius: var(--r-pill);
    background: transparent;
    color: var(--ink-3);
    font-size: var(--fs-xs);
    font-weight: var(--fw-medium);
    transition:
      background var(--dur-fast) var(--ease),
      color var(--dur-fast) var(--ease);
  }
  .fbtn:hover {
    background: var(--bg-hover);
    color: var(--ink);
  }
  .fbtn.on {
    border-color: transparent;
    background: var(--primary-soft);
    color: var(--primary-strong);
  }
  .fbtn .n {
    font-variant-numeric: tabular-nums;
    opacity: 0.8;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    padding: 0 10px 8px;
  }
  .fchip {
    display: inline-flex;
    align-items: center;
    height: 22px;
    border-radius: var(--r-pill);
    background: var(--secondary);
    color: var(--secondary-ink);
    font-size: var(--fs-xs);
    font-weight: var(--fw-medium);
    overflow: hidden;
    max-width: 100%;
  }
  .fchip.neg {
    background: var(--danger-soft);
    color: var(--danger);
  }
  .fchip button {
    border: none;
    background: transparent;
    color: inherit;
    height: 100%;
    font: inherit;
  }
  .fchip-l {
    padding: 0 4px 0 9px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .fchip-x {
    display: grid;
    place-items: center;
    width: 20px;
    opacity: 0.7;
  }
  .fchip-x:hover {
    opacity: 1;
  }
  .not {
    margin-right: 4px;
    font-weight: var(--fw-semibold);
    text-transform: uppercase;
    font-size: 9.5px;
    letter-spacing: 0.05em;
  }
  .rhead {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 2px 10px 6px 14px;
  }
  .rcount {
    flex: 1;
    font-size: var(--fs-xs);
    color: var(--ink-3);
    font-weight: var(--fw-medium);
  }
  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 0 6px 16px;
    outline: none;
  }
  .block {
    padding: 8px 8px 12px;
  }
  .block .section-title {
    margin-bottom: 6px;
  }
  .hint {
    margin: 0;
    font-size: var(--fs-sm);
    color: var(--ink-3);
    line-height: var(--lh);
  }
  .examples {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 2px 10px;
    margin-top: 8px;
  }
  .ex {
    display: contents;
    border: none;
    background: none;
  }
  .ex code,
  .ex span {
    padding: 3px 0;
    cursor: pointer;
    text-align: left;
  }
  .ex code {
    justify-self: start;
    align-self: start;
    padding: 2px 7px;
    border-radius: var(--r-xs);
    background: var(--bg-hover);
    color: var(--ink-2);
    font-family: var(--font-mono);
    font-size: 11px;
    font-variant-ligatures: none;
    white-space: nowrap;
  }
  .ex span {
    color: var(--ink-3);
    font-size: var(--fs-xs);
    line-height: 1.35;
    align-self: start;
  }
  .ex:hover code {
    background: var(--primary-soft);
    color: var(--primary-strong);
  }
  .saved {
    list-style: none;
    margin: 0 -6px;
    padding: 0;
  }
  .saved li {
    display: flex;
    align-items: center;
    gap: 2px;
    padding-right: 4px;
    border-radius: var(--r-xs);
  }
  .saved li:hover {
    background: var(--bg-hover);
  }
  .srow {
    flex: 1;
    min-width: 0;
    border: none;
    background: transparent;
    text-align: left;
    padding: 5px 6px;
    min-height: 36px;
    color: var(--ink-3);
  }
  .srow:hover {
    background: transparent;
  }
  .srow .grow {
    display: flex;
    flex-direction: column;
  }
  .sname {
    color: var(--ink);
    font-weight: var(--fw-medium);
    font-size: var(--fs-md);
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .sq {
    font-family: var(--font-mono);
    font-size: 10.5px;
    color: var(--ink-4);
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .group + .group {
    margin-top: 4px;
  }
  .ghead {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    height: 26px;
    padding: 0 8px 0 4px;
    border: none;
    border-radius: var(--r-xs);
    background: transparent;
    color: var(--ink-3);
    font-size: var(--fs-sm);
    font-weight: var(--fw-semibold);
    text-align: left;
    position: sticky;
    top: 0;
    z-index: 1;
    background: var(--bg-sidebar);
    backdrop-filter: blur(12px);
  }
  .ghead:hover {
    color: var(--ink);
  }
  .gl {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .grow {
    flex: 1;
    min-width: 0;
  }
  .chev {
    display: grid;
    place-items: center;
    transition: transform var(--dur) var(--ease);
  }
  .chev.open {
    transform: rotate(90deg);
  }
  .count {
    font-size: var(--fs-xs);
    color: var(--ink-4);
    font-weight: var(--fw-medium);
  }
  .hit {
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 100%;
    padding: 6px 10px 7px 26px;
    border: none;
    border-radius: var(--r-sm);
    background: transparent;
    text-align: left;
    color: var(--ink-2);
    transition: background var(--dur-fast) var(--ease);
  }
  .hit:hover {
    background: var(--bg-hover);
  }
  .hit.active {
    background: var(--primary-soft);
  }
  .ht {
    display: flex;
    align-items: center;
    gap: 4px;
    color: var(--ink);
    font-weight: var(--fw-medium);
    font-size: var(--fs-md);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ht :global(.arch) {
    color: var(--ink-4);
    flex-shrink: 0;
  }
  .hs {
    font-size: var(--fs-sm);
    color: var(--ink-3);
    line-height: 1.45;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .hm {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
    font-size: 10.5px;
    color: var(--ink-4);
  }
  .hm .tg {
    color: var(--primary);
  }
  .hm .key {
    font-weight: var(--fw-semibold);
  }
  .hm .when {
    margin-left: auto;
  }
  .empty.small {
    padding: var(--sp-6) var(--sp-4);
    font-size: var(--fs-sm);
  }
</style>
