<script lang="ts">
  // Card-face strip for remote-linked cards: status, type, priority, assignee.
  import { TriangleAlert } from '@lucide/svelte';
  import type { RemoteInfo } from '$lib/backend/types';
  import { t } from '$lib/i18n/index.svelte';
  import { tip } from '$lib/components/tooltip';

  let { remote, compact = false }: { boardId: string; id: string; remote?: RemoteInfo; compact?: boolean } = $props();

  const initials = (n: string) =>
    n
      .split(/\s+/)
      .filter(Boolean)
      .slice(0, 2)
      .map((w) => w[0]?.toUpperCase())
      .join('');
</script>

{#if remote}
  <span class="strip" class:compact>
    {#if remote.unavailable}
      <span class="chip warn" use:tip={t('integrations.strip.unavailableTip')}><TriangleAlert size={11} /> {t('integrations.strip.unavailable')}</span>
    {:else if remote.status}
      <span class="chip status {remote.statusCategory ?? 'todo'}" use:tip={t('integrations.strip.status')}>{remote.status}</span>
    {/if}
    {#if !compact && remote.type}<span class="meta">{remote.type}</span>{/if}
    {#if !compact && remote.priority}<span class="meta" use:tip={t('integrations.strip.priority')}>{remote.priority}</span>{/if}
    {#if remote.assignee}
      <span class="avatar" use:tip={remote.assignee.name}>{initials(remote.assignee.name)}</span>
    {/if}
  </span>
{/if}

<style>
  /* Alignment is owned by the card footer (external group, right-aligned). */
  .strip {
    display: inline-flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }
  .status.todo {
    background: var(--bg-hover);
    color: var(--ink-2);
  }
  .status.inProgress {
    background: var(--info-soft);
    color: var(--info);
  }
  .status.done {
    background: var(--ok-soft);
    color: var(--ok);
  }
  .chip.warn {
    background: var(--warn-soft);
    color: var(--warn);
  }
  .meta {
    font-size: var(--fs-xs);
    color: var(--ink-3);
    white-space: nowrap;
  }
  .avatar {
    display: inline-grid;
    place-items: center;
    width: 20px;
    height: 20px;
    border-radius: 50%;
    background: var(--primary-soft);
    color: var(--primary-strong);
    font-size: 9px;
    font-weight: var(--fw-semibold);
  }
</style>
