<script lang="ts">
  import { onDestroy, onMount, untrack } from 'svelte';
  import { Sparkles, CalendarClock, Copy, FileDown, Trash2, Square, Bot, Circle, ChevronDown } from '@lucide/svelte';
  import type { Tab } from '$lib/state/workspace.svelte';
  import Segmented from '$lib/components/Segmented.svelte';
  import Toggle from '$lib/components/Toggle.svelte';
  import SchedulesEditor from '$lib/summaries/SchedulesEditor.svelte';
  import { ai, onAiEvent, requestId, type AiStatus, type Engine, type SavedMeta, type SummaryResult } from '$lib/summaries/api';
  import { RANGE_PRESETS, resolveRange, toIso, ymd, plainText, slackText, resolveLinks, type RangePreset } from '$lib/summaries/range';
  import { testConnection } from '$lib/summaries/summaries.commands';
  import { registry } from '$lib/state/registry.svelte';
  import { settings } from '$lib/settings/store.svelte';
  import { toast } from '$lib/state/toasts.svelte';
  import { confirm } from '$lib/state/dialogs.svelte';
  import { openMenu } from '$lib/state/menu.svelte';
  import { renderMarkdown } from '$lib/markdown/render';
  import { resolveTitle } from '$lib/links/titles.svelte';
  import { openCardById } from '$lib/app/open';
  import { copyText, pickSavePath, reveal } from '$lib/app/helpers';
  import { rpc } from '$lib/backend/rpc';
  import { t, i18n } from '$lib/i18n/index.svelte';

  let { tab }: { tab: Tab } = $props();

  type View = 'summary' | 'schedules';
  let view = $state<View>('summary');
  let preset = $state<RangePreset>('yesterday');
  const today = ymd(new Date());
  let customFrom = $state(today);
  let customTo = $state(today);
  let selected = $state<string[]>([]);
  let detail = $state(Number(settings.get('summaries.defaultDetail')) || 3);
  let prompt = $state('');
  let engine = $state<Engine>('auto');
  let includeRemote = $state(false);
  let showBoards = $state(false);

  let running = $state(false);
  let stage = $state('');
  let stream = $state('');
  let result = $state<SummaryResult | null>(null);
  let shownMarkdown = $state('');
  let currentId = $state<string | null>(null);
  let rid = '';
  let past = $state<SavedMeta[]>([]);
  let status = $state<AiStatus | null>(null);

  const boardsList = $derived(registry.data.boards.filter((b) => !b.missing && !b.hidden));
  const boardsLabel = $derived(
    selected.length === 0
      ? t('summaries.allBoards')
      : selected.length === 1
        ? (boardsList.find((b) => b.id === selected[0])?.name ?? '1')
        : t('summaries.nBoards', { n: selected.length }),
  );
  const range = $derived(resolveRange(preset, new Date(), { from: customFrom, to: customTo }));
  const html = $derived(shownMarkdown ? renderMarkdown(shownMarkdown, { titleOf: (id) => resolveTitle(id), fileUrl: (r) => r }) : '');
  const statusText = $derived.by(() => {
    if (!status) return '';
    if (status.provider === 'off') return t('ai.off');
    if (status.available && status.model) return t('ai.ready', { model: status.model });
    if (status.available) return t('ai.noModel');
    return t('ai.unavailable');
  });

  const offs: (() => void)[] = [];

  function applyPayload(p: Record<string, unknown> | undefined) {
    if (!p) return;
    if (p.view === 'schedules' || p.view === 'summary') view = p.view;
    if (typeof p.preset === 'string' && (RANGE_PRESETS as string[]).includes(p.preset)) preset = p.preset as RangePreset;
    if (Array.isArray(p.boards)) selected = p.boards.filter((b): b is string => typeof b === 'string');
    if (p.auto && view === 'summary') void generate();
  }

  $effect(() => {
    const p = tab.payload;
    untrack(() => applyPayload(p));
  });

  async function refreshPast() {
    past = await ai.saved(50).catch(() => []);
  }

  onMount(() => {
    void refreshPast();
    void ai
      .status()
      .then((s) => (status = s))
      .catch(() => (status = null));
    offs.push(
      onAiEvent<{ requestId: string; text: string }>('summary.chunk', (e) => {
        if (running && e.requestId === rid) stream += e.text;
      }),
      onAiEvent<{ requestId: string; stage: string }>('summary.progress', (e) => {
        if (e.requestId === rid) stage = e.stage;
      }),
      onAiEvent('schedules.ran', () => void refreshPast()),
    );
  });
  onDestroy(() => offs.forEach((f) => f()));

  async function generate() {
    const r = resolveRange(preset, new Date(), { from: customFrom, to: customTo });
    if (!r) {
      toast.warn(t('summaries.invalidRange'));
      return;
    }
    rid = requestId();
    const mine = rid;
    running = true;
    stage = 'facts';
    stream = '';
    result = null;
    shownMarkdown = '';
    try {
      const res = await ai.summarize({
        ...toIso(r),
        boards: $state.snapshot(selected),
        detail,
        prompt: prompt.trim() || undefined,
        engine,
        requestId: mine,
        locale: i18n.locale,
        includeRemote,
        links: settings.get<boolean>('summaries.links') !== false,
        save: true,
      });
      if (mine !== rid) return;
      result = res;
      shownMarkdown = res.markdown;
      currentId = res.id;
      if (res.fallbackReason && engine !== 'basic') toast.info(t('summaries.fallback', { reason: res.fallbackReason }));
      void refreshPast();
    } catch (e) {
      if (mine === rid) toast.error(t('summaries.failed', { message: (e as Error).message }));
    } finally {
      if (mine === rid) {
        running = false;
        stage = '';
      }
    }
  }

  function stop() {
    // The backend finishes on its own; we just stop listening.
    rid = '';
    running = false;
    stage = '';
    if (stream) shownMarkdown = stream;
  }

  async function openSaved(id: string) {
    try {
      const s = await ai.getSaved(id);
      result = null;
      shownMarkdown = s.markdown;
      currentId = s.id;
      view = 'summary';
    } catch (e) {
      toast.error((e as Error).message);
    }
  }

  async function deleteSaved(id: string) {
    if (!(await confirm({ title: t('summaries.deleteConfirm'), confirmLabel: t('summaries.deleteSaved'), danger: true }))) return;
    await ai.deleteSaved(id).catch((e: Error) => toast.error(e.message));
    if (currentId === id) {
      shownMarkdown = '';
      currentId = null;
      result = null;
    }
    void refreshPast();
  }

  async function copyAs(kind: 'md' | 'text' | 'slack') {
    const md = shownMarkdown;
    const titled = resolveLinks(md, (id) => resolveTitle(id));
    await copyText(kind === 'md' ? md : kind === 'text' ? plainText(titled) : slackText(titled));
    toast.success(t('summaries.copied'));
  }

  function copyMenu(e: MouseEvent) {
    openMenu(e, [
      { label: t('summaries.copyMarkdown'), run: () => copyAs('md') },
      { label: t('summaries.copyText'), run: () => copyAs('text') },
      { label: t('summaries.copySlack'), run: () => copyAs('slack') },
    ]);
  }

  async function exportMd() {
    const dest = await pickSavePath(t('summaries.exportMd'), `summary-${ymd(new Date())}.md`, [{ name: 'Markdown', extensions: ['md'] }]);
    if (!dest) return;
    try {
      await rpc('summaries.export', { path: dest, markdown: shownMarkdown });
      toast.success(t('summaries.exported'), { action: { label: t('common.reveal'), run: () => reveal(dest) } });
    } catch (e) {
      toast.error((e as Error).message);
    }
  }

  function toggleBoard(id: string) {
    selected = selected.includes(id) ? selected.filter((x) => x !== id) : [...selected, id];
  }

  function onOutputClick(e: MouseEvent) {
    const el = (e.target as HTMLElement).closest<HTMLElement>('[data-card]');
    if (el?.dataset.card) void openCardById(el.dataset.card);
  }

  function fmt(iso: string) {
    const d = new Date(iso);
    return Number.isNaN(d.getTime()) ? iso : d.toLocaleString(i18n.locale, { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' });
  }

  async function test() {
    await testConnection();
    status = await ai.status().catch(() => null);
  }
</script>

<div class="summary-view">
  <header class="head">
    <div class="titles">
      <h1><Sparkles size={18} strokeWidth={1.8} /> {view === 'summary' ? t('summaries.title') : t('schedules.title')}</h1>
      <p>{view === 'summary' ? t('summaries.subtitle') : t('schedules.subtitle')}</p>
    </div>
    <div class="head-actions">
      {#if status}
        <button class="chip ai-chip" class:ok={status.available && !!status.model} onclick={test} title={t('ai.test')}>
          <Circle size={8} strokeWidth={0} fill="currentColor" />
          {statusText}
        </button>
      {/if}
      <Segmented
        size="sm"
        bind:value={view}
        options={[
          { value: 'summary', label: t('summaries.title'), icon: Sparkles },
          { value: 'schedules', label: t('schedules.title'), icon: CalendarClock },
        ]}
      />
    </div>
  </header>

  {#if status?.remote}
    <div class="notice warn">{t('ai.remoteWarning')}</div>
  {/if}

  {#if view === 'schedules'}
    <SchedulesEditor
      onran={(id) => {
        void refreshPast();
        if (id) void openSaved(id);
      }}
    />
  {:else}
    <div class="layout">
      <section class="form card-surface" aria-label={t('summaries.title')}>
        <div class="group">
          <span class="section-title">{t('summaries.period')}</span>
          <div class="presets" role="radiogroup" aria-label={t('summaries.period')}>
            {#each RANGE_PRESETS as p (p)}
              <button class="chip preset" class:on={preset === p} role="radio" aria-checked={preset === p} onclick={() => (preset = p)}
                >{t(`summaries.presets.${p}`)}</button
              >
            {/each}
          </div>
          {#if preset === 'custom'}
            <div class="dates">
              <label>{t('summaries.from')} <input class="field" type="date" bind:value={customFrom} max={today} /></label>
              <label>{t('summaries.to')} <input class="field" type="date" bind:value={customTo} max={today} /></label>
            </div>
          {/if}
          {#if range}
            <span class="hint">{fmt(range.from.toISOString())} → {fmt(range.to.toISOString())}</span>
          {/if}
        </div>

        <div class="group">
          <button class="section-title disclosure" aria-expanded={showBoards} onclick={() => (showBoards = !showBoards)}>
            {t('summaries.boards')} · <span class="muted">{boardsLabel}</span>
            <ChevronDown size={14} strokeWidth={1.8} class={showBoards ? 'rot' : ''} />
          </button>
          {#if showBoards}
            <div class="board-chips">
              <button class="chip" class:on={selected.length === 0} onclick={() => (selected = [])}>{t('summaries.allBoards')}</button>
              {#each boardsList as b (b.id)}
                <button class="chip" class:on={selected.includes(b.id)} aria-pressed={selected.includes(b.id)} onclick={() => toggleBoard(b.id)}
                  >{b.name}</button
                >
              {/each}
            </div>
          {/if}
        </div>

        <div class="group">
          <label class="section-title" for="sum-detail">{t('summaries.detail')} · <span class="muted">{t(`summaries.detailLevels.${detail}`)}</span></label>
          <input
            id="sum-detail"
            class="slider"
            type="range"
            min="1"
            max="5"
            step="1"
            bind:value={detail}
            aria-valuetext={t(`summaries.detailLevels.${detail}`)}
          />
        </div>

        <div class="group">
          <label class="section-title" for="sum-prompt">{t('summaries.prompt')}</label>
          <textarea
            id="sum-prompt"
            class="field prompt"
            rows="3"
            maxlength="4000"
            placeholder={t('summaries.promptPlaceholder')}
            bind:value={prompt}
            onkeydown={(e) => {
              if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
                e.preventDefault();
                void generate();
              }
            }}></textarea>
        </div>

        <div class="group row-inline">
          <span class="section-title">{t('summaries.engine')}</span>
          <Segmented
            size="sm"
            bind:value={engine}
            options={[
              { value: 'auto', label: t('summaries.engines.auto') },
              { value: 'ai', label: t('summaries.engines.ai') },
              { value: 'basic', label: t('summaries.engines.basic') },
            ]}
          />
        </div>
        <div class="group row-inline">
          <span class="toggle-label">{t('summaries.includeRemote')}</span>
          <Toggle bind:checked={includeRemote} label={t('summaries.includeRemote')} />
        </div>

        <div class="actions">
          {#if running}
            <button class="btn soft" onclick={stop}><Square size={14} strokeWidth={1.8} /> {t('summaries.stop')}</button>
          {/if}
          <button class="btn primary" disabled={running || !range} onclick={generate}>
            <Sparkles size={14} strokeWidth={1.8} />
            {shownMarkdown ? t('summaries.regenerate') : t('summaries.generate')}
          </button>
        </div>
      </section>

      <section class="output card-surface" aria-live="polite">
        {#if running}
          <div class="progress">
            <span class="spin"><Sparkles size={14} strokeWidth={1.8} /></span>
            {t(`summaries.stages.${stage || 'facts'}`)}
          </div>
          {#if stream}
            <pre class="stream">{stream}</pre>
          {/if}
        {:else if shownMarkdown}
          <div class="out-bar">
            <span class="engine-note">
              <Bot size={14} strokeWidth={1.8} />
              {result?.engine === 'ai' && result.model ? t('summaries.writtenBy', { model: result.model }) : result ? t('summaries.basicEngine') : ''}
              {#if result}
                · {t('summaries.totals', {
                  completed: result.facts.totals.completed,
                  started: result.facts.totals.started,
                  created: result.facts.totals.created,
                  edited: result.facts.totals.edited,
                })}
              {/if}
            </span>
            <div class="out-actions">
              <button
                class="btn sm soft"
                onclick={() => copyAs('md')}
                oncontextmenu={(e) => {
                  e.preventDefault();
                  copyMenu(e);
                }}><Copy size={14} strokeWidth={1.8} /> {t('summaries.copy')}</button
              >
              <button class="icon-btn sm" aria-label={t('summaries.copy')} onclick={copyMenu}><ChevronDown size={14} strokeWidth={1.8} /></button>
              <button class="btn sm ghost" onclick={exportMd}><FileDown size={14} strokeWidth={1.8} /> {t('summaries.exportMd')}</button>
            </div>
          </div>
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
          <!-- eslint-disable-next-line svelte/no-at-html-tags -- renderMarkdown: markdown-it with html:false -->
          <article class="md-out" onclick={onOutputClick}>{@html html}</article>
        {:else}
          <div class="empty">
            <Sparkles size={22} strokeWidth={1.6} />
            <p>{t('summaries.empty')}</p>
          </div>
        {/if}
      </section>

      <aside class="past" aria-label={t('summaries.past')}>
        <span class="section-title">{t('summaries.past')}</span>
        {#if past.length === 0}
          <p class="muted small">{t('summaries.noPast')}</p>
        {:else}
          <ul>
            {#each past as s (s.id)}
              <li class="row" class:active={currentId === s.id}>
                <button class="grow past-open" onclick={() => openSaved(s.id)}>
                  <span class="past-title">{s.title}</span>
                  <span class="past-meta">{fmt(s.created)}{s.scheduleId ? ` · ${t('summaries.scheduled')}` : ''}</span>
                </button>
                <button class="icon-btn sm" aria-label={t('summaries.deleteSaved')} onclick={() => deleteSaved(s.id)}
                  ><Trash2 size={14} strokeWidth={1.8} /></button
                >
              </li>
            {/each}
          </ul>
        {/if}
      </aside>
    </div>
  {/if}
</div>

<style>
  .summary-view {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: var(--sp-8) var(--sp-8) var(--sp-10);
    display: flex;
    flex-direction: column;
    gap: var(--sp-5);
  }
  .head {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: var(--sp-4);
    flex-wrap: wrap;
  }
  h1 {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    margin: 0;
    font-size: var(--fs-xl);
    font-weight: var(--fw-semibold);
    color: var(--ink);
  }
  .titles p {
    margin: var(--sp-1) 0 0;
    color: var(--ink-3);
    font-size: var(--fs-md);
  }
  .head-actions {
    display: flex;
    align-items: center;
    gap: var(--sp-3);
  }
  .ai-chip {
    color: var(--ink-3);
    cursor: pointer;
  }
  .ai-chip.ok {
    color: var(--ok);
  }
  .notice {
    padding: var(--sp-2) var(--sp-3);
    border-radius: var(--r-sm);
    font-size: var(--fs-sm);
  }
  .notice.warn {
    background: var(--warn-soft);
    color: var(--ink-2);
  }
  .layout {
    display: grid;
    grid-template-columns: minmax(280px, 340px) minmax(0, 1fr) minmax(200px, 240px);
    gap: var(--sp-5);
    align-items: start;
  }
  @media (max-width: 1100px) {
    .layout {
      grid-template-columns: minmax(260px, 320px) minmax(0, 1fr);
    }
    .past {
      grid-column: 1 / -1;
    }
  }
  .form {
    padding: var(--sp-5);
    display: flex;
    flex-direction: column;
    gap: var(--sp-5);
  }
  .group {
    display: flex;
    flex-direction: column;
    gap: var(--sp-2);
  }
  .row-inline {
    flex-direction: row;
    align-items: center;
    justify-content: space-between;
  }
  .toggle-label {
    font-size: var(--fs-sm);
    color: var(--ink-2);
  }
  .presets,
  .board-chips {
    display: flex;
    flex-wrap: wrap;
    gap: var(--sp-1);
  }
  .chip {
    cursor: pointer;
    border: 1px solid transparent;
    transition: background var(--dur-fast) var(--ease);
  }
  .chip.on {
    background: var(--primary-soft);
    color: var(--primary-strong);
    border-color: var(--primary-ring);
  }
  .dates {
    display: flex;
    gap: var(--sp-2);
  }
  .dates label {
    display: flex;
    flex-direction: column;
    gap: var(--sp-1);
    font-size: var(--fs-xs);
    color: var(--ink-3);
    flex: 1;
  }
  .hint,
  .muted {
    color: var(--ink-3);
    font-size: var(--fs-xs);
    font-weight: var(--fw-regular);
  }
  .small {
    font-size: var(--fs-sm);
  }
  .disclosure {
    display: flex;
    align-items: center;
    gap: var(--sp-1);
    background: none;
    border: 0;
    padding: 0;
    cursor: pointer;
    text-align: left;
  }
  .disclosure :global(.rot) {
    transform: rotate(180deg);
  }
  .slider {
    width: 100%;
    accent-color: var(--primary);
  }
  .prompt {
    resize: vertical;
    min-height: 64px;
    font-family: inherit;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--sp-2);
  }
  .output {
    padding: var(--sp-5) var(--sp-6);
    min-height: 320px;
  }
  .progress {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    color: var(--ink-3);
    font-size: var(--fs-md);
  }
  .stream {
    white-space: pre-wrap;
    font-family: var(--font-ui);
    font-size: var(--fs-md);
    color: var(--ink-2);
    line-height: var(--lh);
  }
  .out-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--sp-3);
    flex-wrap: wrap;
    padding-bottom: var(--sp-3);
    border-bottom: 1px solid var(--line);
    margin-bottom: var(--sp-3);
  }
  .engine-note {
    display: flex;
    align-items: center;
    gap: var(--sp-1);
    color: var(--ink-3);
    font-size: var(--fs-xs);
  }
  .out-actions {
    display: flex;
    align-items: center;
    gap: var(--sp-1);
  }
  .md-out {
    color: var(--ink);
    font-size: var(--fs-md);
    line-height: var(--lh);
  }
  .md-out :global(h1) {
    font-size: var(--fs-xl);
    margin: 0 0 var(--sp-1);
  }
  .md-out :global(h2) {
    font-size: var(--fs-lg);
    margin: var(--sp-5) 0 var(--sp-2);
  }
  .md-out :global(h3) {
    font-size: var(--fs-md);
    color: var(--ink-2);
    margin: var(--sp-4) 0 var(--sp-1);
  }
  .md-out :global(ul) {
    margin: 0;
    padding-left: var(--sp-5);
  }
  .md-out :global(.card-link) {
    color: var(--primary-strong);
    background: var(--primary-softer);
    border-radius: var(--r-xs);
    padding: 0 var(--sp-1);
    cursor: pointer;
  }
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--sp-2);
    padding: var(--sp-10) 0;
    color: var(--ink-3);
  }
  .past ul {
    list-style: none;
    margin: var(--sp-2) 0 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .past-open {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    background: none;
    border: 0;
    padding: 0;
    cursor: pointer;
    min-width: 0;
    text-align: left;
    color: inherit;
  }
  .past-title {
    font-size: var(--fs-sm);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 100%;
  }
  .past-meta {
    font-size: var(--fs-xs);
    color: var(--ink-3);
  }
</style>
