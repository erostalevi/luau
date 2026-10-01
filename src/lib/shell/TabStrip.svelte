<script lang="ts">
  import { X, Plus, Pin, LayoutGrid, FileText, House, Settings, Keyboard, Search, Sparkles, Diamond } from '@lucide/svelte';
  import { flip } from 'svelte/animate';
  import type { Snippet } from 'svelte';
  import { ws, focusTab, closeTab, moveTab, togglePin, closeOthers, type Pane, type Tab } from '$lib/state/workspace.svelte';
  import { boards } from '$lib/state/boards.svelte';
  import { boardEntry } from '$lib/state/registry.svelte';
  import { openMenu } from '$lib/state/menu.svelte';
  import { runCommand } from '$lib/commands/registry.svelte';
  import { tip } from '$lib/components/tooltip';
  import { t } from '$lib/i18n/index.svelte';

  let { pane, leading = 0, end }: { pane: Pane; leading?: number; end?: Snippet } = $props();

  let dragId = $state<string | null>(null);
  let overIndex = $state(-1);

  function title(tab: Tab): string {
    switch (tab.kind) {
      case 'board':
        return boards.get(tab.boardId!)?.header.name ?? boardEntry(tab.boardId!)?.name ?? t('tabs.board');
      case 'doc': {
        const n = boards.get(tab.boardId!)?.node(tab.cardId!);
        return n?.title || t('common.untitled');
      }
      case 'start':
        return t('tabs.start');
      case 'settings':
        return t('tabs.settings');
      case 'keybindings':
        return t('tabs.keybindings');
      case 'savedSearch':
        return String(tab.payload?.name ?? t('tabs.search'));
      case 'summary':
        return t('tabs.summary');
    }
  }

  function icon(tab: Tab) {
    if (tab.kind === 'board') return boardEntry(tab.boardId!)?.mirror ? Diamond : boards.get(tab.boardId!)?.kind === 'files' ? FileText : LayoutGrid;
    return { doc: FileText, start: House, settings: Settings, keybindings: Keyboard, savedSearch: Search, summary: Sparkles, board: LayoutGrid }[tab.kind];
  }

  function tabMenu(e: MouseEvent, tab: Tab) {
    openMenu(e, [
      { label: tab.pinned ? t('tabs.unpin') : t('tabs.pin'), icon: Pin, run: () => togglePin(tab.id) },
      { label: t('tabs.close'), run: () => closeTab(tab.id, true) },
      { label: t('tabs.closeOthers'), run: () => closeOthers(tab.id) },
      { separator: true },
      { label: t('tabs.splitRight'), command: 'view.splitRight' },
      { label: t('tabs.moveToNewWindow'), run: () => runCommand('tab.moveToNewWindow', tab.id) },
    ]);
  }

  // Pointer-based tab dragging (reorder + move between panes).
  function onpointerdown(e: PointerEvent, tab: Tab) {
    if (e.button === 1) {
      e.preventDefault();
      closeTab(tab.id);
      return;
    }
    if (e.button !== 0) return;
    focusTab(tab.id);
    const startX = e.clientX;
    const el = e.currentTarget as HTMLElement;
    let dragging = false;
    const move = (ev: PointerEvent) => {
      if (!dragging && Math.abs(ev.clientX - startX) > 6) {
        dragging = true;
        dragId = tab.id;
      }
      if (!dragging) return;
      const target = document.elementFromPoint(ev.clientX, ev.clientY)?.closest<HTMLElement>('[data-tab-index]');
      const strip = document.elementFromPoint(ev.clientX, ev.clientY)?.closest<HTMLElement>('[data-pane]');
      if (target) {
        const r = target.getBoundingClientRect();
        const i = Number(target.dataset.tabIndex);
        overIndex = ev.clientX > r.left + r.width / 2 ? i + 1 : i;
        (strip ?? el).dataset.over = strip?.dataset.pane ?? pane.id;
      }
    };
    const up = (ev: PointerEvent) => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
      if (dragging) {
        const strip = document.elementFromPoint(ev.clientX, ev.clientY)?.closest<HTMLElement>('[data-pane]');
        const toPane = strip?.dataset.pane ?? pane.id;
        moveTab(tab.id, toPane, overIndex < 0 ? 999 : overIndex);
      }
      dragId = null;
      overIndex = -1;
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
  }
