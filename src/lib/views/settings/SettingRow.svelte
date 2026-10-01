<script lang="ts">
  import { RotateCcw, Puzzle, RefreshCw } from '@lucide/svelte';
  import type { SettingDef } from '$lib/settings/schema';
  import { settings } from '$lib/settings/store.svelte';
  import Toggle from '$lib/components/Toggle.svelte';
  import Segmented from '$lib/components/Segmented.svelte';
  import { tip } from '$lib/components/tooltip';
  import { openMenu } from '$lib/state/menu.svelte';
  import { copyText } from '$lib/app/helpers';
  import { toast } from '$lib/state/toasts.svelte';
  import { t } from '$lib/i18n/index.svelte';
  import { settingLabel, settingDesc, optionLabel, sourceLabel } from './labels';
  import { setSetting, resetSetting } from './effects';
  import NumberControl from './controls/NumberControl.svelte';
  import ColorControl from './controls/ColorControl.svelte';
  import FontControl from './controls/FontControl.svelte';
  import PathsControl from './controls/PathsControl.svelte';
  import ListControl from './controls/ListControl.svelte';
  import TextControl from './controls/TextControl.svelte';

  let { def, showKey = false }: { def: SettingDef; showKey?: boolean } = $props();

  const label = $derived(settingLabel(def));
  const desc = $derived(settingDesc(def));
  const modified = $derived(settings.isModified(def.key));
  const source = $derived(sourceLabel(def.source));
  const wide = $derived(def.type === 'paths' || def.type === 'list');
  const enumOptions = $derived((def.options ?? []).map((o) => ({ value: o, label: optionLabel(def, o) })));

  function menu(e: MouseEvent) {
    openMenu(e, [
      { label: t('prefs.settings.reset'), icon: RotateCcw, disabled: !modified, run: () => resetSetting(def.key) },
      { separator: true },
      { label: t('prefs.settings.copyId'), run: () => void copyText(def.key).then(() => toast.success(t('prefs.copied'))) },
      {
        label: t('prefs.settings.copyJson'),
        run: () => void copyText(`"${def.key}": ${JSON.stringify(settings.get(def.key))}`).then(() => toast.success(t('prefs.copied'))),
      },
    ]);
  }
</script>

<div class="setting" class:modified class:wide id="setting-{def.key}" role="group" aria-label={label} oncontextmenu={menu}>
  <span class="dot" aria-hidden="true" use:tip={modified ? t('common.modified') : null}></span>
  <div class="text">
    <div class="head">
      <span class="label">{label}</span>
      {#if source}
        <span class="chip badge" use:tip={t('prefs.settings.contributedBy', { source })}><Puzzle size={11} strokeWidth={2} />{source}</span>
      {/if}
      {#if def.restart}
        <span class="chip badge warn" use:tip={t('prefs.settings.restartTip')}><RefreshCw size={11} strokeWidth={2} />{t('prefs.settings.restart')}</span>
      {/if}
      {#if showKey}<span class="key mono">{def.key}</span>{/if}
    </div>
    {#if desc}<p class="desc">{desc}</p>{/if}
  </div>
  <div class="control">
    <button
      class="icon-btn sm reset"
      class:show={modified}
      tabindex={modified ? 0 : -1}
      aria-label={t('prefs.settings.reset')}
      use:tip={t('prefs.settings.resetTo', { value: JSON.stringify(def.default) })}
      onclick={() => resetSetting(def.key)}
    >
      <RotateCcw size={13} strokeWidth={2} />
    </button>
    {#if def.type === 'boolean'}
      <Toggle checked={Boolean(settings.get(def.key))} {label} onchange={(v) => setSetting(def.key, v)} />
    {:else if def.type === 'enum'}
      {#if enumOptions.length <= 4}
        <Segmented options={enumOptions} value={String(settings.get(def.key))} onchange={(v) => setSetting(def.key, v)} size="sm" />
      {:else}
        <select class="field" aria-label={label} value={String(settings.get(def.key))} onchange={(e) => setSetting(def.key, e.currentTarget.value)}>
          {#each enumOptions as o (o.value)}<option value={o.value}>{o.label}</option>{/each}
        </select>
      {/if}
    {:else if def.type === 'number'}
      <NumberControl {def} {label} />
    {:else if def.type === 'color'}
      <ColorControl {def} {label} />
    {:else if def.type === 'font'}
      <FontControl {def} {label} />
    {:else if def.type === 'paths'}
      <PathsControl {def} {label} placeholder={def.key === 'discovery.roots' ? t('prefs.settings.rootsEmpty') : undefined} />
    {:else if def.type === 'list'}
      <ListControl {def} {label} />
    {:else}
      <TextControl {def} {label} />
    {/if}
  </div>
</div>

<style>
  .setting {
    position: relative;
    /* Text and control share a line while the text keeps ≥ 15rem; otherwise the
       control wraps below instead of squeezing the description. */
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--sp-2) var(--sp-6);
    padding: var(--sp-4) var(--sp-5) var(--sp-4) var(--sp-6);
    border-radius: var(--r-md);
    transition: background var(--dur-fast) var(--ease);
  }
  .setting:hover,
  .setting:focus-within {
    background: var(--bg-hover);
  }
  .setting.wide {
    flex-direction: column;
    align-items: stretch;
  }
  .dot {
    position: absolute;
    left: 10px;
    top: calc(var(--sp-4) + 7px);
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--primary);
    opacity: 0;
    transform: scale(0.4);
    transition:
      opacity var(--dur) var(--ease),
      transform var(--dur) var(--ease-spring);
  }
  .modified .dot {
    opacity: 1;
    transform: scale(1);
  }
  .text {
    flex: 1 1 15rem;
    min-width: 0;
  }
  .head {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px var(--sp-2);
  }
  .label {
    font-weight: var(--fw-medium);
    color: var(--ink);
  }
  .key {
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    color: var(--ink-4);
  }
  .badge {
    height: 18px;
    padding: 0 7px;
    gap: 4px;
    background: var(--secondary);
    color: var(--secondary-ink);
  }
  .badge.warn {
    background: var(--warn-soft);
    color: var(--warn);
  }
  .desc {
    margin: 3px 0 0;
    color: var(--ink-3);
    font-size: var(--fs-sm);
    line-height: 1.55;
    max-width: 62ch;
  }
  .control {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: var(--sp-2);
    min-width: 0;
    max-width: 100%;
    margin-left: auto;
  }
  .wide .control {
    justify-content: flex-start;
    align-items: flex-start;
  }
  .wide .reset {
    order: 2;
  }
  .reset {
    opacity: 0;
    pointer-events: none;
    transition: opacity var(--dur-fast) var(--ease);
  }
  .reset.show {
    opacity: 1;
    pointer-events: auto;
  }
  select.field {
    width: auto;
    min-width: 180px;
  }
</style>
