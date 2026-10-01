<script lang="ts">
  import type { SettingDef } from '$lib/settings/schema';
  import { settings } from '$lib/settings/store.svelte';
  import { t } from '$lib/i18n/index.svelte';
  import { setSetting } from '../effects';
  import { fonts, loadFonts } from './fonts.svelte';

  let { def, label }: { def: SettingDef; label: string } = $props();
  const value = $derived(settings.get<string>(def.key) || 'default');
  const custom = $derived(value !== 'default' && value !== 'system' && !fonts.list.includes(value) ? value : null);
</script>

<select
  class="field"
  aria-label={label}
  {value}
  onfocus={loadFonts}
  onpointerdown={loadFonts}
  onchange={(e) => setSetting(def.key, e.currentTarget.value)}
  style:font-family={value === 'default' || value === 'system' ? undefined : `'${value.replace(/'/g, '')}'`}
>
  <option value="default">{t('fonts.default')}</option>
  <option value="system">{t('fonts.system')}</option>
  {#if custom}<option value={custom}>{custom}</option>{/if}
  {#if fonts.list.length}
    <optgroup label={t('prefs.settings.installedFonts')}>
      {#each fonts.list as f (f)}
        <option value={f}>{f}</option>
      {/each}
    </optgroup>
  {:else if fonts.loading}
    <option disabled>{t('common.loading')}</option>
  {/if}
</select>

<style>
  select {
    max-width: 280px;
  }
</style>
