<script lang="ts">
  import type { NodeDto } from '$lib/backend/types';
  import { settings } from '$lib/settings/store.svelte';
  import { toggleTask } from './cardActions';
  import InlineMd from './InlineMd.svelte';

  let { node, boardId }: { node: NodeDto; boardId: string } = $props();

  const max = $derived(settings.get<number>('board.face.maxItems'));
  const showPreview = $derived(settings.get<boolean>('board.face.preview'));
  const showSummary = $derived(settings.get<boolean>('board.face.summary'));
  const face = $derived(node.face);
</script>

{#if face.kind === 'checklist' && showPreview}
  <ul class="list tasks">
    {#each face.items.slice(0, max) as it (it.line)}
      <li class:done={it.task}>
        {#if it.task !== null}
          <button
            class="box"
            class:on={it.task}
            aria-label="toggle"
            onpointerdown={(e) => e.stopPropagation()}
            onclick={(e) => {
              e.stopPropagation();
              void toggleTask(boardId, node.id, it.line);
            }}
          ></button>
        {:else}<span class="bullet"></span>{/if}
        <span class="txt"><InlineMd text={it.text} /></span>
      </li>
    {/each}
    {#if face.total > max}<li class="more">+{face.total - max}</li>{/if}
  </ul>
{:else if face.kind === 'list' && showPreview}
  <ul class="list" class:ordered={face.ordered}>
    {#each face.items.slice(0, max) as it, i (it.line)}
      <li>
        {#if face.ordered}<span class="num">{i + 1}.</span>{:else}<span class="bullet"></span>{/if}
        <span class="txt"><InlineMd text={it.text} /></span>
      </li>
    {/each}
    {#if face.total > max}<li class="more">+{face.total - max}</li>{/if}
  </ul>
{:else if face.kind === 'table' && showPreview}
  <div class="table-wrap">
    <table>
      <thead
        ><tr
          >{#each face.header as h, i (i)}<th>{h}</th>{/each}</tr
        ></thead
      >
      <tbody>
        {#each face.rows.slice(0, Math.min(max, 4)) as row, r (r)}
          <tr
            >{#each row as c, i (i)}<td>{c}</td>{/each}</tr
          >
        {/each}
      </tbody>
    </table>
    {#if face.total > Math.min(max, 4)}<div class="more">+{face.total - Math.min(max, 4)}</div>{/if}
  </div>
{:else if face.kind === 'summary' && showSummary}
  <p class="summary"><InlineMd text={face.text} /></p>
{/if}

<style>
  .list {
    list-style: none;
    margin: 6px 0 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
    font-size: var(--fs-sm);
    color: var(--ink-2);
  }
  li {
    display: flex;
    align-items: flex-start;
    gap: 7px;
    line-height: 1.45;
  }
  li.done .txt {
    color: var(--ink-4);
    text-decoration: line-through;
    text-decoration-color: var(--ink-4);
  }
  .txt {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
  }
  .box {
    width: 14px;
    height: 14px;
    margin-top: 2px;
    padding: 0;
    flex-shrink: 0;
    border: none;
    border-radius: 4.5px;
    box-shadow: inset 0 0 0 1.5px var(--line-strong);
    background: transparent;
    transition:
      background var(--dur-fast) var(--ease),
      box-shadow var(--dur-fast) var(--ease),
      transform var(--dur-fast) var(--ease-spring);
  }
  .box:hover {
    box-shadow: inset 0 0 0 1.5px var(--primary);
  }
  .box:active {
    transform: scale(0.85);
  }
  .box.on {
    background: var(--primary)
      url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='white' stroke-width='3.2' stroke-linecap='round' stroke-linejoin='round'%3E%3Cpath d='M20 6 9 17l-5-5'/%3E%3C/svg%3E")
      center / 10px no-repeat;
    box-shadow: none;
  }
  .bullet {
    width: 5px;
    height: 5px;
    margin: 7px 4px 0 4px;
    border-radius: 50%;
    background: var(--ink-4);
    flex-shrink: 0;
  }
  .num {
    color: var(--ink-4);
    font-variant-numeric: tabular-nums;
    min-width: 14px;
  }
  .more {
    color: var(--ink-4);
    font-size: var(--fs-xs);
    padding-left: 21px;
  }
  .summary {
    margin: 6px 0 0;
    font-size: var(--fs-sm);
    color: var(--ink-3);
    line-height: 1.5;
    display: -webkit-box;
    -webkit-line-clamp: 3;
    line-clamp: 3;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .table-wrap {
    margin-top: 6px;
    border-radius: var(--r-xs);
    overflow: hidden;
    box-shadow: inset 0 0 0 1px var(--line);
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: var(--fs-xs);
    table-layout: fixed;
  }
  th,
  td {
    padding: 3px 7px;
    text-align: left;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    border-bottom: 1px solid var(--line);
  }
  th {
    background: var(--bg-hover);
    color: var(--ink-2);
    font-weight: var(--fw-semibold);
  }
  td {
    color: var(--ink-2);
  }
  tr:last-child td {
    border-bottom: none;
  }
  .table-wrap .more {
    padding: 2px 7px;
  }
</style>
