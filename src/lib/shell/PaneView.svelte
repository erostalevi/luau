<script lang="ts">
  import type { Pane } from '$lib/state/workspace.svelte';
  import { ws } from '$lib/state/workspace.svelte';
  import BoardTab from '$lib/views/BoardTab.svelte';
  import DocumentTab from '$lib/views/DocumentTab.svelte';
  import StartPage from '$lib/views/StartPage.svelte';
  // Views that are not needed at startup load on first use (smaller bundle).
  const SettingsView = () => import('$lib/views/settings/SettingsView.svelte');
  const KeybindingsView = () => import('$lib/views/settings/KeybindingsView.svelte');
  const VirtualBoard = () => import('$lib/views/VirtualBoard.svelte');
  const SummaryView = () => import('$lib/views/SummaryView.svelte');

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
        {#await SettingsView() then m}<m.default initial={tab.payload?.query as string | undefined} />{/await}
      {:else if tab.kind === 'keybindings'}
        {#await KeybindingsView() then m}<m.default />{/await}
      {:else if tab.kind === 'savedSearch'}
        {#await VirtualBoard() then m}<m.default {tab} />{/await}
      {:else if tab.kind === 'summary'}
        {#await SummaryView() then m}<m.default {tab} />{/await}
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
