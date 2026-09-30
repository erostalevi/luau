<script lang="ts">
  import { tick } from 'svelte';
  import { ArrowLeft, CornerDownLeft, Loader } from '@lucide/svelte';
  import { qi, closeQuickInput, BACK, type QuickItem, type PickOptions, type InputOptions } from './qi.svelte';
  import { fuzzy, highlightSegments } from '$lib/util/fuzzy';
  import Kbd from '$lib/components/Kbd.svelte';
  import { t } from '$lib/i18n/index.svelte';

  let input: HTMLInputElement | undefined = $state();
  let listEl: HTMLDivElement | undefined = $state();
  let active = $state(0);
  let dynItems = $state<QuickItem[]>([]);
  let validating = $state(false);
  let seq = 0;

  const st = $derived(qi.st);
  const pickOpts = $derived(st?.mode === 'pick' ? (st.opts as PickOptions<unknown>) : null);

  interface Row {
    item: QuickItem;
    index: number;
    pos: number[];
    descPos: number[];
  }

  const rows = $derived.by((): Row[] => {
    if (!st || st.mode !== 'pick') return [];
    const source = pickOpts?.onValue ? dynItems : st.items;
    const q = st.value.trim();
    const skipFilter = !!pickOpts?.selfFiltered;
    if (!q || skipFilter) return source.map((item, index) => ({ item, index, pos: item.highlights ?? [], descPos: [] }));
    const out: (Row & { score: number })[] = [];
    source.forEach((item, index) => {
      if (item.kind === 'separator') return;
      const m = fuzzy(q, item.label);
      const d = !m && pickOpts?.matchOnDescription && item.description ? fuzzy(q, item.description) : null;
      if (m || d || item.alwaysShow) out.push({ item, index, pos: m?.positions ?? [], descPos: d?.positions ?? [], score: m?.score ?? (d ? d.score - 50 : -999) });
    });
    out.sort((a, b) => b.score - a.score);
    const custom = pickOpts?.allowCustom?.(q);
    if (custom && !out.some((r) => r.item.label.toLowerCase() === q.toLowerCase()))
      out.push({ item: custom, index: -1, pos: [], descPos: [], score: -1000 });
    return out;
  });

  const selectable = $derived(rows.filter((r) => r.item.kind !== 'separator'));

  // Reset per new quick input.
  $effect(() => {
    const gen = st?.gen;
    if (gen === undefined) return;
    active = (st?.opts as PickOptions<unknown>)?.activeIndex ?? 0;
    dynItems = [];
    void tick().then(() => {
      input?.focus();
      if ((st?.opts as InputOptions)?.selectAll !== false) input?.select();
      refreshDynamic();
      if (st?.mode === 'input') void validate();
    });
  });

  $effect(() => {
    const r = selectable[active];
    pickOpts?.onActive?.(r?.item);
    const el = listEl?.querySelector(`[data-i="${active}"]`);
    el?.scrollIntoView({ block: 'nearest' });
  });

  async function refreshDynamic() {
    if (!st || st.mode !== 'pick' || !pickOpts?.onValue) return;
    const my = ++seq;
    st.busy = true;
    try {
      const items = await pickOpts.onValue(st.value);
      if (my === seq) {
        dynItems = items;
        if (active >= items.length) active = 0;
      }
    } finally {
      if (my === seq && qi.st) qi.st.busy = false;
    }
  }

  async function validate() {
    if (!st || st.mode !== 'input') return;
    const v = (st.opts as InputOptions).validate;
    if (!v) return;
    validating = true;
    st.validation = await v(st.value);
    validating = false;
  }

  function oninput() {
    if (!st) return;
    active = 0;
    if (pickOpts?.prefixRouter?.(st.value)) return;
    if (st.mode === 'pick') refreshDynamic();
    else validate();
  }

  async function accept() {
    if (!st) return;
    if (st.mode === 'input') {
      await validate();
      if (st.validation) return;
      closeQuickInput(st.value);
      return;
    }
    if (st.canPickMany) {
      const source = pickOpts?.onValue ? dynItems : st.items;
      closeQuickInput([...st.selected].map((i) => source[i]?.value));
      return;
    }
    const r = selectable[active];
    if (!r) return;
    closeQuickInput(r.item.value ?? r.item);
  }

  function toggleSel(r: Row) {
    if (!st) return;
    if (st.selected.has(r.index)) st.selected.delete(r.index);
    else st.selected.add(r.index);
    st.selected = new Set(st.selected);
  }

  function onkeydown(e: KeyboardEvent) {
    if (!st) return;
    const n = selectable.length;
    if (e.key === 'Escape') {
      e.preventDefault();
      closeQuickInput(undefined);
    } else if (e.key === 'ArrowDown' || (e.ctrlKey && e.key === 'n')) {
      e.preventDefault();
      if (n) active = (active + 1) % n;
    } else if (e.key === 'ArrowUp' || (e.ctrlKey && e.key === 'p')) {
      e.preventDefault();
      if (n) active = (active - 1 + n) % n;
    } else if (e.key === 'PageDown') {
      e.preventDefault();
      active = Math.min(n - 1, active + 8);
    } else if (e.key === 'PageUp') {
      e.preventDefault();
      active = Math.max(0, active - 8);
    } else if (e.key === 'Enter') {
      e.preventDefault();
      void accept();
    } else if (e.key === ' ' && st.canPickMany && (e.ctrlKey || e.metaKey || st.value === '')) {
      e.preventDefault();
      const r = selectable[active];
      if (r) toggleSel(r);
    } else if ((e.key === 'Backspace' && st.value === '' && (st.step ?? 1) > 1) || (e.altKey && e.key === 'ArrowLeft')) {
      e.preventDefault();
      closeQuickInput(BACK);
    }
  }