</script>

<div class="strip drag-region" data-tauri-drag-region="deep" data-pane={pane.id} style:padding-left="{leading}px" role="tablist">
  <div class="tabs">
    {#each pane.tabs as tab, i (tab.id)}
      {@const Icon = icon(tab)}
      <div
        class="tab no-drag"
        class:active={pane.active === tab.id}
        class:current={ws.activePane === pane.id && pane.active === tab.id}
        class:pinned={tab.pinned}
        class:dragging={dragId === tab.id}
        class:drop-before={overIndex === i && dragId}
        data-tab-index={i}
        role="tab"
        tabindex="0"
        aria-selected={pane.active === tab.id}
        animate:flip={{ duration: 180 }}
        onpointerdown={(e) => onpointerdown(e, tab)}
        oncontextmenu={(e) => tabMenu(e, tab)}
        onkeydown={(e) => e.key === 'Enter' && focusTab(tab.id)}
        use:tip={tab.pinned ? title(tab) : null}
      >
        <Icon size={14} strokeWidth={1.9} />
        {#if !tab.pinned}
          <span class="title">{title(tab)}</span>
          <button
            class="close"
            aria-label={t('tabs.close')}
            onpointerdown={(e) => e.stopPropagation()}
            onclick={(e) => {
              e.stopPropagation();
              closeTab(tab.id);
            }}><X size={12} strokeWidth={2.2} /></button
          >
        {/if}
      </div>
    {/each}
    <button class="icon-btn sm add no-drag" onclick={() => runCommand('board.openPicker')} use:tip={{ text: t('tabs.new'), command: 'board.openPicker' }}>
      <Plus size={15} />
    </button>
  </div>
  <div class="drag-fill"></div>
  {@render end?.()}
</div>

<style>
  .strip {
    display: flex;
    align-items: center;
    height: var(--titlebar-h);
    /* Fill the whole top bar so every empty spot of it drags the window
       (tabs and buttons opt out with .no-drag). */
    flex: 1 1 auto;
    min-width: 0;
    padding-right: 6px;
  }
  .tabs {
    display: flex;
    align-items: center;
    gap: 2px;
    min-width: 0;
    overflow-x: auto;
    overflow-y: hidden;
    scrollbar-width: none;
    padding: 6px 4px;
  }
  .tabs::-webkit-scrollbar {
    display: none;
  }
  .drag-fill {
    flex: 1;
    align-self: stretch;
    min-width: 24px;
  }
  .tab {
    position: relative;
    display: flex;
    align-items: center;
    gap: 7px;
    height: 30px;
    min-width: 0;
    max-width: 220px;
    padding: 0 6px 0 11px;
    border-radius: var(--r-sm);
    color: var(--ink-3);
    font-size: var(--fs-sm);
    font-weight: var(--fw-medium);
    flex-shrink: 1;
    flex-basis: 170px;
    transition:
      background var(--dur-fast) var(--ease),
      color var(--dur-fast) var(--ease),
      opacity var(--dur-fast) var(--ease);
  }
  .tab:hover {
    background: var(--bg-hover);
    color: var(--ink-2);
  }
  .tab.active {
    background: var(--bg-elev);
    color: var(--ink);
    box-shadow: var(--shadow-1);
  }
  .tab.current {
    color: var(--ink);
  }
  .tab.current :global(svg:first-child) {
    color: var(--primary);
  }
  .tab.pinned {
    flex-basis: auto;
    padding: 0 10px;
  }
  .tab.dragging {
    opacity: 0.5;
  }
  .tab.drop-before::before {
    content: '';
    position: absolute;
    left: -2px;
    top: 6px;
    bottom: 6px;
    width: 2px;
    border-radius: 2px;
    background: var(--primary);
  }
  .title {
    flex: 1;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .close {
    display: grid;
    place-items: center;
    width: 18px;
    height: 18px;
    border: none;
    border-radius: 5px;
    background: transparent;
    color: var(--ink-4);
    opacity: 0;
    transition: opacity var(--dur-fast) var(--ease);
  }
  .tab:hover .close,
  .tab.active .close {
    opacity: 1;
  }
  .close:hover {
    background: var(--bg-active);
    color: var(--ink);
  }
  .add {
    margin-left: 2px;
  }
</style>
