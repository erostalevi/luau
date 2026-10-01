<script lang="ts">
  // Archived lanes and cards of one board, with (undoable) unarchive.
  import { Archive, ArchiveRestore, Columns3, ExternalLink } from '@lucide/svelte';
  import { openBoard, boards, type BoardModel } from '$lib/state/boards.svelte';
  import { openCard } from '$lib/app/open';
  import { tip } from '$lib/components/tooltip';
  import { t } from '$lib/i18n/index.svelte';
  import { hist } from './historyState.svelte';
  import { knownBoards, unarchive } from './actions';

  const boardsList = $derived(knownBoards());
  const boardId = $derived(hist.board || boardsList[0]?.id || '');
  let failed = $state(false);

  $effect(() => {
    const id = boardId;
    failed = false;
    if (id && !boards.get(id)) openBoard({ id }).catch(() => (failed = true));
  });

  const model = $derived<BoardModel | undefined>(boardId ? boards.get(boardId) : undefined);
  const lanes = $derived(model ? model.lanes.filter((l) => l.archived) : []);
  const cards = $derived.by(() => {
    if (!model) return [];
    return [...model.nodes.values()]
      .filter((n) => n.archived)
      .map((n) => ({ id: n.id, title: n.title, lane: model.laneOf(n.id) }))
      .sort((a, b) => a.title.localeCompare(b.title));
  });

  function laneName(id: string | null): string | null {
    return id ? (model?.lane(id)?.name ?? null) : null;
  }

  function all() {
    void unarchive(
      boardId,
      cards.map((c) => c.id),
      lanes.map((l) => l.id),
    );
  }
</script>

<div class="selects">
  <select class="field slim" value={boardId} onchange={(ev) => (hist.board = (ev.currentTarget as HTMLSelectElement).value)} aria-label={t('history.board')}>
    {#each boardsList as b (b.id)}<option value={b.id}>{b.name}</option>{/each}
  </select>
  <button class="btn ghost sm" disabled={!cards.length && !lanes.length} onclick={all}><ArchiveRestore size={14} />{t('history.archive.unarchiveAll')}</button>
</div>

<div class="list">
  {#if !boardId || failed}
    <div class="empty">{t('history.archive.boardOnly')}</div>
  {:else if !cards.length && !lanes.length}
    <div class="empty">
      <div class="empty-icon"><Archive size={20} strokeWidth={1.8} /></div>
      <strong>{t('history.archive.empty')}</strong>
      <span class="hint">{t('history.archive.emptyHint')}</span>
    </div>
  {:else}
    <p class="intro">{t('history.archive.intro')}</p>
  {/if}

  {#if lanes.length}
    <div class="section-title head">{t('history.archive.lanes')}</div>
    {#each lanes as l (l.id)}
      <div class="row-item">
        <span class="ic"><Columns3 size={13} strokeWidth={1.9} /></span>
        <span class="body">
          <span class="text">{l.name}</span>
          <span class="meta">{t('history.trash.cards', { count: l.order.length })}</span>
        </span>
        <button
          class="icon-btn sm"
          onclick={() => void unarchive(boardId, [], [l.id])}
          use:tip={t('history.archive.unarchive')}
          aria-label={t('history.archive.unarchive')}><ArchiveRestore size={14} /></button
        >
      </div>
    {/each}
  {/if}

  {#if cards.length}
    <div class="section-title head">{t('history.archive.cards')}</div>
    {#each cards as c (c.id)}
      <div class="row-item">
        <span class="ic"><Archive size={13} strokeWidth={1.9} /></span>
        <span class="body">
          <span class="text">{c.title || t('common.untitled')}</span>
          {#if laneName(c.lane)}<span class="meta">{t('history.archive.inLane', { lane: laneName(c.lane) ?? '' })}</span>{/if}
        </span>
        <span class="btns">
          <button
            class="icon-btn sm"
            onclick={() => void openCard(boardId, c.id)}
            use:tip={t('history.actions.openCard')}
            aria-label={t('history.actions.openCard')}><ExternalLink size={14} /></button
          >
          <button
            class="icon-btn sm"
            onclick={() => void unarchive(boardId, [c.id], [])}
            use:tip={t('history.archive.unarchive')}
            aria-label={t('history.archive.unarchive')}><ArchiveRestore size={14} /></button
          >
        </span>
      </div>
    {/each}
  {/if}
</div>

<style>
  .selects {
    display: flex;
    gap: 6px;
    padding: 0 10px 8px;
  }
  .field.slim {
    flex: 1;
    min-width: 0;
    height: 28px;
    padding-top: 0;
    padding-bottom: 0;
    font-size: var(--fs-sm);
  }
  .list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 0 6px 16px;
  }
  .intro {
    margin: 2px 8px 8px;
    font-size: var(--fs-xs);
    color: var(--ink-3);
  }
  .head {
    padding: 10px 8px 4px;
  }
  .row-item {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 7px 8px;
    border-radius: var(--r-sm);
  }
  .row-item:hover,
  .row-item:focus-within {
    background: var(--bg-hover);
  }
  .ic {
    flex-shrink: 0;
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    margin-top: 1px;
    border-radius: 7px;
    background: var(--warn-soft);
    color: var(--warn);
  }
  .body {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .text {
    font-size: var(--fs-md);
    color: var(--ink);
    overflow-wrap: anywhere;
  }
  .meta {
    font-size: var(--fs-xs);
    color: var(--ink-3);
  }
  .btns {
    display: flex;
    gap: 2px;
  }
  .hint {
    font-size: var(--fs-sm);
    max-width: 220px;
  }
</style>
