<script lang="ts">
  import { Keyboard, Loader, Undo2, Redo2, CloudOff } from '@lucide/svelte';
  import { kb } from '$lib/keybindings/resolver.svelte';
  import { formatKey } from '$lib/keybindings/keys';
  import { activeTab } from '$lib/state/workspace.svelte';
  import { boards } from '$lib/state/boards.svelte';
  import { registry } from '$lib/state/registry.svelte';
  import { runCommand } from '$lib/commands/registry.svelte';
  import { integrationStatus } from '$lib/integrations/status.svelte';
  import { tip } from '$lib/components/tooltip';
  import { t } from '$lib/i18n/index.svelte';

  const tab = $derived(activeTab());
  const board = $derived(tab?.boardId ? boards.get(tab.boardId) : undefined);
  const counts = $derived.by(() => {
    if (!board) return null;
    let cards = 0;
    let groups = 0;
    for (const n of board.nodes.values()) (n.isGroup ? groups++ : cards++);
    return { cards, groups };
  });
</script>

<footer class="status">
  {#if kb.pending}
    <span class="chord"><Keyboard size={13} /> {t('keys.chordWaiting', { key: formatKey(kb.pending).join('') })}</span>
  {:else if registry.scanning}
    <span class="item"><Loader size={12} class="spin" /> {t('status.indexing', { done: registry.indexed.done, total: registry.indexed.total || '…' })}</span>
  {/if}
  <span class="grow"></span>
  {#if integrationStatus.offline}
    <span class="item warn"><CloudOff size={12} /> {integrationStatus.offline}</span>
  {/if}
  {#if board && counts}
    <span class="item">{t('status.cards', { count: counts.cards })}{counts.groups ? ` · ${t('status.groups', { count: counts.groups })}` : ''}</span>
    <button class="icon-btn sm" disabled={!board.undo.canUndo} onclick={() => runCommand('edit.undo')} use:tip={{ text: t('common.undo'), command: 'edit.undo', placement: 'top' }}>
      <Undo2 size={13} />
    </button>
    <button class="icon-btn sm" disabled={!board.undo.canRedo} onclick={() => runCommand('edit.redo')} use:tip={{ text: t('common.redo'), command: 'edit.redo', placement: 'top' }}>
      <Redo2 size={13} />
    </button>
  {/if}
</footer>

<style>
  .status {
    display: flex;
    align-items: center;
    gap: 10px;
    height: var(--statusbar-h);
    padding: 0 12px;
    font-size: var(--fs-xs);
    color: var(--ink-3);
    border-top: 1px solid var(--line);
    flex-shrink: 0;
  }
  .grow {
    flex: 1;
  }
  .item,
  .chord {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .chord {
    color: var(--primary-strong);
    font-weight: var(--fw-medium);
  }
  .warn {
    color: var(--warn);
  }
  button:disabled {
    opacity: 0.35;
  }
</style>
