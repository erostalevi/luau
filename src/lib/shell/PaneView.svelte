<script lang="ts">
  import type { Pane } from '$lib/state/workspace.svelte';
  import { ws } from '$lib/state/workspace.svelte';
  import BoardTab from '$lib/views/BoardTab.svelte';
  import DocumentTab from '$lib/views/DocumentTab.svelte';
  import StartPage from '$lib/views/StartPage.svelte';
  import SettingsView from '$lib/views/settings/SettingsView.svelte';
  import KeybindingsView from '$lib/views/settings/KeybindingsView.svelte';
  import VirtualBoard from '$lib/views/VirtualBoard.svelte';
  import SummaryView from '$lib/views/SummaryView.svelte';

  let { pane }: { pane: Pane } = $props();
  const tab = $derived(pane.tabs.find((x) => x.id === pane.active) ?? null);
</script>

<div class="pane-view" role="presentation" onpointerdown={() => (ws.activePane = pane.id)}>
  {#if !tab}
    <StartPage />
  {:else}
    {#key tab.id}
      {#if tab.kind === 'board'}
        <BoardTab boardId={tab.boardId!} />
      {:else if tab.kind === 'doc'}
        <DocumentTab boardId={tab.boardId!} cardId={tab.cardId!} tabId={tab.id} />
      {:else if tab.kind === 'start'}
        <StartPage />
      {:else if tab.kind === 'settings'}
        <SettingsView initial={tab.payload?.query as string | undefined} />
      {:else if tab.kind === 'keybindings'}
        <KeybindingsView />
      {:else if tab.kind === 'savedSearch'}
        <VirtualBoard {tab} />
      {:else if tab.kind === 'summary'}
        <SummaryView {tab} />
      {/if}
    {/key}
  {/if}
</div>

<style>
  .pane-view {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    position: relative;
  }
</style>
