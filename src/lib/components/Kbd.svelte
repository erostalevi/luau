<script lang="ts">
  import { formatKey } from '$lib/keybindings/keys';
  import { primaryKey } from '$lib/keybindings/resolver.svelte';

  let { keys, command }: { keys?: string | null; command?: string } = $props();
  const resolved = $derived(keys ?? (command ? primaryKey(command) : null));
  const strokes = $derived(resolved ? formatKey(resolved) : []);
</script>

{#if strokes.length}
  <span class="kbd">
    {#each strokes as s, i (i)}
      <span>{s}</span>
    {/each}
  </span>
{/if}
