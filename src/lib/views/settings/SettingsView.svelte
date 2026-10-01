<script lang="ts">
  import { tick } from 'svelte';
  import { Search, X, ListFilter, FileJson, Upload, Download, RotateCcw, Keyboard, ArrowRight, SearchX } from '@lucide/svelte';
  import { CATEGORIES, type SettingDef } from '$lib/settings/schema';
  import { settings } from '$lib/settings/store.svelte';
  import { runCommand, getCommand } from '$lib/commands/registry.svelte';
  import { confirm } from '$lib/state/dialogs.svelte';
  import { openMenuAt, type MenuItem } from '$lib/state/menu.svelte';
  import { toast } from '$lib/state/toasts.svelte';
  import { tip } from '$lib/components/tooltip';
  import Kbd from '$lib/components/Kbd.svelte';
  import { isMac } from '$lib/keybindings/keys';
  import { t, setLocale, detectLocale } from '$lib/i18n/index.svelte';
  import { parseSettingsQuery, filterSettings, categoryOrder, isFiltering } from './search';
  import { settingLabel, settingDesc, categoryTitle } from './labels';
  import { categoryIcon } from './categoryIcons';
  import { setSetting } from './effects';
  import { prefsUi } from './prefsState.svelte';
  import SettingRow from './SettingRow.svelte';
  import AiStatusCard from './AiStatusCard.svelte';

  let { initial }: { initial?: string } = $props();

  let query = $state('');
  let input: HTMLInputElement | undefined = $state();
  let scroller: HTMLDivElement | undefined = $state();
  let active = $state('');

  // Honour the initial query (and later openSingleton('settings', { query }) calls).
  $effect(() => {
    if (initial !== undefined) query = initial;
  });

  $effect(() => {
    if (prefsUi.settingsFocus) void tick().then(() => input?.select());
  });

  const defs = $derived(settings.defs);
  const order = $derived(categoryOrder(defs, CATEGORIES));
  const parsed = $derived(parseSettingsQuery(query, order));
  const filtering = $derived(isFiltering(parsed));
  const text = (d: SettingDef) => ({ label: settingLabel(d), desc: settingDesc(d) });
  const results = $derived(filterSettings(defs, parsed, text, (k) => settings.isModified(k)));
  const groups = $derived(order.map((id) => ({ id, items: results.filter((d) => d.category === id) })).filter((g) => g.items.length));
  const totals = $derived(new Map(order.map((id) => [id, defs.filter((d) => !d.hidden && d.category === id).length])));
  const modifiedCount = $derived(defs.filter((d) => !d.hidden && settings.isModified(d.key)).length);

  $effect(() => {
    if (!groups.some((g) => g.id === active)) active = groups[0]?.id ?? '';
  });

  function onScroll() {
    if (!scroller) return;
    const top = scroller.scrollTop + 48;
    let cur = groups[0]?.id ?? '';
    for (const g of groups) {
      const el = scroller.querySelector<HTMLElement>(`[data-cat="${g.id}"]`);
      if (el && el.offsetTop <= top) cur = g.id;
    }
    if (scroller.scrollTop + scroller.clientHeight >= scroller.scrollHeight - 4) cur = groups[groups.length - 1]?.id ?? cur;
    active = cur;
  }

  function jump(id: string) {
    const el = scroller?.querySelector<HTMLElement>(`[data-cat="${id}"]`);
    if (!el || !scroller) return;
    const motion = settings.get<string>('appearance.motion');
    scroller.scrollTo({ top: el.offsetTop - 12, behavior: motion === 'none' ? 'auto' : 'smooth' });
    active = id;
  }

  function toggleToken(token: string) {
    const parts = query.split(/\s+/).filter(Boolean);
    const i = parts.findIndex((p) => p.toLowerCase() === token.toLowerCase());
    if (i >= 0) parts.splice(i, 1);
    else parts.unshift(token);
    query = parts.join(' ') + (parts.length ? ' ' : '');
    input?.focus();
  }

  function filterMenu(e: MouseEvent) {
    const has = (tok: string) => query.toLowerCase().split(/\s+/).includes(tok.toLowerCase());
    const items: MenuItem[] = [
      { label: t('prefs.settings.filterModified'), checked: has('@modified'), run: () => toggleToken('@modified') },
      { label: t('prefs.settings.filterAi'), checked: has('@ai'), run: () => toggleToken('@ai') },
      { separator: true },
      ...order.map((id) => ({
        label: categoryTitle(id),
        icon: categoryIcon(id),
        checked: has(`@category:${id}`),
        run: () => toggleToken(`@category:${id}`),
      })),
    ];
    openMenuAt(e.currentTarget as HTMLElement, items);
  }

  async function resetAll() {
    const ok = await confirm({
      title: t('prefs.settings.resetAllTitle'),
      message: t('prefs.settings.resetAllMessage', { count: modifiedCount }),
      confirmLabel: t('prefs.settings.resetAll'),
      danger: true,
    });
    if (!ok) return;
    const hadAutostart = settings.get<boolean>('app.launchAtLogin');
    const keep = settings.get<boolean>('general.firstRunDone');
    settings.resetAll();
    if (keep) settings.set('general.firstRunDone', true);
    if (hadAutostart) setSetting('app.launchAtLogin', false);
    await setLocale(detectLocale());
    toast.success(t('prefs.settings.resetDone'));
  }

  function onKeydown(e: KeyboardEvent) {
    const mod = isMac ? e.metaKey : e.ctrlKey;
    if (mod && !e.shiftKey && !e.altKey && e.key.toLowerCase() === 'f') {
      e.preventDefault();
      input?.select();
    }
  }

  const run = (id: string) => () => void runCommand(id);
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="settings" onkeydown={onKeydown}>
  <header>
    <div class="titlebar">
      <h1>{t('tabs.settings')}</h1>
      <span class="spacer"></span>
      <button class="btn sm ghost" onclick={run('app.openKeybindings')} use:tip={{ text: t('commands.app.openKeybindings'), command: 'app.openKeybindings' }}>
        <Keyboard size={14} strokeWidth={1.8} />{t('tabs.keybindings')}
      </button>
      {#if getCommand('app.openSettingsJson')}
        <button
          class="icon-btn"
          aria-label={t('commands.app.openSettingsJson')}
          use:tip={t('commands.app.openSettingsJson')}
          onclick={run('app.openSettingsJson')}
        >
          <FileJson size={16} strokeWidth={1.8} />
        </button>
      {/if}
      <button class="icon-btn" aria-label={t('commands.app.exportSettings')} use:tip={t('commands.app.exportSettings')} onclick={run('app.exportSettings')}>
        <Upload size={16} strokeWidth={1.8} />
      </button>
      <button class="icon-btn" aria-label={t('commands.app.importSettings')} use:tip={t('commands.app.importSettings')} onclick={run('app.importSettings')}>
        <Download size={16} strokeWidth={1.8} />
      </button>
      <button class="icon-btn" aria-label={t('prefs.settings.resetAll')} use:tip={t('prefs.settings.resetAll')} onclick={resetAll} disabled={!modifiedCount}>
        <RotateCcw size={16} strokeWidth={1.8} />
      </button>
    </div>
    <div class="search" class:filtering>
      <Search size={16} strokeWidth={1.8} />
      <input
        bind:this={input}
        bind:value={query}
        placeholder={t('prefs.settings.searchPlaceholder')}
        aria-label={t('prefs.settings.searchPlaceholder')}
        spellcheck="false"
        onkeydown={(e) => {
          if (e.key === 'Escape' && query) {
            e.preventDefault();
            e.stopPropagation();
            query = '';
          }
        }}
      />
      {#if filtering}
        <span class="count">{t('prefs.settings.found', { count: results.length })}</span>
        <button class="icon-btn sm" aria-label={t('prefs.clear')} use:tip={t('prefs.clear')} onclick={() => ((query = ''), input?.focus())}
          ><X size={14} /></button
        >
      {/if}
      <button class="icon-btn sm" aria-label={t('prefs.settings.filters')} use:tip={t('prefs.settings.filters')} onclick={filterMenu}
        ><ListFilter size={15} strokeWidth={1.8} /></button
      >
    </div>
  </header>

  <div class="body">
    <nav class="toc" aria-label={t('prefs.settings.categories')}>
      {#each order as id (id)}
        {@const n = groups.find((g) => g.id === id)?.items.length ?? 0}
        {@const Icon = categoryIcon(id)}
        <button class="row cat" class:active={active === id && n > 0} disabled={!n} onclick={() => jump(id)}>
          <Icon size={15} strokeWidth={1.8} />
          <span class="grow">{categoryTitle(id)}</span>
          {#if filtering}<span class="n">{n}</span>{:else}<span class="n faint">{totals.get(id)}</span>{/if}
        </button>
      {/each}
      <div class="toc-foot">
        <button class="row cat" class:active={parsed.modified} onclick={() => toggleToken('@modified')}>
          <span class="mdot"></span>
          <span class="grow">{t('prefs.settings.modifiedOnly')}</span>
          <span class="n">{modifiedCount}</span>
        </button>
      </div>
    </nav>

    <div class="scroller" bind:this={scroller} onscroll={onScroll}>
      <div class="content">
        {#each groups as g (g.id)}
          {@const Icon = categoryIcon(g.id)}
          <section data-cat={g.id}>
            <h2><span class="cat-ic"><Icon size={15} strokeWidth={1.8} /></span>{categoryTitle(g.id)}</h2>
            {#if g.id === 'keyboard'}
              <button class="link-card" onclick={run('app.openKeybindings')}>
                <Keyboard size={18} strokeWidth={1.7} />
                <span class="grow">
                  <strong>{t('prefs.settings.shortcutsCardTitle')}</strong>
                  <span>{t('prefs.settings.shortcutsCardDesc')}</span>
                </span>
                <Kbd command="app.openKeybindings" />
                <ArrowRight size={15} />
              </button>
            {/if}
            {#if g.id === 'ai'}<AiStatusCard />{/if}
            {#each g.items as d (d.key)}
              <SettingRow def={d} showKey={filtering} />
            {/each}
          </section>
        {:else}
          <div class="empty">
            <span class="empty-icon"><SearchX size={20} strokeWidth={1.7} /></span>
            <strong>{t('prefs.settings.noResults')}</strong>
            <span>{t('prefs.settings.noResultsHint')}</span>
            <button class="btn sm soft" onclick={() => (query = '')}>{t('prefs.clear')}</button>
          </div>
        {/each}
      </div>
    </div>
  </div>
</div>

<style>
  .settings {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    animation: fade-in var(--dur) var(--ease);
  }
  header {
    padding: var(--sp-6) var(--sp-8) var(--sp-4);
    display: flex;
    flex-direction: column;
    gap: var(--sp-4);
    border-bottom: 1px solid var(--line);
  }
  .titlebar {
    display: flex;
    align-items: center;
    gap: var(--sp-1);
  }
  h1 {
    margin: 0;
    font-size: var(--fs-2xl);
    font-weight: var(--fw-semibold);
    letter-spacing: -0.01em;
  }
  .spacer {
    flex: 1;
  }
  .search {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    height: 40px;
    padding: 0 var(--sp-2) 0 var(--sp-3);
    border-radius: var(--r-md);
    border: 1px solid var(--line-strong);
    background: var(--bg-elev);
    color: var(--ink-3);
    transition:
      border-color var(--dur-fast) var(--ease),
      box-shadow var(--dur-fast) var(--ease);
  }
  .search:focus-within {
    border-color: var(--primary);
    box-shadow: 0 0 0 3px var(--focus);
  }
  .search input {
    flex: 1;
    min-width: 0;
    height: 100%;
    border: none;
    background: transparent;
    font-size: var(--fs-lg);
    color: var(--ink);
    outline: none;
  }
  .search input::placeholder {
    color: var(--ink-4);
  }
  .count {
    font-size: var(--fs-sm);
    color: var(--ink-3);
    white-space: nowrap;
    animation: fade-in var(--dur) var(--ease);
  }
  .body {
    flex: 1;
    min-height: 0;
    display: flex;
  }
  .toc {
    width: 220px;
    flex-shrink: 0;
    overflow-y: auto;
    padding: var(--sp-4) var(--sp-3) var(--sp-6) var(--sp-6);
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .cat {
    width: 100%;
    height: 32px;
    border: none;
    background: transparent;
    font-size: var(--fs-md);
    text-align: left;
  }
  .cat:disabled {
    opacity: 0.4;
  }
  .cat :global(svg) {
    color: var(--ink-3);
    flex-shrink: 0;
  }
  .cat.active :global(svg) {
    color: inherit;
  }
  .n {
    font-size: var(--fs-xs);
    color: var(--ink-3);
    font-variant-numeric: tabular-nums;
  }
  .n.faint {
    color: var(--ink-4);
  }
  .toc-foot {
    margin-top: var(--sp-3);
    padding-top: var(--sp-3);
    border-top: 1px solid var(--line);
  }
  .mdot {
    width: 15px;
    display: grid;
    place-items: center;
  }
  .mdot::before {
    content: '';
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--primary);
  }
  .scroller {
    flex: 1;
    min-width: 0;
    overflow-y: auto;
    scroll-behavior: auto;
  }
  .content {
    max-width: 860px;
    padding: var(--sp-4) var(--sp-8) var(--sp-12) var(--sp-4);
  }
  section + section {
    margin-top: var(--sp-6);
  }
  h2 {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    margin: 0;
    padding: var(--sp-3) var(--sp-6) var(--sp-2);
    font-size: var(--fs-lg);
    font-weight: var(--fw-semibold);
    color: var(--ink);
  }
  .cat-ic {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    border-radius: var(--r-xs);
    background: var(--primary-softer);
    color: var(--primary);
  }
  .link-card {
    display: flex;
    align-items: center;
    gap: var(--sp-3);
    width: calc(100% - var(--sp-6));
    margin: var(--sp-2) 0 var(--sp-2) var(--sp-6);
    padding: var(--sp-3) var(--sp-4);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    background: var(--primary-softer);
    color: var(--primary-strong);
    text-align: left;
    transition:
      background var(--dur-fast) var(--ease),
      transform var(--dur-fast) var(--ease);
  }
  .link-card:hover {
    background: var(--primary-soft);
  }
  .link-card .grow {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .link-card strong {
    font-weight: var(--fw-medium);
    color: var(--ink);
  }
  .link-card .grow span {
    font-size: var(--fs-sm);
    color: var(--ink-3);
  }
  @media (max-width: 760px) {
    .toc {
      display: none;
    }
    header {
      padding: var(--sp-5) var(--sp-5) var(--sp-3);
    }
  }
</style>
