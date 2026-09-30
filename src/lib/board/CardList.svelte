<script lang="ts">
  import { flip } from 'svelte/animate';
  import type { Parent } from '$lib/backend/types';
  import { boards } from '$lib/state/boards.svelte';
  import { ui } from '$lib/state/ui.svelte';
  import { boardUi, listKey } from './boardUi.svelte';
  import { dnd } from './dnd.svelte';
  import Card from './Card.svelte';
  import QuickAdd from './QuickAdd.svelte';

  let {
    boardId,
    parent,
    ids,
    depth = 0,
    flow = 'y',
    wrap = false,
    empty,
  }: { boardId: string; parent: Parent; ids: string[]; depth?: number; flow?: 'x' | 'y'; wrap?: boolean; empty?: import('svelte').Snippet } = $props();

  const model = $derived(boards.get(boardId));
  const key = $derived(listKey(boardId, parent));
  // Archived cards keep their relative order but sit at the end when shown.
  const visible = $derived.by(() => {
    if (!model) return [];
    const live: string[] = [];
    const archived: string[] = [];
    for (const id of ids) {
      const n = model.nodes.get(id);
      if (!n) continue;
      if (n.archived) archived.push(id);
      else live.push(id);
    }
    return ui.showArchived ? [...live, ...archived] : live;
  });
  const qa = $derived(boardUi.quickAdd?.key === key ? boardUi.quickAdd : null);
  const qaIndex = $derived(qa ? (qa.before && visible.includes(qa.before) ? visible.indexOf(qa.before) : visible.length) : -1);
  const QA = '\u0000qa';
  const items = $derived(qa ? [...visible.slice(0, qaIndex), QA, ...visible.slice(qaIndex)] : visible);
  const pk = $derived(parent.kind);
  const pid = $derived('id' in parent ? parent.id : '');
</script>

<div
  class="list flow-{flow}"
  class:wrap
  class:nested={depth > 0}
  class:empty={!visible.length && !qa}
  class:dragging={dnd.active}
  data-list
  data-board={boardId}
  data-parent-kind={pk}
  data-parent-id={pid}
  data-flow={flow}
  style:--depth={Math.min(depth, 4)}
>
  {#each items as id (id)}
    <div class="flip-item" animate:flip={{ duration: dnd.active ? 160 : 220 }}>
      {#if id === QA}<QuickAdd target={qa!} />{:else}<Card {boardId} {id} {depth} {flow} />{/if}
    </div>
  {/each}
  {#if !visible.length && !qa}{@render empty?.()}{/if}
</div>

<style>
  .list {
    display: flex;
    gap: 8px;
    min-height: 8px;
    border-radius: var(--r-md);
    transition: background var(--dur-fast) var(--ease);
  }
  .flow-y {
    flex-direction: column;
  }
  .flow-x {
    flex-direction: row;
    align-items: flex-start;
  }
  .flow-x.wrap {
    flex-wrap: wrap;
  }
  .flow-y.wrap {
    flex-wrap: wrap;
    align-content: flex-start;
    max-height: 100%;
  }
  .flow-x > .flip-item,
  .wrap > .flip-item {
    width: var(--card-w);
    flex-shrink: 0;
  }
  .empty {
    min-height: 44px;
  }
  .dragging.empty {
    min-height: 64px;
    outline: 1.5px dashed var(--line-strong);
    outline-offset: -4px;
  }
  .nested {
    gap: 6px;
  }
</style>
