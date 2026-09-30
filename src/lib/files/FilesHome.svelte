<script lang="ts">
  import type { BoardModel } from '$lib/state/boards.svelte';
  let { board }: { board: BoardModel } = $props();
</script>
<div class="empty">{board.header.name}</div>
