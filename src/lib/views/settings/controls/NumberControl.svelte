<script lang="ts">
  import type { SettingDef } from '$lib/settings/schema';
  import { settings } from '$lib/settings/store.svelte';
  import { setSetting } from '../effects';

  let { def, label }: { def: SettingDef; label: string } = $props();

  const value = $derived(Number(settings.get<number>(def.key)));
  const hasRange = $derived(def.min !== undefined && def.max !== undefined);
  const decimals = $derived(String(def.step ?? 1).split('.')[1]?.length ?? 0);
  const pct = $derived(hasRange ? ((value - def.min!) / (def.max! - def.min!)) * 100 : 0);
  let draft = $state<string | null>(null);

  function clamp(n: number): number {
    let v = n;
    if (def.min !== undefined) v = Math.max(def.min, v);
    if (def.max !== undefined) v = Math.min(def.max, v);
    if (def.step) v = Math.round(v / def.step) * def.step;
    return Number(v.toFixed(decimals));
  }

  function commit(raw: string) {
    draft = null;
    const n = Number(raw);
    if (raw.trim() === '' || !Number.isFinite(n)) return;
    setSetting(def.key, clamp(n));
  }
</script>

<div class="num">
  {#if hasRange}
    <input
      class="slider"
      type="range"
      min={def.min}
      max={def.max}
      step={def.step ?? 1}
      {value}
      style:--pct="{pct}%"
      aria-label={label}
      oninput={(e) => setSetting(def.key, clamp(Number(e.currentTarget.value)))}
    />
  {/if}
  <input
    class="field mono"
    type="number"
    min={def.min}
    max={def.max}
    step={def.step ?? 1}
    value={draft ?? value.toFixed(decimals)}
    aria-label={label}
    oninput={(e) => (draft = e.currentTarget.value)}
    onchange={(e) => commit(e.currentTarget.value)}
    onkeydown={(e) => {
      if (e.key === 'Enter') commit(e.currentTarget.value);
      if (e.key === 'Escape') {
        draft = null;
        e.currentTarget.blur();
      }
    }}
  />
</div>

<style>
  .num {
    display: flex;
    align-items: center;
    gap: var(--sp-3);
    width: 100%;
    max-width: 380px;
  }
  .field {
    width: 88px;
    flex-shrink: 0;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .slider {
    flex: 1;
    min-width: 0;
    appearance: none;
    height: 4px;
    border-radius: var(--r-pill);
    background: linear-gradient(to right, var(--primary) 0 var(--pct), var(--line-strong) var(--pct) 100%);
    outline: none;
  }
  .slider::-webkit-slider-thumb {
    appearance: none;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--bg-elev);
    border: 2px solid var(--primary);
    box-shadow: var(--shadow-1);
    transition: transform var(--dur-fast) var(--ease-spring);
  }
  .slider::-webkit-slider-thumb:hover {
    transform: scale(1.15);
  }
  .slider:focus-visible::-webkit-slider-thumb {
    box-shadow: 0 0 0 4px var(--focus);
  }
  .slider::-moz-range-thumb {
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: var(--bg-elev);
    border: 2px solid var(--primary);
  }
</style>