</script>

{#if st}
  <div class="qi-scrim" role="presentation" onpointerdown={() => closeQuickInput(undefined)}></div>
  <div class="qi card-surface glass" role="dialog" aria-modal="true" aria-label={st.title ?? 'Quick input'}>
    {#if st.title || st.totalSteps}
      <div class="head">
        {#if (st.step ?? 1) > 1}
          <button class="icon-btn sm" onclick={() => closeQuickInput(BACK)} title={t('common.back')}><ArrowLeft size={14} /></button>
        {/if}
        <span class="title">{st.title ?? ''}</span>
        {#if st.totalSteps}<span class="steps">{st.step ?? 1}/{st.totalSteps}</span>{/if}
      </div>
    {/if}
    <div class="bar">
      <input
        bind:this={input}
        bind:value={st.value}
        type={st.password ? 'password' : 'text'}
        placeholder={st.placeholder}
        spellcheck="false"
        autocomplete="off"
        {oninput}
        {onkeydown}
      />
      {#if st.busy || validating}<Loader size={15} class="spin muted" />{/if}
    </div>
    {#if st.mode === 'input'}
      <div class="prompt" class:error={!!st.validation}>
        {#if st.validation}
          {st.validation}
        {:else}
          <span>{st.prompt ?? ''}</span>
          <span class="enter"><CornerDownLeft size={12} /> {t('common.confirm')}</span>
        {/if}
      </div>
    {:else}
      <div class="list" bind:this={listEl} role="listbox">
        {#each rows as r, ri (r.item.kind === 'separator' ? 'sep' + ri : r.index + ':' + r.item.label + ri)}
          {#if r.item.kind === 'separator'}
            <div class="sep">{r.item.label}</div>
          {:else}
            {@const si = selectable.indexOf(r)}
            <button
              class="opt"
              class:active={si === active}
              data-i={si}
              role="option"
              aria-selected={si === active}
              onpointermove={() => (active = si)}
              onclick={() => (st.canPickMany ? toggleSel(r) : ((active = si), accept()))}
            >
              {#if st.canPickMany}
                <span class="check" class:on={st.selected.has(r.index)}></span>
              {/if}
              {#if r.item.icon}
                <span class="ic" style:color={r.item.iconColor}><r.item.icon size={15} strokeWidth={1.8} /></span>
              {/if}
              <span class="text">
                <span class="label">
                  {#each highlightSegments(r.item.label, r.pos) as seg, k (k)}
                    {#if seg.hit}<mark>{seg.text}</mark>{:else}{seg.text}{/if}
                  {/each}
                </span>
                {#if r.item.description}
                  <span class="desc">
                    {#each highlightSegments(r.item.description, r.descPos) as seg, k (k)}
                      {#if seg.hit}<mark>{seg.text}</mark>{:else}{seg.text}{/if}
                    {/each}
                  </span>
                {/if}
                {#if r.item.detail}<span class="detail">{r.item.detail}</span>{/if}
              </span>
              {#if r.item.keybinding}<Kbd keys={r.item.keybinding} />{/if}
            </button>
          {/if}
        {:else}
          {#if !st.busy}<div class="none">{t('quickinput.noResults')}</div>{/if}
        {/each}
      </div>
      {#if st.canPickMany}
        <div class="foot">
          <span class="muted">{t('quickinput.selected', { count: st.selected.size })}</span>
          <button class="btn sm primary" onclick={accept}>{t('common.ok')}</button>
        </div>
      {/if}
    {/if}
  </div>
{/if}

<style>
  .qi-scrim {
    position: fixed;
    inset: 0;
    z-index: 950;
    background: transparent;
  }
  .qi {
    position: fixed;
    z-index: 951;
    top: calc(var(--titlebar-h) + 14px);
    left: 50%;
    width: min(640px, calc(100vw - 48px));
    translate: -50% 0;
    border-radius: var(--r-lg);
    overflow: hidden;
    animation: pop-in var(--dur) var(--ease-out);
    display: flex;
    flex-direction: column;
    max-height: min(560px, calc(100vh - 120px));
  }
  .head {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 10px 14px 0;
    font-size: var(--fs-sm);
    color: var(--ink-3);
  }
  .head .title {
    flex: 1;
    font-weight: var(--fw-medium);
  }
  .steps {
    font-variant-numeric: tabular-nums;
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    border-bottom: 1px solid var(--line);
  }
  .bar input {
    flex: 1;
    border: none;
    background: transparent;
    font-size: var(--fs-lg);
    padding: 4px 2px;
    color: var(--ink);
  }
  .bar input::placeholder {
    color: var(--ink-4);
  }
  .prompt {
    display: flex;
    justify-content: space-between;
    padding: 10px 16px 12px;
    font-size: var(--fs-sm);
    color: var(--ink-3);
  }
  .prompt.error {
    color: var(--danger);
  }
  .enter {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .list {
    overflow-y: auto;
    padding: 6px;
  }
  .opt {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    min-height: 34px;
    padding: 5px 10px;
    border: none;
    border-radius: var(--r-sm);
    background: transparent;
    text-align: left;
    color: var(--ink);
  }
  .opt.active {
    background: var(--primary-soft);
  }
  .ic {
    display: grid;
    place-items: center;
    width: 20px;
    color: var(--ink-3);
    flex-shrink: 0;
  }
  .text {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: baseline;
    gap: 8px;
    flex-wrap: wrap;
  }
  .label {
    font-size: var(--fs-md);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 100%;
  }
  .desc {
    font-size: var(--fs-sm);
    color: var(--ink-3);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .detail {
    flex-basis: 100%;
    font-size: var(--fs-sm);
    color: var(--ink-3);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .sep {
    padding: 10px 10px 4px;
    font-size: var(--fs-xs);
    font-weight: var(--fw-semibold);
    color: var(--ink-3);
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }
  .none {
    padding: 18px;
    text-align: center;
    color: var(--ink-3);
  }
  .check {
    width: 16px;
    height: 16px;
    border-radius: 5px;
    box-shadow: inset 0 0 0 1.5px var(--line-strong);
    flex-shrink: 0;
  }
  .check.on {
    background: var(--primary);
    box-shadow: none;
  }
  .foot {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 8px 12px;
    border-top: 1px solid var(--line);
  }
</style>
