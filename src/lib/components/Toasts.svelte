<script lang="ts">
  import { fly } from 'svelte/transition';
  import { flip } from 'svelte/animate';
  import { CircleCheck, Info, TriangleAlert, CircleX, X } from '@lucide/svelte';
  import { toasts, dismiss } from '$lib/state/toasts.svelte';

  const icons = { info: Info, success: CircleCheck, warn: TriangleAlert, error: CircleX };
</script>

<div class="toasts" aria-live="polite">
  {#each toasts.list as tst (tst.id)}
    {@const Icon = icons[tst.level]}
    <div class="toast card-surface glass {tst.level}" animate:flip={{ duration: 200 }} transition:fly={{ y: 16, duration: 220 }}>
      <Icon size={16} strokeWidth={2} />
      <span class="msg">{tst.message}</span>
      {#if tst.action}
        <button
          class="btn sm soft"
          onclick={() => {
            tst.action?.run();
            dismiss(tst.id);
          }}>{tst.action.label}</button
        >
      {/if}
      <button class="icon-btn sm" onclick={() => dismiss(tst.id)} aria-label="Dismiss"><X size={13} /></button>
    </div>
  {/each}
</div>

<style>
  .toasts {
    position: fixed;
    bottom: calc(var(--statusbar-h) + 16px);
    left: 50%;
    transform: translateX(-50%);
    z-index: 1500;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    pointer-events: none;
  }
  .toast {
    pointer-events: auto;
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 280px;
    max-width: 520px;
    padding: 9px 10px 9px 14px;
    border-radius: var(--r-md);
    font-size: var(--fs-md);
  }
  .msg {
    flex: 1;
  }
  .info :global(svg:first-child) {
    color: var(--info);
  }
  .success :global(svg:first-child) {
    color: var(--ok);
  }
  .warn :global(svg:first-child) {
    color: var(--warn);
  }
  .error :global(svg:first-child) {
    color: var(--danger);
  }
</style>
