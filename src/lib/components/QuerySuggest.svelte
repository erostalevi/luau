<script lang="ts" module>
  import { rpc } from '$lib/backend/rpc';
  import { registry } from '$lib/state/registry.svelte';
  import { boards } from '$lib/state/boards.svelte';
  import type { SuggestSources } from '$lib/panels/search/suggest';

  /** Tags, people, boards and lanes across everything indexed (fetched on focus). */
  export async function loadSources(): Promise<SuggestSources> {
    const [tags, people] = await Promise.all([
      rpc<[string, number][]>('search.tags', { boards: [] }).catch(() => [] as [string, number][]),
      rpc<string[]>('search.people').catch(() => [] as string[]),
    ]);
    const lanes = new Map<string, string>();
    for (const b of boards.values()) for (const l of b.lanes) if (!l.archived && l.name.trim()) lanes.set(l.name.trim().toLowerCase(), l.name.trim());
    return {
      tags: tags.map(([tag]) => tag),
      people,
      boards: registry.data.boards.filter((b) => !b.missing).map((b) => b.name),
      lanes: [...lanes.values()],
    };
  }
</script>

<script lang="ts">
  // Autocomplete dropdown for a search <input>: filter keys, values, #tags,
  // @people and dates (see panels/search/suggest.ts). It listens on the input
  // itself and only takes Up/Down/Enter/Tab/Esc while the list is open.
  import { tick } from 'svelte';
  import { Hash, AtSign, CalendarDays, Filter, CornerDownLeft } from '@lucide/svelte';
  import { t } from '$lib/i18n/index.svelte';
  import { suggest, applySuggestion, type SuggestConfig, type SuggestResult, type Suggestion } from '$lib/panels/search/suggest';

  let {
    input,
    config,
    sources = loadSources,
    onapply,
  }: {
    input: HTMLInputElement | undefined;
    config: Omit<SuggestConfig, 'label'>;
    sources?: () => Promise<SuggestSources>;
    /** Called with the new text; the component restores the caret after the next render. */
    onapply: (text: string) => void;
  } = $props();

  let src: SuggestSources = { tags: [], people: [] };
  let result = $state<SuggestResult | null>(null);
  let active = $state(0);
  let rect = $state<{ left: number; top: number; width: number } | null>(null);
  let dismissedAt = -1;

  const label = (id: string) => t(`searchPanel.suggest.${id}`);
  const ICONS = { key: Filter, value: Filter, tag: Hash, person: AtSign, date: CalendarDays };

  function refresh() {
    if (!input || document.activeElement !== input) return close();
    const caret = input.selectionStart ?? input.value.length;
    if (input.selectionEnd !== caret || caret === dismissedAt) return close();
    const r = suggest(input.value, caret, src, { ...config, label });
    if (!r) return close();
    const changed = !result || result.from !== r.from || result.items.length !== r.items.length;
    result = r;
    if (changed || active >= r.items.length) active = 0;
    // Anchor to the input's box (icon + buttons), not the bare text field.
    const b = (input.parentElement ?? input).getBoundingClientRect();
    const width = Math.min(Math.max(260, b.width), 380, window.innerWidth - 16);
    rect = { left: Math.max(8, Math.min(b.left, window.innerWidth - width - 8)), top: b.bottom + 4, width };
  }

  function close() {
    result = null;
  }

  async function pick(s: Suggestion) {
    if (!input || !result) return;
    const next = applySuggestion(input.value, result, s);
    onapply(next.text);
    await tick();
    if (input.value !== next.text) input.value = next.text;
    input.setSelectionRange(next.caret, next.caret);
    input.focus();
    dismissedAt = -1;
    refresh();
  }

  function onkeydown(e: KeyboardEvent) {
    if (!result || e.isComposing) return;
    const n = result.items.length;
    let handled = true;
    if (e.key === 'ArrowDown') active = (active + 1) % n;
    else if (e.key === 'ArrowUp') active = (active - 1 + n) % n;
    else if ((e.key === 'Enter' && !e.shiftKey && !e.metaKey && !e.ctrlKey) || (e.key === 'Tab' && !e.shiftKey)) void pick(result.items[active]);
    else if (e.key === 'Escape') {
      dismissedAt = input?.selectionStart ?? -1;
      close();
    } else handled = false;
    if (handled) {
      e.preventDefault();
      e.stopImmediatePropagation();
    }
  }

  $effect(() => {
    const el = input;
    if (!el) return;
    const onInput = () => {
      dismissedAt = -1;
      refresh();
    };
    const onFocus = () => {
      void sources().then((s) => {
        src = s;
        refresh();
      });
    };
    const onCaret = (e: Event) => {
      if (e instanceof KeyboardEvent && !['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(e.key)) return;
      refresh();
    };
    const onBlur = () => close();
    // Capture so we run before the input's own keydown handler.
    el.addEventListener('keydown', onkeydown, true);
    el.addEventListener('input', onInput);
    el.addEventListener('focus', onFocus);
    el.addEventListener('keyup', onCaret);
    el.addEventListener('click', onCaret);
    el.addEventListener('blur', onBlur);
    if (document.activeElement === el) onFocus();
    return () => {
      el.removeEventListener('keydown', onkeydown, true);
      el.removeEventListener('input', onInput);
      el.removeEventListener('focus', onFocus);
      el.removeEventListener('keyup', onCaret);
      el.removeEventListener('click', onCaret);
      el.removeEventListener('blur', onBlur);
    };
  });
</script>

{#if result && rect}
  <div
    class="qsuggest card-surface glass"
    role="listbox"
    aria-label={t('searchPanel.suggest.label')}
    style:left="{rect.left}px"
    style:top="{rect.top}px"
    style:width="{rect.width}px"
  >
    {#each result.items as s, i (s.insert)}
      {@const Icon = ICONS[s.kind]}
      <!-- mousedown keeps focus in the input -->
      <div
        class="opt"
        class:active={i === active}
        role="option"
        tabindex="-1"
        aria-selected={i === active}
        onpointerenter={() => (active = i)}
        onmousedown={(e) => {
          e.preventDefault();
          void pick(s);
        }}
      >
        <Icon size={13} strokeWidth={1.9} />
        <span class="label">{s.label}</span>
        {#if s.detail}<span class="detail">{s.detail}</span>{/if}
        {#if i === active}<CornerDownLeft size={12} class="enter" />{/if}
      </div>
    {/each}
  </div>
{/if}

<style>
  .qsuggest {
    position: fixed;
    z-index: 950;
    padding: 4px;
    border-radius: var(--r-md);
    animation: pop-in var(--dur) var(--ease-out);
  }
  .opt {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 28px;
    padding: 0 8px;
    border-radius: var(--r-xs);
    color: var(--ink-2);
    font-size: var(--fs-md);
    cursor: default;
  }
  .opt :global(svg) {
    flex: none;
    color: var(--ink-3);
  }
  .opt.active {
    background: var(--primary-soft);
    color: var(--primary-strong);
  }
  .opt.active :global(svg) {
    color: inherit;
  }
  .label {
    flex: 1;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .detail {
    flex: none;
    color: var(--ink-3);
    font-size: var(--fs-sm);
    font-variant-numeric: tabular-nums;
  }
</style>
