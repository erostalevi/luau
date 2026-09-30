<script lang="ts">
  import { X } from '@lucide/svelte';
  import type { SettingDef } from '$lib/settings/schema';
  import { settings } from '$lib/settings/store.svelte';
  import { t } from '$lib/i18n/index.svelte';
  import { setSetting } from '../effects';

  let { def, label }: { def: SettingDef; label: string } = $props();
  const list = $derived((Array.isArray(settings.get(def.key)) ? settings.get<unknown[]>(def.key) : []).map(String));
  let text = $state('');

  function add() {
    const items = text
      .split(',')
      .map((s) => s.trim())
      .filter((s) => s && !list.includes(s));
    text = '';
    if (items.length) setSetting(def.key, [...list, ...new Set(items)]);
  }

  function remove(i: number) {
    setSetting(
      def.key,
      list.filter((_, j) => j !== i),
    );
  }
</script>

<div class="list" role="group" aria-label={label}>
  {#each list as item, i (item)}
    <span class="chip item">
      <span class="txt">{item}</span>
      <button class="x" aria-label={t('common.remove')} onclick={() => remove(i)}><X size={11} strokeWidth={2.2} /></button>
    </span>
  {/each}
  <input
    class="add"
    bind:value={text}
    placeholder={t('prefs.settings.addItem')}
    aria-label={label}
    onkeydown={(e) => {
      if (e.key === 'Enter' || e.key === ',') {
        e.preventDefault();
        add();
      } else if (e.key === 'Backspace' && !text && list.length) remove(list.length - 1);
    }}
    onblur={add}
  />
</div>

<style>
  .list {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    width: 100%;
    max-width: 520px;
    min-height: 32px;
    padding: 4px 6px;
    border-radius: var(--r-sm);
    border: 1px solid var(--line-strong);
    background: var(--bg-elev);
    transition:
      border-color var(--dur-fast) var(--ease),
      box-shadow var(--dur-fast) var(--ease);
  }
  .list:focus-within {
    border-color: var(--primary);
    box-shadow: 0 0 0 3px var(--focus);
  }
  .item {
    height: 22px;
    padding: 0 4px 0 9px;
    background: var(--primary-soft);
    color: var(--primary-strong);
    font-family: var(--font-mono);
    animation: pop-in var(--dur) var(--ease-out);
  }
  .txt {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .x {
    display: grid;
    place-items: center;
    width: 16px;
    height: 16px;
    padding: 0;
    border: none;
    border-radius: 50%;
    background: transparent;
    color: inherit;
    opacity: 0.7;
  }
  .x:hover {
    opacity: 1;
    background: var(--bg-hover);
  }
  .add {
    flex: 1;
    min-width: 120px;
    height: 22px;
    border: none;
    background: transparent;
    font-size: var(--fs-md);
    outline: none;
  }
  .add::placeholder {
    color: var(--ink-4);
  }
</style>
