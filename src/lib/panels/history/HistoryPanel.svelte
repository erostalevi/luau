<script lang="ts">
  import { History, Trash2, Archive } from '@lucide/svelte';
  import Segmented from '$lib/components/Segmented.svelte';
  import { t } from '$lib/i18n/index.svelte';
  import { hist, closeDiff, type HistoryView } from './historyState.svelte';
  import Timeline from './Timeline.svelte';
  import DiffView from './DiffView.svelte';
  import TrashView from './TrashView.svelte';
  import ArchiveView from './ArchiveView.svelte';

  const views = $derived([
    { value: 'timeline' as HistoryView, icon: History, label: t('history.views.timeline') },
    { value: 'trash' as HistoryView, icon: Trash2, label: t('history.views.trash') },
    { value: 'archive' as HistoryView, icon: Archive, label: t('history.views.archive') },
  ]);
</script>

<div class="history">
  {#if hist.diff}
    <DiffView />
  {:else}
    <div class="views">
      <Segmented size="sm" full options={views} bind:value={hist.view} onchange={closeDiff} />
    </div>
    {#if hist.view === 'trash'}
      <TrashView />
    {:else if hist.view === 'archive'}
      <ArchiveView />
    {:else}
      <Timeline />
    {/if}
  {/if}
</div>

<style>
  .history {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }
  .views {
    padding: 0 10px 8px;
  }
</style>
