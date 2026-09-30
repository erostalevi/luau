<script lang="ts">
  import type { SettingDef } from '$lib/settings/schema';
  import { settings } from '$lib/settings/store.svelte';
  import { pickColor } from '$lib/state/colorDialog.svelte';
  import { setSetting } from '../effects';

  let { def, label }: { def: SettingDef; label: string } = $props();
  const value = $derived(settings.get<string>(def.key) || String(def.default));

  async function open() {
    const prev = value;
    const hex = await pickColor(label, prev, (v) => setSetting(def.key, v));
    setSetting(def.key, hex ?? prev);
  }
</script>

<button class="color" onclick={open} aria-label={label}>
  <span class="sw" style:background={value}></span>
  <span class="mono hex">{value}</span>
</button>

<style>
  .color {
    display: inline-flex;
    align-items: center;
    gap: var(--sp-2);
    height: 32px;
    padding: 0 var(--sp-3) 0 5px;
    border-radius: var(--r-sm);
    border: 1px solid var(--line-strong);
    background: var(--bg-elev);
    transition:
      border-color var(--dur-fast) var(--ease),
      box-shadow var(--dur-fast) var(--ease);
  }
  .color:hover {
    border-color: var(--primary);
  }
  .sw {
    width: 22px;
    height: 22px;
    border-radius: var(--r-xs);
    box-shadow: inset 0 0 0 1px rgb(0 0 0 / 0.08);
  }
  .hex {
    font-family: var(--font-mono);
    font-size: var(--fs-sm);
    color: var(--ink-2);
  }
</style>
