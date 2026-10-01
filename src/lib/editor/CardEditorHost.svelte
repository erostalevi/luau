<script lang="ts">
  // Hosts the card editor either as a centered modal or a right sidebar
  // (user setting). The pinned sidebar follows the board selection.
  import { untrack } from 'svelte';
  import { fade, fly, scale } from 'svelte/transition';
  import { ui, closeEditor, openEditor } from '$lib/state/ui.svelte';
  import { settings } from '$lib/settings/store.svelte';
  import { selection } from '$lib/state/selection.svelte';
  import { boards } from '$lib/state/boards.svelte';
  import { ctx } from '$lib/commands/context.svelte';
  import CardEditor from './CardEditor.svelte';

  let { mode }: { mode: 'modal' | 'sidebar' } = $props();

  const active = $derived(settings.get<string>('editor.openMode') === mode);
  const board = $derived(boards.get(ui.editor.boardId));
  const exists = $derived(!!board?.nodes.get(ui.editor.cardId));
  const visible = $derived(active && ui.editor.open && exists && board?.kind !== 'files');
  const pinned = $derived(settings.get<boolean>('editor.sidebarPinned'));
  let width = $state(settings.get<number>('editor.sidebarWidth') || 520);

  $effect(() => {
    if (mode === 'modal') ctx.modalOpen = visible;
  });

  // Pinned sidebar follows the focused card — but a card the user just closed
  // stays closed until the selection moves to another card.
  let dismissed: string | null = null;
  $effect(() => {
    if (!ui.editor.open && ui.editor.cardId) dismissed = ui.editor.cardId;
  });
  $effect(() => {
    if (mode !== 'sidebar' || !pinned || !active) return;
    const id = selection.focus;
    const bid = selection.boardId;
    untrack(() => {
      if (id !== dismissed) dismissed = null;
      if (!id || !bid || id === dismissed) return;
      if (boards.get(bid)?.nodes.get(id) && boards.get(bid)?.kind !== 'files' && (id !== ui.editor.cardId || !ui.editor.open)) openEditor(bid, id);
    });
  });

  // Card deleted while open → close.
  $effect(() => {
    if (ui.editor.open && board && !exists) closeEditor();
  });

  function onkeydown(e: KeyboardEvent) {
    if (e.key !== 'Escape' || !visible) return;
    // Let autocomplete / vim / search panels consume Escape first.
    const t = e.target as HTMLElement;
    if (t.closest('.cm-editor') && (document.querySelector('.cm-tooltip-autocomplete') || document.querySelector('.cm-panel') || t.closest('.cm-vim-insert'))) return;
    if (e.defaultPrevented || document.querySelector('.menu, .qi, [data-overlay]')) return;
    e.preventDefault();
    closeEditor();
  }

  function startResize(e: PointerEvent) {
    const startX = e.clientX;
    const startW = width;
    const move = (ev: PointerEvent) => (width = Math.round(Math.min(window.innerWidth * 0.7, Math.max(360, startW - (ev.clientX - startX)))));
    const up = () => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
      document.documentElement.style.cursor = '';
      settings.set('editor.sidebarWidth', width);
    };
    document.documentElement.style.cursor = 'col-resize';
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
  }
</script>

<svelte:window {onkeydown} />

{#if visible}
  {#if mode === 'modal'}
    <div class="scrim" role="presentation" transition:fade={{ duration: 160 }} onpointerdown={() => closeEditor()}></div>
    <div class="modal card-surface" role="dialog" aria-modal="true" transition:scale={{ start: 0.97, duration: 200 }}>
      <CardEditor boardId={ui.editor.boardId} cardId={ui.editor.cardId} variant="modal" />
    </div>
  {:else}
    <aside class="sidebar" style:width="{width}px" transition:fly={{ x: 40, duration: 220 }}>
      <div class="resize" role="separator" aria-orientation="vertical" onpointerdown={startResize}></div>
      <CardEditor boardId={ui.editor.boardId} cardId={ui.editor.cardId} variant="sidebar" />
    </aside>
  {/if}
{/if}

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 400;
    background: var(--bg-overlay);
    backdrop-filter: blur(3px);
  }
  .modal {
    position: fixed;
    z-index: 401;
    top: max(40px, 6vh);
    bottom: max(32px, 5vh);
    left: 50%;
    translate: -50% 0;
    width: min(calc(var(--editor-w) + 140px), calc(100vw - 64px));
    border-radius: var(--r-xl);
    overflow: hidden;
  }
  .sidebar {
    position: relative;
    flex-shrink: 0;
    height: 100%;
    border-left: 1px solid var(--line);
    background: var(--bg-elev);
    box-shadow: -8px 0 32px rgb(30 30 70 / 0.05);
    min-width: 360px;
  }
  .resize {
    position: absolute;
    left: -3px;
    top: 0;
    bottom: 0;
    width: 6px;
    cursor: col-resize;
    z-index: 3;
  }
  .resize:hover {
    background: var(--primary-ring);
  }
</style>
