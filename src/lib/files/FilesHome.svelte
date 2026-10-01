<script lang="ts">
  // Home of a files board (SPEC §7.4): recent documents, a filter, list/grid
  // views and a "New document" button. Documents open in document tabs.
  import { FilePlus2, FileText, Search, LayoutGrid, List, Archive, ChevronRight, Folder } from '@lucide/svelte';
  import type { BoardModel } from '$lib/state/boards.svelte';
  import { openDocTab } from '$lib/state/workspace.svelte';
  import { openMenu } from '$lib/state/menu.svelte';
  import { uiGet, uiSet } from '$lib/state/persist.svelte';
  import Segmented from '$lib/components/Segmented.svelte';
  import { t, relTime } from '$lib/i18n/index.svelte';
  import { collectDocs, visibleDocs, type DocSort } from './filesHome';
  import { newDocument, docMenu } from './filesActions';
  import { files } from './filesState.svelte';

  let { board }: { board: BoardModel } = $props();

  type Layout = 'list' | 'grid';
  const prefs = uiGet<{ layout?: Layout; sort?: DocSort }>('filesHome', {});
  let layout = $state<Layout>(prefs.layout === 'grid' ? 'grid' : 'list');
  let sort = $state<DocSort>(prefs.sort === 'title' ? 'title' : 'recent');
  let filter = $state('');
  let showArchived = $state(false);
  let active = $state(0);
  let input = $state<HTMLInputElement>();
  let listEl = $state<HTMLElement>();

  $effect(() => uiSet('filesHome', { layout, sort }));

  const readOnly = $derived(!!board.header.readOnly);
  const all = $derived.by(() => {
    void board.version;
    return collectDocs(board, t('common.untitled'));
  });
  const docs = $derived(visibleDocs(all, { filter, sort, showArchived }));
  const archivedCount = $derived(all.filter((d) => d.archived).length);

  $effect(() => {
    if (active >= docs.length) active = Math.max(0, docs.length - 1);
  });

  // `files.focusFilter` command → focus the filter box of the visible home.
  let seen = files.focusTick;
  $effect(() => {
    const tk = files.focusTick;
    if (tk !== seen) {
      seen = tk;
      input?.focus();
    }
  });

  function open(i: number) {
    const d = docs[i];
    if (d) void openDocTab(board.id, d.id);
  }

  function cols(): number {
    if (layout === 'list' || !listEl) return 1;
    const first = listEl.firstElementChild as HTMLElement | null;
    if (!first) return 1;
    return Math.max(1, Math.round(listEl.clientWidth / (first.offsetWidth + 12)));
  }

  function focusItem(i: number) {
    active = Math.max(0, Math.min(docs.length - 1, i));
    (listEl?.children[active] as HTMLElement | undefined)?.focus();
  }

  function onListKey(e: KeyboardEvent) {
    const c = cols();
    const map: Record<string, number> = { ArrowDown: c, ArrowUp: -c, ArrowRight: layout === 'grid' ? 1 : 0, ArrowLeft: layout === 'grid' ? -1 : 0 };
    if (e.key in map && map[e.key]) {
      e.preventDefault();
      focusItem(active + map[e.key]);
    } else if (e.key === 'Home') focusItem(0);
    else if (e.key === 'End') focusItem(docs.length - 1);
    else if (e.key === 'Enter') {
      e.preventDefault();
      open(active);
    } else if (e.key === 'ContextMenu' || (e.shiftKey && e.key === 'F10')) {
      e.preventDefault();
      const el = listEl?.children[active] as HTMLElement | undefined;
      const r = el?.getBoundingClientRect();
      if (r && docs[active]) openMenu({ x: r.left + 24, y: r.bottom }, docMenu(board, docs[active].id));
    }
  }

  function onFilterKey(e: KeyboardEvent) {
    if (e.key === 'ArrowDown' && docs.length) {
      e.preventDefault();
      focusItem(0);
    } else if (e.key === 'Enter' && docs.length) {
      e.preventDefault();
      open(0);
    } else if (e.key === 'Escape' && filter) {
      e.stopPropagation();
      filter = '';
    }
  }
</script>

