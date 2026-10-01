<script lang="ts">
  // Top-right cluster of the top bar: pending chord hint, status light and
  // undo/redo for the active board. Replaces the old bottom status bar.
  // Every control opts out of the window drag region.
  import { Keyboard, Undo2, Redo2 } from '@lucide/svelte';
  import { kb } from '$lib/keybindings/resolver.svelte';
  import { formatKey } from '$lib/keybindings/keys';
  import { activeTab } from '$lib/state/workspace.svelte';
  import { boards } from '$lib/state/boards.svelte';
  import { runCommand } from '$lib/commands/registry.svelte';
  import { tip } from '$lib/components/tooltip';
  import { t } from '$lib/i18n/index.svelte';
  import StatusLight from './StatusLight.svelte';

  const tab = $derived(activeTab());
  const board = $derived(tab?.boardId ? boards.get(tab.boardId) : undefined);
</script>

<div class="actions">
  {#if kb.pending}
    <span class="chord" role="status"><Keyboard size={13} /> {t('keys.chordWaiting', { key: formatKey(kb.pending).join('') })}</span>
  {/if}
  <span class="no-drag" data-tauri-drag-region="false"><StatusLight /></span>
  {#if board}
    <span class="undo no-drag" data-tauri-drag-region="false">
      <button
        class="icon-btn sm"
        aria-label={t('common.undo')}
        disabled={!board.undo.canUndo}
        onclick={() => runCommand('edit.undo')}
        use:tip={{ text: t('common.undo'), command: 'edit.undo', placement: 'bottom' }}
      >
        <Undo2 size={15} strokeWidth={1.8} />
      </button>
      <button
        class="icon-btn sm"
        aria-label={t('common.redo')}
        disabled={!board.undo.canRedo}
        onclick={() => runCommand('edit.redo')}
        use:tip={{ text: t('common.redo'), command: 'edit.redo', placement: 'bottom' }}
      >
        <Redo2 size={15} strokeWidth={1.8} />
      </button>
    </span>
  {/if}
</div>

<style>
  .actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
    min-width: 0;
    padding: 0 4px 0 6px;
    font-size: var(--fs-xs);
    color: var(--ink-3);
  }
  .undo {
    display: inline-flex;
    align-items: center;
    gap: 2px;
  }
  .undo button:disabled {
    opacity: 0.35;
  }
  .chord {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--primary-strong);
    font-weight: var(--fw-medium);
    white-space: nowrap;
  }
</style>
