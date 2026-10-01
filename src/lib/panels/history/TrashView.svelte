<script lang="ts">
  // Per-board trash: days left before cleanup, restore (undoable), delete forever, empty, TTL.
  import { Trash2, RotateCcw, Flame, Columns3, Layers, Eraser } from '@lucide/svelte';
  import type { TrashEntry } from '$lib/backend/types';
  import { onCoreEvent } from '$lib/backend/rpc';
  import { tip } from '$lib/components/tooltip';
  import { settings } from '$lib/settings/store.svelte';
  import { t, relTime } from '$lib/i18n/index.svelte';
  import { hist } from './historyState.svelte';
  import { knownBoards, listTrash, restoreFromTrash, deleteForever, emptyTrash, cleanupTrash, trashTtl } from './actions';
  import { daysLeft } from './model';

  const TTLS = [1, 3, 7, 14, 30, 60, 90];

  let entries = $state<TrashEntry[]>([]);
  let loading = $state(false);
  let seq = 0;

  const boardsList = $derived(knownBoards());
  const board = $derived(hist.board || boardsList[0]?.id || '');
  const ttl = $derived(trashTtl());
  const ttlOptions = $derived([...new Set([...TTLS, ttl])].sort((a, b) => a - b));

  async function load(id: string) {
    const my = ++seq;
    if (!id) {
      entries = [];
      return;
    }
    loading = true;
    try {
      const list = await listTrash(id);
      if (my === seq) entries = [...list].sort((a, b) => Date.parse(b.deletedAt) - Date.parse(a.deletedAt));
    } catch {
      if (my === seq) entries = [];
    } finally {
      if (my === seq) loading = false;
    }
  }

  $effect(() => {
    void hist.refresh;
    void load(board);
  });

  $effect(() =>
    onCoreEvent((e) => {
      if (e.type === 'boardDelta' && e.delta.boardId === board) void load(board);
    }),
  );

  function setTtl(v: string) {
    const n = Math.floor(Number(v));
    if (Number.isFinite(n) && n >= 1 && n <= 365) settings.set('trash.ttlDays', n);
  }

  function left(e: TrashEntry): string {
    const d = daysLeft(e.deletedAt, ttl);
    return d === 0 ? t('history.trash.lastDay') : t('history.trash.daysLeft', { count: d });
  }
</script>

<div class="selects">
  <select class="field slim" value={board} onchange={(ev) => (hist.board = (ev.currentTarget as HTMLSelectElement).value)} aria-label={t('history.board')}>
    {#each boardsList as b (b.id)}<option value={b.id}>{b.name}</option>{/each}
  </select>
  <label class="ttl" use:tip={t('settings.keys.trash.ttlDays.desc')}>
    <span>{t('history.trash.keepFor')}</span>
    <select class="field slim" value={String(ttl)} onchange={(ev) => setTtl((ev.currentTarget as HTMLSelectElement).value)}>
      {#each ttlOptions as d (d)}<option value={String(d)}>{t('history.trash.days', { count: d })}</option>{/each}
    </select>
  </label>
</div>

<div class="actions">
  <button class="btn ghost sm" disabled={!board} onclick={() => void cleanupTrash([board])}><Eraser size={14} />{t('history.trash.cleanup')}</button>
  <button class="btn ghost sm danger" disabled={!entries.length} onclick={() => void emptyTrash(board)}><Flame size={14} />{t('history.trash.emptyTrash')}</button>
</div>

<div class="list">
  {#if !entries.length && !loading}
    <div class="empty">
      <div class="empty-icon"><Trash2 size={20} strokeWidth={1.8} /></div>
      <strong>{t('history.trash.empty')}</strong>
      <span class="hint">{t('history.trash.emptyHint', { days: ttl })}</span>
    </div>
  {:else}
    <p class="intro">{t('history.trash.intro')}</p>
  {/if}
  {#each entries as e (e.id)}
    {@const soon = daysLeft(e.deletedAt, ttl) <= 1}
    <div class="row-item">
      <span class="ic">
        {#if e.kind === 'lane'}<Columns3 size={13} strokeWidth={1.9} />{:else if e.isGroup}<Layers size={13} strokeWidth={1.9} />{:else}<Trash2 size={13} strokeWidth={1.9} />{/if}
      </span>
      <span class="body">
        <span class="text">{e.title || t('common.untitled')}</span>
        <span class="meta">
          {#if e.kind === 'lane'}<span class="chip">{t('history.trash.lane')}</span>{:else if e.isGroup}<span class="chip">{t('history.trash.group')}</span>{/if}
          {#if e.count > 1}<span>{t('history.trash.cards', { count: e.count })}</span><span class="dot">·</span>{/if}
          <span>{t('history.trash.deleted', { when: relTime(e.deletedAt) })}</span>
          <span class="dot">·</span>
          <span class:soon>{left(e)}</span>
        </span>
      </span>
      <span class="btns">
        <button class="icon-btn sm" onclick={() => void restoreFromTrash(board, [e])} use:tip={t('history.trash.restore')} aria-label={t('history.trash.restore')}><RotateCcw size={14} /></button>
        <button class="icon-btn sm danger" onclick={() => void deleteForever(board, e)} use:tip={t('history.trash.deleteForever')} aria-label={t('history.trash.deleteForever')}><Flame size={14} /></button>
      </span>
    </div>
  {/each}
</div>

<style>
  .selects {
    display: flex;
    gap: 6px;
    padding: 0 10px 6px;
  }
  .field.slim {
    flex: 1;
    min-width: 0;
    height: 28px;
    padding-top: 0;
    padding-bottom: 0;
    font-size: var(--fs-sm);
  }
  .ttl {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: 1;
    min-width: 0;
    font-size: var(--fs-xs);
    color: var(--ink-3);
    white-space: nowrap;
  }
  .actions {
    display: flex;
    gap: 4px;
    padding: 0 6px 6px;
  }
  .btn.danger {
    color: var(--danger);
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
    background: var(--danger-soft);
    color: var(--danger);
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
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 5px;
    font-size: var(--fs-xs);
    color: var(--ink-3);
  }
  .meta .chip {
    height: 16px;
    line-height: 16px;
    padding: 0 6px;
    font-size: 10.5px;
  }
  .soon {
    color: var(--warn);
  }
  .dot {
    color: var(--ink-4);
  }
  .btns {
    display: flex;
    gap: 2px;
    opacity: 0.55;
    transition: opacity var(--dur-fast) var(--ease);
  }
  .row-item:hover .btns,
  .row-item:focus-within .btns {
    opacity: 1;
  }
  .icon-btn.danger:hover {
    color: var(--danger);
  }
  .hint {
    font-size: var(--fs-sm);
    max-width: 220px;
  }
</style>
