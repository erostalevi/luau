<script lang="ts">
  // Version comparison for one journal entry: "this change" (before → after) or
  // "compared with now" (that version → current card text). Word-level, markdown-aware.
  import { ArrowLeft, ChevronUp, ChevronDown, Copy, RotateCcw, ExternalLink } from '@lucide/svelte';
  import { rpc } from '$lib/backend/rpc';
  import Segmented from '$lib/components/Segmented.svelte';
  import { tip } from '$lib/components/tooltip';
  import { openCard } from '$lib/app/open';
  import { copyText } from '$lib/app/helpers';
  import { toast } from '$lib/state/toasts.svelte';
  import { settings } from '$lib/settings/store.svelte';
  import { t, fmtDate, relTime } from '$lib/i18n/index.svelte';
  import { hist, closeDiff, openDiff } from './historyState.svelte';
  import { loadBlob, restoreVersion, boardName, fmtDateTime } from './actions';
  import { entryCardId, describeEntry } from './model';
  import { diffLines, diffStats, enrich, sideBySide, collapse, unifiedText, type RichLine } from './diff';

  type Mode = 'change' | 'current';
  type Layout = 'inline' | 'split';

  const req = $derived(hist.diff!);
  const e = $derived(req.entry);
  const card = $derived(entryCardId(e));
  let mode = $state<Mode>('change');
  let layout = $state<Layout>(settings.get<string>('history.diffLayout') === 'split' ? 'split' : 'inline');

  let before = $state<string | null>(null);
  let after = $state<string | null>(null);
  let current = $state<string | null>(null);
  let failed = $state(false);
  let loading = $state(true);
  let seq = 0;

  async function load() {
    const my = ++seq;
    loading = true;
    failed = false;
    const [b, a, c] = await Promise.all([
      loadBlob(e.board, e.before),
      loadBlob(e.board, e.after),
      card ? rpc<string>('card.read', { board: e.board, id: card }).catch(() => null) : Promise.resolve(null),
    ]);
    if (my !== seq) return;
    before = b;
    after = a;
    current = c;
    failed = (e.before && b === null) || (e.after && a === null) ? true : false;
    loading = false;
  }

  $effect(() => {
    void e;
    void hist.refresh;
    void load();
  });

  /** The version this entry produced (or, for a deletion-like entry, the one before it). */
  const version = $derived(e.after ? after : before);
  const left = $derived(mode === 'change' ? (before ?? '') : (version ?? ''));
  const right = $derived(mode === 'change' ? (after ?? '') : (current ?? ''));
  const lines = $derived(enrich(diffLines(left, right), left, right));
  const stats = $derived(diffStats(lines));
  const changed = (l: RichLine) => l.kind !== 'eq';
  const inlineRows = $derived(collapse(lines, changed));
  const splitRows = $derived(collapse(sideBySide(lines), (r) => !!(r.left && changed(r.left)) || !!(r.right && changed(r.right))));
  const identical = $derived(!loading && stats.added === 0 && stats.removed === 0);

  const idx = $derived(req.siblings.indexOf(e));
  const newer = $derived(idx > 0 ? req.siblings[idx - 1] : null);
  const older = $derived(idx >= 0 && idx < req.siblings.length - 1 ? req.siblings[idx + 1] : null);

  function title(): string {
    const d = describeEntry(e, boardName);
    return t(`history.desc.${d.key}`, { ...d.params, title: d.params.title || t('common.untitled') });
  }

  function setLayout(v: Layout) {
    layout = v;
    settings.set('history.diffLayout', v);
  }

  async function restore(text: string | null) {
    if (!card || text === null) return;
    if (await restoreVersion(e.board, card, text, e.ts)) closeDiff();
  }

  async function copy() {
    await copyText(unifiedText(lines));
    toast.info(t('history.diff.copied'), { timeout: 1400 });
  }

  function keys(ev: KeyboardEvent) {
    if (ev.key === 'Escape') {
      ev.preventDefault();
      closeDiff();
    } else if (ev.altKey && ev.key === 'ArrowUp' && newer) openDiff(newer, req.siblings);
    else if (ev.altKey && ev.key === 'ArrowDown' && older) openDiff(older, req.siblings);
  }

  let root: HTMLElement;
  $effect(() => root?.focus());
