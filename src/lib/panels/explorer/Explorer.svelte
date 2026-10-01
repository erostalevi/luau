<script lang="ts">
  // Left-panel explorer: a virtualized tree of Boards → Lanes → Cards → Subcards
  // (files boards: Documents → Subdocuments) plus a Mirrors section. Human
  // names only. Rows are drop targets for the shared pointer DnD engine
  // (`data-tree`), and card rows are drag sources.
  import { tick, untrack } from 'svelte';
  import { SvelteSet } from 'svelte/reactivity';
  import {
    ChevronRight,
    LayoutGrid,
    FileText,
    Diamond,
    Layers,
    Pin,
    EyeOff,
    Eye,
    TriangleAlert,
    Loader,
    Plus,
    LocateFixed,
    ChevronsDownUp,
    RotateCw,
    FolderSearch,
    Fingerprint,
  } from '@lucide/svelte';
  import { boards, openBoard } from '$lib/state/boards.svelte';
  import { registry, sortedBoards } from '$lib/state/registry.svelte';
  import { uiState } from '$lib/state/persist.svelte';
  import { settings } from '$lib/settings/store.svelte';
  import { ctx } from '$lib/commands/context.svelte';
  import { runCommand } from '$lib/commands/registry.svelte';
  import { openMenu } from '$lib/state/menu.svelte';
  import { activeTab } from '$lib/state/workspace.svelte';
  import { ui } from '$lib/state/ui.svelte';
  import { dnd, startDrag } from '$lib/board/dnd.svelte';
  import { tip } from '$lib/components/tooltip';
  import { t } from '$lib/i18n/index.svelte';
  import { flatten, boardKey, cardKey, type Row } from './flatten';
  import { explorer, expanded, loadExpanded, setExpanded, toggleExpanded, collapseAll, revealActive } from './explorerState.svelte';
  import { step, edge, leftTarget, rightTarget, typeahead, indexOfKey, focusable } from './nav';
  import {
    tree,
    rowMenu,
    openRow,
    startRename,
    commitRename,
    deleteRow,
    relocate,
    boardDragSource,
    registerExplorerDrop,
    toggleShowHidden,
  } from './explorerActions';

  const H = 28;
  const OVERSCAN = 10;

  const errors = new SvelteSet<string>();
  const loading = new Set<string>();

  const showHidden = $derived(settings.get<boolean>('explorer.showHidden'));
  const boardsList = $derived(sortedBoards('boards', showHidden));
  const mirrorsList = $derived(sortedBoards('mirrors', showHidden));

  const rows = $derived(
    flatten({
      sections: [
        { id: 'boards', label: t('explorer.sections.boards'), boards: boardsList, show: true },
        { id: 'mirrors', label: t('explorer.sections.mirrors'), boards: mirrorsList, show: mirrorsList.length > 0 },
      ],
      board: (id) => boards.get(id),
      expanded,
      showArchived: ui.showArchived,
      loading,
      errors,
      untitled: t('common.untitled'),
    }),
  );

  $effect(() => {
    tree.rows = rows;
  });

  $effect(() => {
    if (uiState.loaded) untrack(loadExpanded);
  });

  registerExplorerDrop();

  // Lazy loading: an expanded board without a model shows a "loading" row.
  $effect(() => {
    for (const r of rows) if (r.type === 'loading') untrack(() => load(r.boardId));
  });

  function load(id: string) {
    if (loading.has(id) || errors.has(id) || boards.get(id)) return;
    loading.add(id);
    openBoard({ id })
      .catch(() => errors.add(id))
      .finally(() => loading.delete(id));
  }

  function retry(id: string) {
    errors.delete(id);
    load(id);
  }

  // --- virtualization ------------------------------------------------------

  let scroller: HTMLDivElement | undefined = $state();
  let treeEl: HTMLDivElement | undefined = $state();
  let scrollTop = $state(0);
  let viewH = $state(400);
  const start = $derived(Math.max(0, Math.floor(scrollTop / H) - OVERSCAN));
  const end = $derived(Math.min(rows.length, Math.ceil((scrollTop + viewH) / H) + OVERSCAN));
  const slice = $derived(rows.slice(start, end));

  function ensureVisible(key: string, center = false) {
    const i = indexOfKey(rows, key);
    if (i < 0 || !scroller) return;
    const top = i * H;
    if (center) scroller.scrollTop = Math.max(0, top - viewH / 2 + H / 2);
    else if (top < scroller.scrollTop) scroller.scrollTop = top;
    else if (top + H > scroller.scrollTop + viewH) scroller.scrollTop = top + H - viewH;
  }

  // Reveal requests (commands, search results).
  let lastReveal = explorer.revealTick;
  $effect(() => {
    const tk = explorer.revealTick;
    if (tk === lastReveal) return;
    lastReveal = tk;
    void tick().then(() => {
      ensureVisible(explorer.focusKey, true);
    });
  });

  let lastFocus = explorer.focusTick;
  $effect(() => {
    const tk = explorer.focusTick;
    if (tk === lastFocus) return;
    lastFocus = tk;
    void tick().then(() => {
      if (!rows.some((r) => r.key === explorer.focusKey)) explorer.focusKey = activeKey || edge(rows, false);
      treeEl?.focus({ preventScroll: true });
      ensureVisible(explorer.focusKey);
    });
  });

  // --- active row ------------------------------------------------------------

  const activeKey = $derived.by(() => {
    const tab = activeTab();
    if (!tab?.boardId) return '';
    if (tab.kind === 'doc' && tab.cardId) return cardKey(tab.boardId, tab.cardId);
    if (ui.editor.open && ui.editor.boardId === tab.boardId && ui.editor.cardId) return cardKey(tab.boardId, ui.editor.cardId);
    return tab.kind === 'board' ? boardKey(tab.boardId) : '';
  });

  // --- DnD: expand collapsed rows when hovering during a drag ------------------

  let hoverKey = '';
  let hoverTimer: ReturnType<typeof setTimeout> | null = null;
  $effect(() => {
    const tg = dnd.target;
    const key = dnd.active && tg?.type === 'tree' ? (tg.el.dataset.key ?? '') : '';
    if (key === hoverKey) return;
    hoverKey = key;
    if (hoverTimer) clearTimeout(hoverTimer);
    hoverTimer = null;
    if (!key) return;
    hoverTimer = setTimeout(() => {
      const r = rows.find((x) => x.key === key);
      if (r && r.expandable && !r.expanded) setExpanded(key, true);
    }, 650);
  });

  const dragging = $derived(dnd.active && dnd.source?.kind === 'cards' ? new Set(dnd.source.ids ?? []) : null);

  // --- interaction -----------------------------------------------------------

  function focusRow(key: string) {
    explorer.focusKey = key;
    ensureVisible(key);
  }

  function onRowPointerDown(e: PointerEvent, r: Row) {
    if (e.button !== 0 || explorer.renaming === r.key || (e.target as HTMLElement).closest('button, input')) return;
    if (r.type === 'card') {
      const m = boards.get(r.boardId);
      const copyOnly = !!m?.header.readOnly && m.header.readOnly.startsWith('mirror');
      startDrag(
        e,
        {
          kind: 'cards',
          boardId: r.boardId,
          ids: [r.id],
          copyOnly: copyOnly || !!registry.data.boards.find((b) => b.id === r.boardId)?.mirror,
          label: r.label,
        },
        null,
        scroller ? [scroller] : [],
      );
    } else if (r.type === 'board' && r.entry && !r.entry.missing) {
      startDrag(e, boardDragSource(r.entry), null, scroller ? [scroller] : []);
    }
  }

  function onRowClick(e: MouseEvent, r: Row) {
    if (explorer.renaming === r.key) return;
    focusRow(r.key);
    treeEl?.focus({ preventScroll: true });
    if (r.type === 'section') return setOpen(r, !r.expanded);
    if (r.type === 'board' && r.entry?.missing) return openMenu(e, rowMenu(r));
    if (r.type === 'loading' || r.type === 'empty') return;
    if (r.type === 'error') return retry(r.boardId);
    if (r.type === 'hint') return openMenu(e, rowMenu(r));
    if (r.type === 'lane') return r.expandable ? toggleExpanded(r.key) : undefined;
    const side = e.metaKey || e.ctrlKey;
    if (r.type === 'board' && !r.expanded && !side) setExpanded(r.key, true);
    void openRow(r, side);
  }

  function onRowDblClick(r: Row) {
    if (r.type === 'lane') void openRow(r);
    else if (r.type === 'board' && r.expanded) setExpanded(r.key, false);
  }

  function onAuxClick(e: MouseEvent, r: Row) {
    if (e.button === 1 && (r.type === 'board' || (r.type === 'card' && r.kind === 'files'))) {
      e.preventDefault();
      void openRow(r, true);
    }
  }

  function onContextMenu(e: MouseEvent, r: Row) {
    focusRow(r.key);
    const items = rowMenu(r);
    if (items.length) openMenu(e, items);
    else e.preventDefault();
  }

  function chevron(e: MouseEvent, r: Row) {
    e.stopPropagation();
    focusRow(r.key);
    setOpen(r, !r.expanded);
  }

  function setOpen(r: Row, open: boolean) {
    if (r.type === 'section') setExpanded(`${r.key}:collapsed`, !open);
    else setExpanded(r.key, open);
  }

  let typed = '';
  let typedTimer: ReturnType<typeof setTimeout> | null = null;

  function onKeydown(e: KeyboardEvent) {
    if (explorer.renaming) return;
    const cur = rows.find((r) => r.key === explorer.focusKey);
    const key = e.key;
    const handled = () => {
      e.preventDefault();
      e.stopPropagation();
    };
    if (key === 'ArrowDown' || key === 'ArrowUp') {
      handled();
      focusRow(step(rows, explorer.focusKey, key === 'ArrowDown' ? 1 : -1));
    } else if (key === 'Home' || key === 'End') {
      handled();
      focusRow(edge(rows, key === 'End'));
    } else if (key === 'PageDown' || key === 'PageUp') {
      handled();
      focusRow(step(rows, explorer.focusKey, (key === 'PageDown' ? 1 : -1) * Math.max(1, Math.floor(viewH / H) - 1)));
    } else if (key === 'ArrowLeft' && cur) {
      handled();
      const r = leftTarget(rows, cur.key);
      if (r.collapse) setOpen(cur, false);
      else if (r.focus) focusRow(r.focus);
    } else if (key === 'ArrowRight' && cur) {
      handled();
      const r = rightTarget(rows, cur.key);
      if (r.expand) setOpen(cur, true);
      else if (r.focus) focusRow(r.focus);
    } else if (key === 'Enter' && cur) {
      handled();
      if (cur.type === 'lane' || cur.type === 'section') setOpen(cur, !cur.expanded);
      else if (cur.type === 'board' && cur.entry?.missing) void relocate(cur.entry);
      else void openRow(cur, e.metaKey || e.ctrlKey || e.altKey);
    } else if (key === ' ' && cur && cur.expandable) {
      handled();
      setOpen(cur, !cur.expanded);
    } else if (key === 'F2' && cur) {
      handled();
      startRename(cur);
    } else if ((key === 'Delete' || (key === 'Backspace' && (e.metaKey || e.ctrlKey))) && cur) {
      handled();
      void deleteRow(cur);
    } else if ((key === 'ContextMenu' || (key === 'F10' && e.shiftKey)) && cur) {
      handled();
      const el = treeEl?.querySelector<HTMLElement>(`[data-key="${CSS.escape(cur.key)}"]`);
      const rc = el?.getBoundingClientRect();
      const items = rowMenu(cur);
      if (items.length) openMenu({ x: (rc?.left ?? 0) + 24, y: (rc?.bottom ?? 0) + 2 }, items);
    } else if (key === 'Escape') {
      handled();
      treeEl?.blur();
    } else if (key.length === 1 && !e.metaKey && !e.ctrlKey && !e.altKey && key !== ' ') {
      handled();
      typed += key;
      if (typedTimer) clearTimeout(typedTimer);
      typedTimer = setTimeout(() => (typed = ''), 700);
      const hit = typeahead(rows, explorer.focusKey, typed);
      if (hit) focusRow(hit);
    }
  }

  function onFocusIn() {
    ctx.explorerFocus = true;
    if (!rows.some((r) => r.key === explorer.focusKey && focusable(r)))
      explorer.focusKey = activeKey && rows.some((r) => r.key === activeKey) ? activeKey : edge(rows, false);
  }

  function onFocusOut(e: FocusEvent) {
    if (!(e.currentTarget as HTMLElement).contains(e.relatedTarget as Node)) ctx.explorerFocus = false;
  }

  // Inline rename: focus + select once the input mounts.
  function renameInput(node: HTMLInputElement, r: Row) {
    node.value = r.label === t('common.untitled') ? '' : r.label;
    queueMicrotask(() => {
      node.focus();
      node.select();
    });
    let done = false;
    const finish = (commit: boolean) => {
      if (done) return;
      done = true;
      if (commit) void commitRename(r, node.value);
      else explorer.renaming = '';
      treeEl?.focus({ preventScroll: true });
    };
    const key = (e: KeyboardEvent) => {
      e.stopPropagation();
      if (e.key === 'Enter') {
        e.preventDefault();
        finish(true);
      } else if (e.key === 'Escape') {
        e.preventDefault();
        finish(false);
      }
    };
    const blur = () => finish(true);
    node.addEventListener('keydown', key);
    node.addEventListener('blur', blur);
    return {
      destroy() {
        node.removeEventListener('keydown', key);
        node.removeEventListener('blur', blur);
      },
    };
  }

  const activeDescendant = $derived(explorer.focusKey && rows.some((r) => r.key === explorer.focusKey) ? `ex-${explorer.focusKey}` : undefined);
  const noBoards = $derived(registry.data.boards.length === 0);
