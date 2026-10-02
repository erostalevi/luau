<script lang="ts">
  import { Search, X, ChevronDown, ChevronUp } from '@lucide/svelte';
  import { ui } from '$lib/state/ui.svelte';
  import { select } from '$lib/state/selection.svelte';
  import { boards } from '$lib/state/boards.svelte';
  import { compileFilter, FILTER_KEYS } from './filter';
  import QuerySuggest from '$lib/components/QuerySuggest.svelte';
  import { t } from '$lib/i18n/index.svelte';

  let { boardId, count }: { boardId: string; count: number } = $props();
  const fs = $derived(ui.filter[boardId] ?? { open: true, text: '' });
  let cursor = $state(-1);
  let input: HTMLInputElement | undefined = $state();

  function setText(v: string) {
    ui.filter[boardId] = { open: true, text: v };
    cursor = -1;
  }

  function matches(): string[] {
    const f = compileFilter(fs.text);
    const els = [...document.querySelectorAll<HTMLElement>(`[data-board-scroll] [data-card][data-board="${boardId}"]`)];
    const b = boards.get(boardId);
    return els
      .map((e) => e.dataset.card!)
      .filter((id) => {
        const n = b?.node(id);
        return n && !n.isGroup && f.test(n, b?.remote.get(id));
      });
  }

  function jump(dir: 1 | -1) {
    const m = matches();
    if (!m.length) return;
    cursor = (cursor + dir + m.length) % m.length;
    const id = m[cursor];
    select(boardId, id);
    document.querySelector(`[data-board-scroll] [data-card="${id}"]`)?.scrollIntoView({ block: 'nearest', inline: 'nearest', behavior: 'smooth' });
  }

  function close() {
    ui.filter[boardId] = { open: false, text: fs.text };
  }
</script>

<div class="board-filter">
  <Search size={15} class="muted" />
  <input
    bind:this={input}
    value={fs.text}
    placeholder={t('board.filterPlaceholder')}
    spellcheck="false"
    oninput={(e) => setText((e.currentTarget as HTMLInputElement).value)}
    onkeydown={(e) => {
      e.stopPropagation();
      if (e.key === 'Escape') close();
      if (e.key === 'Enter') jump(e.shiftKey ? -1 : 1);
    }}
  />
  <QuerySuggest {input} config={{ keys: FILTER_KEYS, tagStyle: 'hash' }} onapply={setText} />
  {#if fs.text}
    <span class="count">{t('board.matches', { count })}</span>
    <button class="icon-btn sm" onclick={() => jump(-1)} aria-label="prev"><ChevronUp size={14} /></button>
    <button class="icon-btn sm" onclick={() => jump(1)} aria-label="next"><ChevronDown size={14} /></button>
  {/if}
  <button class="icon-btn sm" onclick={close} aria-label={t('common.close')}><X size={14} /></button>
</div>

<style>
  .board-filter {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 20px 10px 24px;
    padding: 0 8px 0 12px;
    height: 38px;
    border-radius: var(--r-md);
    background: var(--bg-elev);
    box-shadow:
      var(--shadow-1),
      0 0 0 1px var(--line);
    animation: pop-in var(--dur) var(--ease-out);
  }
  input {
    flex: 1;
    border: none;
    outline: none;
    background: transparent;
    font-size: var(--fs-md);
  }
  .count {
    font-size: var(--fs-xs);
    color: var(--ink-3);
    white-space: nowrap;
  }
</style>
