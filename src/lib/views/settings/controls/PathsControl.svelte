<script lang="ts">
  import { Folder, Plus, X } from '@lucide/svelte';
  import type { SettingDef } from '$lib/settings/schema';
  import { settings } from '$lib/settings/store.svelte';
  import { pickFolder } from '$lib/app/helpers';
  import { app } from '$lib/app/bootstrap';
  import { tip } from '$lib/components/tooltip';
  import { t } from '$lib/i18n/index.svelte';
  import { setSetting } from '../effects';

  let { def, label, placeholder }: { def: SettingDef; label: string; placeholder?: string } = $props();
  const list = $derived((settings.get<string[]>(def.key) ?? []).filter((p) => typeof p === 'string'));

  async function add() {
    const p = await pickFolder(t('prefs.settings.addFolder'), app.info?.home ?? undefined);
    if (p && !list.includes(p)) setSetting(def.key, [...list, p]);
  }

  function remove(p: string) {
    setSetting(
      def.key,
      list.filter((x) => x !== p),
    );
  }
</script>

<div class="paths" role="group" aria-label={label}>
  {#each list as p (p)}
    <div class="path">
      <Folder size={14} strokeWidth={1.8} />
      <span class="grow mono" title={p}>{p}</span>
      <button class="icon-btn sm" aria-label={t('common.remove')} use:tip={t('common.remove')} onclick={() => remove(p)}>
        <X size={13} strokeWidth={2} />
      </button>
    </div>
  {:else}
    {#if placeholder}<div class="hint">{placeholder}</div>{/if}
  {/each}
  <button class="btn sm soft add" onclick={add}><Plus size={14} strokeWidth={2} />{t('prefs.settings.addFolder')}</button>
</div>

<style>
  .paths {
    display: flex;
    flex-direction: column;
    gap: 6px;
    width: 100%;
    max-width: 520px;
  }
  .path {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    height: 32px;
    padding: 0 6px 0 var(--sp-3);
    border-radius: var(--r-sm);
    background: var(--bg-sunken);
    color: var(--ink-3);
    animation: pop-in var(--dur) var(--ease-out);
  }
  .grow {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--ink);
    font-family: var(--font-mono);
    font-size: var(--fs-sm);
    direction: rtl;
    text-align: left;
  }
  .hint {
    font-size: var(--fs-sm);
    color: var(--ink-3);
  }
  .add {
    align-self: flex-start;
  }
</style>
