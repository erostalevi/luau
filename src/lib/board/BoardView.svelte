<script lang="ts">
  import { setContext, onDestroy } from 'svelte';
  import {
    Columns3,
    Rows3,
    Search,
    Archive,
    Plus,
    MoreHorizontal,
    AlignVerticalSpaceAround,
    AlignHorizontalSpaceAround,
    ZoomIn,
    FolderSearch,
    Pencil,
    Repeat,
    Download,
    Diamond,
    TriangleAlert,
  } from '@lucide/svelte';
  import type { BoardModel } from '$lib/state/boards.svelte';
  import { apply, newLaneId } from '$lib/state/boards.svelte';
  import { ui, boardZoom, setBoardZoom } from '$lib/state/ui.svelte';
  import { ctx } from '$lib/commands/context.svelte';
  import { clearSelection, selectMany, selection } from '$lib/state/selection.svelte';
  import { runCommand } from '$lib/commands/registry.svelte';
  import { openMenuAt } from '$lib/state/menu.svelte';
  import { tip } from '$lib/components/tooltip';
  import Segmented from '$lib/components/Segmented.svelte';
  import { t } from '$lib/i18n/index.svelte';
  import Lane from './Lane.svelte';
  import FilterBar from './FilterBar.svelte';
  import { compileFilter } from './filter';
  import { boardUi } from './boardUi.svelte';
  import { dnd } from './dnd.svelte';
  import { keepScroll } from '$lib/components/keepScroll';

  let { board }: { board: BoardModel } = $props();

  const view = $derived(board.header.view);
  const zoom = $derived(boardZoom(board.id));
  const filterState = $derived(ui.filter[board.id] ?? { open: false, text: '' });
  const compiled = $derived(compileFilter(filterState.open ? filterState.text : ''));
  setContext('boardFilter', () => compiled);
  const lanes = $derived(board.lanes.filter((l) => ui.showArchived || !l.archived));
  const readOnly = $derived(!!board.header.readOnly);
  const matchCount = $derived.by(() => {
    if (compiled.empty) return 0;
    let n = 0;
    for (const node of board.nodes.values()) if (!node.isGroup && compiled.test(node, board.remote.get(node.id))) n++;
    return n;
  });

  let scroller: HTMLDivElement | undefined = $state();
  let lasso = $state<{ x0: number; y0: number; x1: number; y1: number } | null>(null);
  let newLaneName = $state('');

  const orientation = $derived(view.orientation);
  const orientationValue = $derived(orientation);

  async function setOrientation(o: 'columns' | 'rows') {
    const v = { ...view, orientation: o };
    board.header = { ...board.header, view: v };
    await apply(board.id, { op: 'updateBoard', patch: { view: v } }, t('commands.board.toggleOrientation'));
  }

  async function toggleSpacing() {
    await runCommand('board.toggleSpacing');
  }

  function onfocusin() {
    ctx.boardFocus = true;
  }
  // WebKit does not fire focusout when a focused board unmounts (tab switch).
  onDestroy(() => (ctx.boardFocus = false));
  function onfocusout(e: FocusEvent) {
    if (!(e.currentTarget as HTMLElement).contains(e.relatedTarget as Node)) ctx.boardFocus = false;
  }

  // Trackpad pinch (ctrl+wheel on Chromium/WebKitGTK/WebView2, gesture events on WebKit).
  function pinch(node: HTMLElement) {
    let gestureStart = 1;
    const wheel = (e: WheelEvent) => {
      if (e.ctrlKey || e.metaKey) {
        e.preventDefault();
        setBoardZoom(board.id, boardZoom(board.id) * Math.exp(-e.deltaY * 0.01));
      }
    };
    const gs = (e: Event) => {
      e.preventDefault();
      gestureStart = boardZoom(board.id);
    };
    const gc = (e: Event) => {
      e.preventDefault();
      setBoardZoom(board.id, gestureStart * (e as unknown as { scale: number }).scale);
    };
    node.addEventListener('wheel', wheel, { passive: false });
    node.addEventListener('gesturestart', gs);
    node.addEventListener('gesturechange', gc);
    return {
      destroy() {
        node.removeEventListener('wheel', wheel);
        node.removeEventListener('gesturestart', gs);
        node.removeEventListener('gesturechange', gc);
      },
    };
  }

  function onBackgroundPointerDown(e: PointerEvent) {
    if (e.button !== 0) return;
    const target = e.target as HTMLElement;
    if (target.closest('[data-card], button, input, textarea, .lhead')) return;
    scroller?.focus({ preventScroll: true });
    ctx.boardFocus = true;
    if (!(e.metaKey || e.ctrlKey || e.shiftKey)) {
      clearSelection();
      const laneEl = target.closest<HTMLElement>('[data-lane]');
      selection.lane = laneEl?.dataset.lane ?? '';
      selection.boardId = board.id;
      if (!e.metaKey && !e.ctrlKey) return;
    }
    // ⌘/Ctrl-drag: Finder-like area selection.
    e.preventDefault();
    const additive = e.shiftKey;
    const start = { x: e.clientX, y: e.clientY };
    const base = additive ? [...selection.ids] : [];
    const move = (ev: PointerEvent) => {
      lasso = { x0: start.x, y0: start.y, x1: ev.clientX, y1: ev.clientY };
      const [l, r] = [Math.min(start.x, ev.clientX), Math.max(start.x, ev.clientX)];
      const [tp, b] = [Math.min(start.y, ev.clientY), Math.max(start.y, ev.clientY)];
      const hit: string[] = [];
      scroller?.querySelectorAll<HTMLElement>('[data-card]').forEach((el) => {
        if (el.dataset.group === '1') return;
        const rc = el.getBoundingClientRect();
        if (rc.right > l && rc.left < r && rc.bottom > tp && rc.top < b) hit.push(el.dataset.card!);
      });
      selectMany(board.id, [...new Set([...base, ...hit])]);
    };
    const up = () => {
      lasso = null;
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
  }

  async function createLane() {
    const name = newLaneName.trim();
    if (!name) {
      boardUi.newLane = null;
      return;
    }
    newLaneName = '';
    const id = await newLaneId(board.id);
    await apply(board.id, { op: 'createLane', id, name, index: boardUi.newLane?.index ?? null }, t('ops.createLane'));
    requestAnimationFrame(() =>
      scroller?.scrollTo({ left: scroller.scrollWidth, top: orientation === 'rows' ? scroller.scrollHeight : 0, behavior: 'smooth' }),
    );
  }

  function moreMenu(e: MouseEvent) {
    openMenuAt(e.currentTarget as HTMLElement, [
      { label: t('commands.board.rename'), icon: Pencil, command: 'board.rename', disabled: readOnly },
      { label: t('commands.board.toggleSpacing'), icon: AlignHorizontalSpaceAround, command: 'board.toggleSpacing' },
      { label: ui.showArchived ? t('board.hideArchived') : t('board.showArchived'), icon: Archive, command: 'board.toggleArchived' },
      { label: t('commands.board.convert'), icon: Repeat, command: 'board.convert', disabled: readOnly },
      { separator: true },
      { label: t('commands.board.export'), icon: Download, command: 'board.export' },
      { label: t('commands.board.reveal'), icon: FolderSearch, command: 'board.reveal' },
      { separator: true },
      { label: t('board.zoomReset', { pct: Math.round(zoom * 100) }), icon: ZoomIn, command: 'view.zoomReset' },
    ]);
  }
</script>

<div class="board" role="application" aria-label={board.header.name} {onfocusin} {onfocusout}>
  <header class="bhead">
    <div class="titlebox">
      {#if board.remote.size || board.header.readOnly?.startsWith('mirror')}<Diamond size={15} class="muted" />{/if}
      <h1>{board.header.name}</h1>
      {#if board.header.readOnly === 'loose'}
        <span class="chip warn" use:tip={t('io.loose.tip')}><TriangleAlert size={12} /> {t('board.readOnly')}</span>
        <button class="btn sm soft" onclick={() => runCommand('board.import', { path: board.header.root })}>{t('io.loose.importCopy')}</button>
      {:else if board.header.readOnly === 'corrupt_manifest'}
        <span class="chip warn" use:tip={t('io.repair.tip')}><TriangleAlert size={12} /> {t('board.readOnly')}</span>
        <button class="btn sm soft" onclick={() => runCommand('board.repair')}>{t('io.repair.confirm')}</button>
      {:else if board.header.readOnly && !board.header.readOnly.startsWith('mirror')}
        <span class="chip warn" use:tip={t('board.readOnlyTip')}><TriangleAlert size={12} /> {t('board.readOnly')}</span>
        {#if board.header.readOnly.startsWith('newer_schema')}
          <button class="btn sm soft" onclick={() => runCommand('board.upgradeSchema')}>{t('board.convertVersion')}</button>
        {/if}
      {/if}
      {#if board.header.warnings.length}
        <span class="chip warn" use:tip={board.header.warnings.join('\n')}
          ><TriangleAlert size={12} /> {t('board.recovered', { count: board.header.warnings.length })}</span
        >
      {/if}
    </div>
    <div class="tools">
      {#if zoom !== 1}<button class="chip" onclick={() => setBoardZoom(board.id, 1)} use:tip={t('commands.view.zoomReset')}>{Math.round(zoom * 100)}%</button
        >{/if}
      <button
        class="icon-btn"
        class:active={filterState.open}
        onclick={() => runCommand('board.filter')}
        use:tip={{ text: t('commands.board.filter'), command: 'board.filter' }}><Search size={16} /></button
      >
      <button
        class="icon-btn"
        class:active={ui.showArchived}
        onclick={() => runCommand('board.toggleArchived')}
        use:tip={{ text: t('commands.board.toggleArchived'), command: 'board.toggleArchived' }}><Archive size={16} /></button
      >
      <button class="icon-btn" onclick={toggleSpacing} use:tip={{ text: t(view.spacing === 'fixedMain' ? 'board.spacingMain' : 'board.spacingCross') }}>
        {#if (view.spacing === 'fixedMain') === (orientation === 'columns')}<AlignVerticalSpaceAround size={16} />{:else}<AlignHorizontalSpaceAround
            size={16}
          />{/if}
      </button>
      <Segmented
        size="sm"
        value={orientationValue}
        options={[
          { value: 'columns', icon: Columns3, title: t('board.columns') },
          { value: 'rows', icon: Rows3, title: t('board.rows') },
        ]}
        onchange={(v) => setOrientation(v as 'columns' | 'rows')}
      />
      <button class="icon-btn" onclick={moreMenu} use:tip={t('common.more')}><MoreHorizontal size={16} /></button>
    </div>
  </header>
  {#if filterState.open}
    <FilterBar boardId={board.id} count={matchCount} />
  {/if}
  <div
    class="scroll"
    bind:this={scroller}
    data-board-scroll
    data-autoscroll
    tabindex="-1"
    role="region"
    onpointerdown={onBackgroundPointerDown}
    use:pinch
    use:keepScroll={'board'}
  >
    <div class="lanes {orientation} {view.spacing}" style:zoom={zoom === 1 ? undefined : zoom}>
      {#each lanes as lane, i (lane.id)}
        <Lane boardId={board.id} {lane} index={i} {orientation} spacing={view.spacing} />
      {/each}
      {#if !readOnly}
        {#if boardUi.newLane?.boardId === board.id}
          <div class="new-lane {orientation}">
            <!-- svelte-ignore a11y_autofocus -->
            <input
              class="field"
              bind:value={newLaneName}
              placeholder={t('lanes.namePlaceholder')}
              autofocus
              onkeydown={(e) => {
                e.stopPropagation();
                if (e.key === 'Enter') void createLane();
                if (e.key === 'Escape') boardUi.newLane = null;
              }}
              onblur={() => void createLane()}
            />
          </div>
        {:else}
          <button
            class="add-lane {orientation}"
            onclick={() => (boardUi.newLane = { boardId: board.id, index: null })}
            use:tip={{ text: t('lanes.new'), command: 'lane.new' }}
          >
            <Plus size={16} />
            {#if !lanes.length}{t('lanes.new')}{/if}
          </button>
        {/if}
      {/if}
    </div>
    {#if !lanes.length && !readOnly}
      <div class="empty-board empty">
        <div class="empty-icon"><Columns3 size={22} /></div>
        <strong>{t('board.emptyTitle')}</strong>
        <span>{t('board.emptyHint')}</span>
      </div>
    {/if}
  </div>
  {#if lasso}
    <div
      class="lasso"
      style:left="{Math.min(lasso.x0, lasso.x1)}px"
      style:top="{Math.min(lasso.y0, lasso.y1)}px"
      style:width="{Math.abs(lasso.x1 - lasso.x0)}px"
      style:height="{Math.abs(lasso.y1 - lasso.y0)}px"
    ></div>
  {/if}
  {#if dnd.active && dnd.target && !dnd.target.valid}
    <div class="deny-hint">{board.header.readOnly?.startsWith('mirror') ? t('board.mirrorDragHint') : t('board.cannotDrop')}</div>
  {/if}
</div>

<style>
  .board {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    position: relative;
  }
  .bhead {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 14px 20px 10px 24px;
  }
  .titlebox {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
    flex: 1;
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
  .tools {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .chip.warn {
    background: var(--warn-soft);
    color: var(--warn);
  }
  .scroll {
    flex: 1;
    min-height: 0;
    overflow: auto;
    outline: none;
    padding: 4px 20px 24px;
  }
  .lanes {
    display: flex;
    gap: 14px;
    min-height: 100%;
    width: max-content;
    min-width: 100%;
  }
  .lanes.columns {
    flex-direction: row;
    align-items: flex-start;
    height: 100%;
  }
  .lanes.rows {
    flex-direction: column;
    width: 100%;
  }
  .add-lane {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    border: 1.5px dashed var(--line-strong);
    border-radius: var(--r-lg);
    background: transparent;
    color: var(--ink-3);
    font-weight: var(--fw-medium);
    transition:
      background var(--dur-fast) var(--ease),
      color var(--dur-fast) var(--ease),
      border-color var(--dur-fast) var(--ease);
    flex-shrink: 0;
  }
  .add-lane:hover {
    background: var(--bg-hover);
    color: var(--ink);
    border-color: var(--ink-4);
  }
  .add-lane.columns {
    width: 48px;
    height: 48px;
  }
  .add-lane.rows {
    height: 44px;
  }
  .new-lane.columns {
    width: var(--lane-w);
    flex-shrink: 0;
  }
  .new-lane .field {
    height: 40px;
    border-radius: var(--r-md);
    font-weight: var(--fw-semibold);
  }
  .empty-board {
    position: absolute;
    inset: 40% 0 auto;
    pointer-events: none;
  }
  .lasso {
    position: fixed;
    z-index: 50;
    border: 1px solid var(--primary);
    background: color-mix(in srgb, var(--primary) 12%, transparent);
    border-radius: 4px;
    pointer-events: none;
  }
  .deny-hint {
    position: absolute;
    bottom: 18px;
    left: 50%;
    transform: translateX(-50%);
    padding: 7px 14px;
    border-radius: var(--r-pill);
    background: var(--bg-elev);
    box-shadow: var(--shadow-2);
    font-size: var(--fs-sm);
    color: var(--ink-2);
    pointer-events: none;
    animation: pop-in var(--dur) var(--ease-out);
  }
</style>