</script>

<div class="explorer">
  <div class="toolbar">
    <span class="section-title grow">{t('panels.explorer')}</span>
    {#if registry.scanning}
      <span class="scan" use:tip={t('explorer.scanning')}><Loader size={13} class="spin" /></span>
    {/if}
    <button
      class="icon-btn sm"
      aria-label={t('commands.board.new')}
      use:tip={{ text: t('commands.board.new'), command: 'board.new' }}
      onclick={() => void runCommand('board.new')}
    >
      <Plus size={15} strokeWidth={1.8} />
    </button>
    <button
      class="icon-btn sm"
      aria-label={t('commands.explorer.revealActive')}
      use:tip={{ text: t('commands.explorer.revealActive'), command: 'explorer.revealActive' }}
      onclick={() => revealActive()}
    >
      <LocateFixed size={14} strokeWidth={1.8} />
    </button>
    <button
      class="icon-btn sm"
      aria-label={t('commands.explorer.collapseAll')}
      use:tip={{ text: t('commands.explorer.collapseAll'), command: 'explorer.collapseAll' }}
      onclick={collapseAll}
    >
      <ChevronsDownUp size={14} strokeWidth={1.8} />
    </button>
    <button
      class="icon-btn sm"
      class:active={showHidden}
      aria-pressed={showHidden}
      aria-label={t('explorer.showHidden')}
      use:tip={{ text: t('explorer.showHidden'), command: 'explorer.toggleHidden' }}
      onclick={toggleShowHidden}
    >
      {#if showHidden}<Eye size={14} strokeWidth={1.8} />{:else}<EyeOff size={14} strokeWidth={1.8} />{/if}
    </button>
  </div>

  {#if noBoards}
    <div class="empty">
      <div class="empty-icon"><LayoutGrid size={20} strokeWidth={1.8} /></div>
      <strong>{t('explorer.emptyTitle')}</strong>
      <span>{t('explorer.emptyHint')}</span>
      <button class="btn soft sm" onclick={() => void runCommand('board.new')}><Plus size={14} />{t('commands.board.new')}</button>
    </div>
  {:else}
    <div class="scroller" bind:this={scroller} bind:clientHeight={viewH} onscroll={() => (scrollTop = scroller?.scrollTop ?? 0)} data-autoscroll>
      <div
        class="tree"
        bind:this={treeEl}
        role="tree"
        aria-label={t('panels.explorer')}
        aria-activedescendant={activeDescendant}
        tabindex="0"
        style:height="{rows.length * H}px"
        onkeydown={onKeydown}
        onfocusin={onFocusIn}
        onfocusout={onFocusOut}
      >
        {#each slice as r, i (r.key)}
          {@const top = (start + i) * H}
          {@const renaming = explorer.renaming === r.key}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <div
            id="ex-{r.key}"
            class="trow t-{r.type}"
            class:focused={explorer.focusKey === r.key}
            class:active={activeKey === r.key}
            class:missing={r.entry?.missing}
            class:hidden-board={r.entry?.hidden}
            class:archived={r.archived}
            class:drag-src={!!dragging?.has(r.id) && r.type === 'card'}
            role="treeitem"
            aria-level={r.depth + 1}
            aria-expanded={r.expandable ? r.expanded : undefined}
            aria-selected={explorer.focusKey === r.key}
            tabindex="-1"
            style:transform="translateY({top}px)"
            style:--depth={r.depth}
            data-key={r.key}
            data-tree={r.type === 'board' && !r.entry?.missing ? 'board' : r.type === 'lane' || r.type === 'card' ? r.type : undefined}
            data-board={r.boardId || undefined}
            data-kind={r.kind}
            data-first-lane={r.type === 'board' ? r.firstLane : undefined}
            data-id={r.type === 'lane' || r.type === 'card' ? r.id : undefined}
            data-parent-kind={r.parent?.kind}
            data-parent-id={r.parent && 'id' in r.parent ? r.parent.id : undefined}
            data-next={r.type === 'card' ? r.next || undefined : undefined}
            onpointerdown={(e) => onRowPointerDown(e, r)}
            onclick={(e) => onRowClick(e, r)}
            ondblclick={() => onRowDblClick(r)}
            onauxclick={(e) => onAuxClick(e, r)}
            oncontextmenu={(e) => onContextMenu(e, r)}
          >
            {#if r.expandable}
              <button
                class="chev"
                class:open={r.expanded}
                tabindex="-1"
                aria-label={r.expanded ? t('explorer.collapse') : t('explorer.expand')}
                onclick={(e) => chevron(e, r)}
              >
                <ChevronRight size={13} strokeWidth={2} />
              </button>
            {:else}
              <span class="chev-space"></span>
            {/if}

            {#if r.type === 'section'}
              <span class="sec-label grow">{r.label}</span>
              <span class="count">{r.count}</span>
            {:else if r.type === 'board'}
              <span class="ico">
                {#if r.entry?.missing}<TriangleAlert size={14} strokeWidth={1.8} />
                {:else if r.mirror}<Diamond size={13} strokeWidth={2} />
                {:else if r.kind === 'files'}<FileText size={14} strokeWidth={1.8} />
                {:else}<LayoutGrid size={14} strokeWidth={1.8} />{/if}
              </span>
              {#if renaming}
                <input class="rename" use:renameInput={r} aria-label={t('explorer.rename')} />
              {:else}
                <span class="label grow">{r.label}</span>
                {#if r.entry?.missing}<span class="tag warn">{t('explorer.missing')}</span>{/if}
                {#if r.entry?.hidden}<EyeOff size={12} strokeWidth={1.8} class="meta-ico" />{/if}
                {#if r.entry?.pinned}<Pin size={12} strokeWidth={1.8} class="meta-ico" />{/if}
                {#if r.count !== undefined}<span class="count">{r.count}</span>{/if}
              {/if}
            {:else if r.type === 'lane'}
              <span class="dot" style:background={r.color ?? 'var(--line-strong)'}></span>
              {#if renaming}
                <input class="rename" use:renameInput={r} aria-label={t('explorer.rename')} />
              {:else}
                <span class="label grow">{r.label}</span>
                <span class="count">{r.count}</span>
              {/if}
            {:else if r.type === 'card'}
              <span class="ico small">
                {#if r.isGroup}<Layers size={13} strokeWidth={1.8} />
                {:else if r.kind === 'files'}<FileText size={13} strokeWidth={1.8} />
                {:else}<span class="card-dot"></span>{/if}
              </span>
              {#if renaming}
                <input class="rename" use:renameInput={r} aria-label={t('explorer.rename')} />
              {:else}
                <span class="label grow">{r.label}</span>
                {#if r.remoteKey}<span class="tag">{r.remoteKey}</span>{/if}
                {#if r.count}<span class="count">{r.count}</span>{/if}
              {/if}
            {:else if r.type === 'loading'}
              <Loader size={13} class="spin" />
              <span class="muted grow">{t('common.loading')}</span>
            {:else if r.type === 'error'}
              <TriangleAlert size={13} strokeWidth={1.8} />
              <span class="muted grow">{t('explorer.loadFailed')}</span>
              <button class="icon-btn sm" tabindex="-1" aria-label={t('common.retry')} onclick={(e) => (e.stopPropagation(), retry(r.boardId))}
                ><RotateCw size={12} /></button
              >
            {:else if r.type === 'empty'}
              <span class="muted grow"
                >{r.kind === 'files' || boards.get(r.boardId)?.kind === 'files'
                  ? t('explorer.noDocs')
                  : boards.get(r.boardId)?.lanes.length
                    ? t('explorer.noCards')
                    : t('explorer.noLanes')}</span
              >
            {:else if r.type === 'hint'}
              <Fingerprint size={12} strokeWidth={1.8} />
              <span class="muted grow" title={r.path}>{t('explorer.copyFound')}</span>
              <button class="icon-btn sm" tabindex="-1" aria-label={t('cards.reveal')} onclick={(e) => (e.stopPropagation(), openMenu(e, rowMenu(r)))}
                ><FolderSearch size={12} /></button
              >
            {/if}
          </div>
        {/each}
      </div>
    </div>
  {/if}
</div>

<style>
  .explorer {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .toolbar {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 2px 10px 6px 14px;
  }
  .grow {
    flex: 1;
    min-width: 0;
  }
  .scan {
    display: inline-grid;
    place-items: center;
    width: 22px;
    color: var(--ink-4);
  }
  .scroller {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    overflow-x: hidden;
    padding: 0 6px 12px;
  }
  .tree {
    position: relative;
    outline: none;
  }
  .trow {
    position: absolute;
    left: 0;
    right: 0;
    top: 0;
    height: 28px;
    display: flex;
    align-items: center;
    gap: 6px;
    padding-left: calc(4px + var(--depth) * 14px);
    padding-right: 8px;
    border-radius: var(--r-xs);
    color: var(--ink-2);
    font-size: var(--fs-md);
    cursor: default;
    user-select: none;
    transition: background var(--dur-fast) var(--ease);
  }
  .trow:hover {
    background: var(--bg-hover);
  }
  .trow.active {
    background: var(--primary-soft);
    color: var(--primary-strong);
  }
  .tree:focus-visible .trow.focused,
  .tree:focus .trow.focused {
    box-shadow: inset 0 0 0 1.5px var(--primary-ring);
  }
  .t-section {
    margin-top: 2px;
  }
  .sec-label {
    font-size: var(--fs-xs);
    font-weight: var(--fw-semibold);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--ink-3);
  }
  .t-board .label {
    font-weight: var(--fw-medium);
    color: var(--ink);
  }
  .trow.active .label {
    color: inherit;
  }
  .label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .chev,
  .chev-space {
    width: 16px;
    height: 16px;
    flex-shrink: 0;
  }
  .chev {
    display: grid;
    place-items: center;
    padding: 0;
    border: none;
    background: transparent;
    color: var(--ink-4);
    border-radius: 4px;
    transition: transform var(--dur) var(--ease);
  }
  .chev:hover {
    color: var(--ink);
    background: var(--bg-active);
  }
  .chev.open {
    transform: rotate(90deg);
  }
  .ico {
    display: inline-grid;
    place-items: center;
    width: 16px;
    flex-shrink: 0;
    color: var(--ink-3);
  }
  .t-board .ico {
    color: var(--primary);
  }
  .trow.missing .ico {
    color: var(--warn);
  }
  .card-dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--ink-4);
  }
  .dot {
    width: 8px;
    height: 8px;
    margin: 0 4px;
    border-radius: 50%;
    flex-shrink: 0;
  }
  .count {
    font-size: var(--fs-xs);
    color: var(--ink-4);
    font-variant-numeric: tabular-nums;
  }
  .tag {
    font-size: 10.5px;
    padding: 0 6px;
    line-height: 16px;
    border-radius: var(--r-pill);
    background: var(--bg-active);
    color: var(--ink-3);
    white-space: nowrap;
  }
  .tag.warn {
    background: var(--warn-soft);
    color: var(--warn);
  }
  .trow :global(.meta-ico) {
    color: var(--ink-4);
    flex-shrink: 0;
  }
  .muted {
    color: var(--ink-4);
    font-size: var(--fs-sm);
    font-style: italic;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .t-hint .muted {
    color: var(--warn);
    font-style: normal;
  }
  .trow.missing .label,
  .trow.hidden-board .label,
  .trow.archived .label {
    opacity: 0.55;
  }
  .trow.archived .label {
    text-decoration: line-through;
    text-decoration-color: var(--ink-4);
  }
  .trow.drag-src {
    opacity: 0.45;
  }
  .trow:global(.dnd-into) {
    background: var(--primary-soft);
    box-shadow: inset 0 0 0 1.5px var(--primary);
  }
  .rename {
    flex: 1;
    min-width: 0;
    height: 22px;
    padding: 0 6px;
    border-radius: 5px;
    border: 1px solid var(--primary);
    background: var(--bg-elev);
    color: var(--ink);
    font: inherit;
    box-shadow: 0 0 0 3px var(--focus);
    outline: none;
  }
  .empty {
    flex: 1;
    align-content: center;
  }
</style>
