<script lang="ts">
  // Full-page document (files boards). Tabs can hold documents of files boards.
  import { Loader } from '@lucide/svelte';
  import { boards, openBoard } from '$lib/state/boards.svelte';
  import { closeTab, syncWorkspaceCtx } from '$lib/state/workspace.svelte';
  import CardEditor from '$lib/editor/CardEditor.svelte';
  import { t } from '$lib/i18n/index.svelte';

  let { boardId, cardId, tabId }: { boardId: string; cardId: string; tabId: string } = $props();
  const board = $derived(boards.get(boardId));
  const node = $derived(board?.nodes.get(cardId));

  $effect(() => {
    if (!boards.get(boardId)) void openBoard({ id: boardId }).then(syncWorkspaceCtx);
    else syncWorkspaceCtx();
  });

  // Deleted document → close its tab.
  $effect(() => {
    if (board && !node && board.nodes.size) closeTab(tabId, true);
  });
</script>

{#if board && node}
  <div class="doc">
    <CardEditor {boardId} {cardId} variant="page" />
  </div>
{:else if board}
  <div class="empty">{t('links.missing')}</div>
{:else}
  <div class="empty"><Loader size={18} class="spin" /></div>
{/if}

<style>
  .doc {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
</style>