<div class="home">
  <div class="inner">
    <header>
      <div class="title">
        <span class="ic"><FileText size={18} strokeWidth={1.8} /></span>
        <div>
          <h1>{board.header.name}</h1>
          <p class="muted">{t('filesHome.count', { count: all.length - archivedCount })}</p>
        </div>
      </div>
      <button class="btn primary" onclick={() => void newDocument(board.id)} disabled={readOnly}>
        <FilePlus2 size={15} strokeWidth={1.8} />
        {t('filesHome.new')}
      </button>
    </header>

    <div class="toolbar">
      <label class="field filter">
        <Search size={14} strokeWidth={1.8} />
        <input bind:this={input} bind:value={filter} placeholder={t('filesHome.filter')} aria-label={t('filesHome.filter')} onkeydown={onFilterKey} spellcheck="false" />
      </label>
      <Segmented
        size="sm"
        bind:value={sort}
        options={[
          { value: 'recent', label: t('filesHome.sortRecent') },
          { value: 'title', label: t('filesHome.sortTitle') },
        ]}
      />
      <Segmented
        size="sm"
        bind:value={layout}
        options={[
          { value: 'list', icon: List, title: t('filesHome.list') },
          { value: 'grid', icon: LayoutGrid, title: t('filesHome.grid') },
        ]}
      />
      {#if archivedCount}
        <button class="btn ghost sm" class:on={showArchived} aria-pressed={showArchived} onclick={() => (showArchived = !showArchived)}>
          <Archive size={14} strokeWidth={1.8} />
          {t('filesHome.showArchived', { count: archivedCount })}
        </button>
      {/if}
    </div>

    {#if !all.length}
      <div class="empty">
        <strong>{t('filesHome.emptyTitle')}</strong>
        <span>{t('filesHome.emptyHint')}</span>
        {#if !readOnly}
          <button class="btn soft" onclick={() => void newDocument(board.id)}><FilePlus2 size={15} strokeWidth={1.8} />{t('filesHome.new')}</button>
        {/if}
      </div>
    {:else if !docs.length}
      <div class="empty"><span>{t('filesHome.noMatch')}</span></div>
    {:else}
      <ul class="docs {layout}" bind:this={listEl} role="listbox" tabindex="-1" aria-label={t('filesHome.documents')} onkeydown={onListKey}>
        {#each docs as d, i (d.id)}
          <li
            role="option"
            aria-selected={i === active}
            tabindex={i === active ? 0 : -1}
            class="doc card-surface"
            class:archived={d.archived}
            onclick={() => ((active = i), open(i))}
            onfocus={() => (active = i)}
            oncontextmenu={(e) => {
              e.preventDefault();
              active = i;
              openMenu(e, docMenu(board, d.id));
            }}
            onkeydown={() => {}}
          >
            <span class="dic">{#if d.subdocs}<Folder size={15} strokeWidth={1.8} />{:else}<FileText size={15} strokeWidth={1.8} />{/if}</span>
            <span class="main">
              <span class="name">{d.title}</span>
              {#if d.path.length}
                <span class="path muted">
                  {#each d.path as p, j (j)}{#if j}<ChevronRight size={11} />{/if}{p}{/each}
                </span>
              {/if}
            </span>
            <span class="meta muted">
              {#if d.subdocs}<span>{t('filesHome.subdocs', { count: d.subdocs })}</span>{/if}
              <span>{relTime(d.mtime)}</span>
            </span>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</div>

<style>
  .home {
    flex: 1;
    overflow-y: auto;
  }
  .inner {
    max-width: 960px;
    margin: 0 auto;
    padding: 48px 32px;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--sp-4);
  }
  .title {
    display: flex;
    align-items: center;
    gap: var(--sp-3);
  }
  .ic {
    width: 38px;
    height: 38px;
    display: grid;
    place-items: center;
    border-radius: var(--r-md);
    background: var(--primary-soft);
    color: var(--primary-strong);
  }
  h1 {
    margin: 0;
    font-size: var(--fs-2xl);
    font-weight: 650;
    letter-spacing: -0.02em;
  }
  header p {
    margin: 2px 0 0;
  }
  .toolbar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--sp-2);
    margin: var(--sp-6) 0 var(--sp-4);
  }
  .filter {
    flex: 1;
    min-width: 200px;
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    color: var(--ink-3);
  }
  .filter input {
    flex: 1;
    border: none;
    background: transparent;
    outline: none;
    color: var(--ink);
    font: inherit;
  }
  .btn.on {
    background: var(--primary-soft);
    color: var(--primary-strong);
  }
  .docs {
    list-style: none;
    margin: 0;
    padding: 0;
    outline: none;
  }
  .docs.list {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .docs.grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
    gap: 12px;
  }
  .doc {
    display: flex;
    align-items: center;
    gap: var(--sp-3);
    padding: 10px 14px;
    border-radius: var(--r-md);
    cursor: pointer;
    outline: none;
    transition:
      transform var(--dur) var(--ease-spring),
      box-shadow var(--dur-fast) var(--ease),
      background var(--dur-fast) var(--ease);
  }
  .grid .doc {
    flex-direction: column;
    align-items: flex-start;
    min-height: 108px;
    padding: 14px;
  }
  .doc:hover {
    background: var(--bg-hover);
  }
  .grid .doc:hover {
    transform: translateY(-2px);
  }
  .doc:focus-visible,
  .doc[aria-selected='true']:focus {
    box-shadow: 0 0 0 2px var(--primary-ring);
  }
  .doc.archived {
    opacity: 0.6;
  }
  .dic {
    color: var(--primary-strong);
    display: grid;
    place-items: center;
  }
  .main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .name {
    font-weight: var(--fw-medium);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .grid .name {
    white-space: normal;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
  }
  .path {
    display: flex;
    align-items: center;
    gap: 2px;
    font-size: var(--fs-xs);
    overflow: hidden;
    white-space: nowrap;
  }
  .meta {
    display: flex;
    gap: var(--sp-3);
    font-size: var(--fs-xs);
    white-space: nowrap;
  }
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--sp-2);
    padding: 64px 0;
  }
</style>
