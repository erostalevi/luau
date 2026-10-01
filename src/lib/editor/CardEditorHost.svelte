<script lang="ts">
  // Hosts the card editor as a centered modal or a right sidebar (user
  // setting). One surface serves both modes, so switching keeps the same
  // CardEditor instance (cursor, scroll, undo) and morphs the box between the
  // two rectangles. The pinned sidebar follows the board selection.
  import { tick, untrack } from 'svelte';
  import { fade } from 'svelte/transition';
  import { ui, closeEditor, openEditor } from '$lib/state/ui.svelte';
  import { settings } from '$lib/settings/store.svelte';
  import { selection } from '$lib/state/selection.svelte';
  import { boards } from '$lib/state/boards.svelte';
  import { ctx } from '$lib/commands/context.svelte';
  import CardEditor from './CardEditor.svelte';
  import { heroTransition, morphBox, motion, MORPH_MS } from './cardMorph';

  type Mode = 'modal' | 'sidebar';
  const mode = $derived<Mode>(settings.get<string>('editor.openMode') === 'sidebar' ? 'sidebar' : 'modal');
  /** Mode currently rendered; trails `mode` by one measurement for the morph. */
  let shown = $state<Mode>(untrack(() => mode));
  const board = $derived(boards.get(ui.editor.boardId));
  const exists = $derived(!!board?.nodes.get(ui.editor.cardId));
  const visible = $derived(ui.editor.open && exists && board?.kind !== 'files');
  const pinned = $derived(settings.get<boolean>('editor.sidebarPinned'));
  let width = $state(settings.get<number>('editor.sidebarWidth') || 520);
  let el: HTMLElement | undefined = $state();
  let morph: Animation | null = null;

  $effect(() => {
    ctx.modalOpen = visible && shown === 'modal';
  });

  // Mode switch: measure the old box, re-render in the new mode, then morph.
  $effect(() => {
    const next = mode;
    untrack(() => {
      if (next === shown) return;
      if (!el || !visible || motion() === 'none') shown = next;
      else void switchMode(el, next);
    });
  });

  async function switchMode(node: HTMLElement, next: Mode) {
    const first = node.getBoundingClientRect();
    const radius = getComputedStyle(node).borderRadius;
    morph?.cancel();
    morph = null;
    delete node.dataset.morph;
    shown = next;
    await tick();
    if (el !== node) return;
    if (motion() === 'reduced') node.animate([{ opacity: 0.4 }, { opacity: 1 }], { duration: 140 });
    else morph = morphBox(node, first, radius);
    // The toggle button that had focus went away with the old header: hand
    // focus back to the editor (its selection is intact).
    const a = document.activeElement;
    if (!a || a === document.body) node.querySelector<HTMLElement>('.cm-content')?.focus({ preventScroll: true });
  }

  // Pinned sidebar follows the focused card — but a card the user just closed
  // stays closed until the selection moves to another card.
  let dismissed: string | null = null;
  $effect(() => {
    if (!ui.editor.open && ui.editor.cardId) dismissed = ui.editor.cardId;
  });
  $effect(() => {
    if (mode !== 'sidebar' || !pinned) return;
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
    if (t.closest('.cm-editor') && (document.querySelector('.cm-tooltip-autocomplete') || document.querySelector('.cm-panel') || t.closest('.cm-vim-insert')))
      return;
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

  function scrimMs() {
    const m = motion();
    return m === 'none' ? 0 : m === 'reduced' ? 120 : MORPH_MS;
  }
</script>

<svelte:window {onkeydown} />

{#if visible}
  {#if shown === 'modal'}
    <div class="scrim" role="presentation" transition:fade|global={{ duration: scrimMs() }} onpointerdown={() => closeEditor()}></div>
  {:else}
    <!-- Reserves the sidebar's width in the layout; the surface is laid over it. -->
    <div class="slot" style:width="{width}px"></div>
  {/if}
  <div
    bind:this={el}
    class="surface"
    class:modal={shown === 'modal'}
    class:card-surface={shown === 'modal'}
    class:sidebar={shown === 'sidebar'}
    style:--sidebar-w="{width}px"
    role={shown === 'modal' ? 'dialog' : 'complementary'}
    aria-modal={shown === 'modal' ? 'true' : undefined}
    transition:heroTransition={{ boardId: ui.editor.boardId, cardId: ui.editor.cardId, mode: shown }}
  >
    {#if shown === 'sidebar'}
      <div class="resize" role="separator" aria-orientation="vertical" onpointerdown={startResize}></div>
    {/if}
    <CardEditor boardId={ui.editor.boardId} cardId={ui.editor.cardId} variant={shown} />
  </div>
{/if}

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 400;
    background: var(--bg-overlay);
    backdrop-filter: blur(3px);
  }
  .slot {
    flex-shrink: 0;
    min-width: 360px;
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
  /* Laid over .slot; the app body (position: relative) is the containing block. */
  .sidebar {
    position: absolute;
    z-index: 2;
    top: 0;
    right: 0;
    bottom: 0;
    width: max(360px, var(--sidebar-w));
    border-left: 1px solid var(--line);
    background: var(--bg-elev);
    box-shadow: -8px 0 32px rgb(30 30 70 / 0.05);
  }
  /* While morphing between modes the animation drives the box in viewport
     coordinates (left/top/width/height keyframes). */
  .surface:global([data-morph]) {
    position: fixed;
    z-index: 401;
    right: auto;
    bottom: auto;
    translate: none;
    overflow: hidden;
    box-shadow: var(--shadow-3);
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
