<script lang="ts">
  import { Loader } from '@lucide/svelte';
  import { boards, openBoard } from '$lib/state/boards.svelte';
  import { syncWorkspaceCtx } from '$lib/state/workspace.svelte';
  import BoardView from '$lib/board/BoardView.svelte';
  import FilesHome from '$lib/files/FilesHome.svelte';
  import { t } from '$lib/i18n/index.svelte';

  let { boardId }: { boardId: string } = $props();
  const board = $derived(boards.get(boardId));
  let error = $state('');

  $effect(() => {
    if (!boards.get(boardId))
      openBoard({ id: boardId })
        .then(syncWorkspaceCtx)
        .catch((e) => (error = String(e?.message ?? e)));
    else syncWorkspaceCtx();
  });
</script>

{#if board}
  {#if board.kind === 'files'}
    <FilesHome {board} />
  {:else}
    <BoardView {board} />
  {/if}
{:else if error}
  <div class="empty"><strong>{t('errors.openBoardTitle')}</strong><span>{error}</span></div>
{:else}
  <div class="empty"><Loader size={18} class="spin" /></div>
{/if}