</script>

{#snippet segs(l: RichLine)}
  {#if l.segs}
    {#each l.segs as s, i (i)}<span class:hl={s.kind !== 'eq'}>{s.text}</span>{/each}
  {:else}{l.text || ' '}{/if}
{/snippet}

<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<section class="diff" tabindex="0" bind:this={root} onkeydown={keys} aria-label={t('history.diff.title')}>
  <header class="head">
    <button class="icon-btn" onclick={closeDiff} use:tip={t('common.back')} aria-label={t('common.back')}><ArrowLeft size={15} strokeWidth={1.8} /></button>
    <div class="titles">
      <strong class="ellipsis">{title()}</strong>
      <span class="sub">{fmtDateTime(e.ts)} · {relTime(e.ts)}</span>
    </div>
    {#if req.siblings.length > 1}
      <button class="icon-btn sm" disabled={!newer} onclick={() => newer && openDiff(newer, req.siblings)} use:tip={t('history.diff.next')} aria-label={t('history.diff.next')}><ChevronUp size={15} /></button>
      <button class="icon-btn sm" disabled={!older} onclick={() => older && openDiff(older, req.siblings)} use:tip={t('history.diff.prev')} aria-label={t('history.diff.prev')}><ChevronDown size={15} /></button>
    {/if}
  </header>

  <div class="controls">
    <Segmented
      size="sm"
      bind:value={mode}
      options={[
        { value: 'change', label: t('history.diff.mode.change') },
        { value: 'current', label: t('history.diff.mode.current') },
      ]}
    />
    <Segmented
      size="sm"
      value={layout}
      onchange={setLayout}
      options={[
        { value: 'inline', label: t('history.diff.layout.inline') },
        { value: 'split', label: t('history.diff.layout.split') },
      ]}
    />
  </div>

  <div class="body">
    {#if loading}
      <div class="empty">{t('common.loading')}</div>
    {:else if failed}
      <div class="empty">{t('history.diff.loadFailed')}</div>
    {:else if !e.before && !e.after}
      <div class="empty">{t('history.diff.noText')}</div>
    {:else if identical}
      <div class="empty">{mode === 'current' ? t('history.diff.identical') : t('history.diff.noText')}</div>
    {:else}
      <div class="stats">
        <span class="add">+{stats.added}</span><span class="del">−{stats.removed}</span>
        <span class="legend">{mode === 'change' ? `${t('history.diff.before')} → ${t('history.diff.after')}` : `${t('history.diff.version', { date: fmtDate(e.ts) })} → ${t('history.diff.current')}`}</span>
      </div>
      {#if layout === 'inline'}
        <div class="code">
          {#each inlineRows as r (r.key)}
            {#if r.type === 'gap'}
              <div class="gap">{t('history.diff.unchanged', { count: r.count })}</div>
            {:else}
              <div class="ln {r.line.kind} md-{r.line.md.kind}">
                <span class="no">{r.line.kind === 'add' ? '' : r.line.a}</span>
                <span class="no">{r.line.kind === 'del' ? '' : r.line.b}</span>
                <span class="mk">{r.line.kind === 'add' ? '+' : r.line.kind === 'del' ? '−' : ''}</span>
                <span class="tx">{@render segs(r.line)}</span>
              </div>
            {/if}
          {/each}
        </div>
      {:else}
        <div class="code split">
          {#each splitRows as r (r.key)}
            {#if r.type === 'gap'}
              <div class="gap span">{t('history.diff.unchanged', { count: r.count })}</div>
            {:else}
              {#each [r.line.left, r.line.right] as side, si (si)}
                <div class="ln {side ? side.kind : 'none'} {side ? 'md-' + side.md.kind : ''}">
                  {#if side}<span class="tx">{@render segs(side)}</span>{/if}
                </div>
              {/each}
            {/if}
          {/each}
        </div>
      {/if}
    {/if}
  </div>

  <footer class="foot">
    {#if card}
      <button class="btn ghost sm" onclick={() => card && void openCard(e.board, card)}><ExternalLink size={14} />{t('history.actions.openCard')}</button>
    {/if}
    <button class="btn ghost sm" onclick={copy} disabled={identical || loading}><Copy size={14} />{t('history.diff.copy')}</button>
    <span class="grow"></span>
    {#if card && !failed && !loading}
      {#if e.before && e.after && mode === 'change'}
        <button class="btn soft sm" onclick={() => restore(before)}><RotateCcw size={14} />{t('history.diff.restoreBefore')}</button>
      {/if}
      {#if version !== null && version !== current}
        <button class="btn primary sm" onclick={() => restore(version)}><RotateCcw size={14} />{t('history.diff.restoreAfter')}</button>
      {/if}
    {/if}
  </footer>
</section>

<style>
  .diff {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    outline: none;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 10px 8px;
  }
  .titles {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .titles strong {
    font-size: var(--fs-md);
    color: var(--ink);
  }
  .sub {
    font-size: var(--fs-xs);
    color: var(--ink-3);
  }
  .ellipsis {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .controls {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    padding: 0 10px 8px;
  }
  .body {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 0 10px 10px;
  }
  .stats {
    display: flex;
    gap: 8px;
    align-items: baseline;
    font-size: var(--fs-xs);
    color: var(--ink-3);
    padding: 2px 2px 6px;
    font-variant-numeric: tabular-nums;
  }
  .stats .add {
    color: var(--ok);
  }
  .stats .del {
    color: var(--danger);
  }
  .legend {
    margin-left: auto;
  }
  .code {
    border-radius: var(--r-sm);
    border: 1px solid var(--line);
    background: var(--bg-raised, var(--bg));
    font-family: var(--font-mono, ui-monospace, monospace);
    font-size: 11.5px;
    line-height: 1.55;
    overflow: hidden;
  }
  .code.split {
    display: grid;
    grid-template-columns: 1fr 1fr;
  }
  .split .ln {
    grid-template-columns: 1fr;
    border-right: 1px solid var(--line);
  }
  .ln {
    display: grid;
    grid-template-columns: 2.4em 2.4em 1.2em 1fr;
    min-width: 0;
  }
  .no {
    color: var(--ink-4);
    text-align: right;
    padding-right: 4px;
    user-select: none;
  }
  .mk {
    color: var(--ink-3);
    user-select: none;
    text-align: center;
  }
  .tx {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    padding-right: 6px;
    color: var(--ink-2);
  }
  .split .tx {
    padding-left: 6px;
  }
  .ln.add {
    background: var(--ok-soft);
  }
  .ln.del {
    background: var(--danger-soft);
  }
  .ln.none {
    background: var(--bg-hover);
  }
  .ln.add .hl {
    background: color-mix(in srgb, var(--ok) 28%, transparent);
    border-radius: 3px;
  }
  .ln.del .hl {
    background: color-mix(in srgb, var(--danger) 26%, transparent);
    border-radius: 3px;
    text-decoration: line-through;
    text-decoration-color: color-mix(in srgb, var(--danger) 60%, transparent);
  }
  .md-heading .tx {
    font-weight: var(--fw-semibold, 600);
    color: var(--ink);
  }
  .md-taskDone .tx {
    color: var(--ink-3);
  }
  .md-footer .tx,
  .md-rule .tx {
    color: var(--ink-3);
    font-style: italic;
  }
  .md-quote .tx {
    font-style: italic;
  }
  .gap {
    padding: 2px 8px;
    font-size: var(--fs-xs);
    color: var(--ink-3);
    background: var(--bg-hover);
    font-family: var(--font-ui, inherit);
  }
  .gap.span {
    grid-column: 1 / -1;
  }
  .foot {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    padding: 8px 10px;
    border-top: 1px solid var(--line);
  }
  .grow {
    flex: 1;
  }
</style>
