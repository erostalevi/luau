<script lang="ts" generics="T extends string">
  import type { Component } from 'svelte';

  interface Option {
    value: T;
    label?: string;
    icon?: Component<any>;
    title?: string;
  }

  let {
    options,
    value = $bindable(),
    onchange,
    size = 'md',
    full = false,
  }: { options: Option[]; value: T; onchange?: (v: T) => void; size?: 'sm' | 'md'; full?: boolean } = $props();

  const idx = $derived(
    Math.max(
      0,
      options.findIndex((o) => o.value === value),
    ),
  );
</script>

<div class="seg {size}" class:full role="tablist" style:--n={options.length} style:--i={idx}>
  <span class="thumb" aria-hidden="true"></span>
  {#each options as o (o.value)}
    <button
      role="tab"
      aria-selected={o.value === value}
      class:on={o.value === value}
      title={o.title ?? o.label}
      onclick={() => {
        value = o.value;
        onchange?.(o.value);
      }}
    >
      {#if o.icon}<o.icon size={size === 'sm' ? 14 : 15} strokeWidth={1.9} />{/if}
      {#if o.label}<span>{o.label}</span>{/if}
    </button>
  {/each}
</div>

<style>
  .seg {
    position: relative;
    display: inline-grid;
    grid-template-columns: repeat(var(--n), 1fr);
    padding: 3px;
    border-radius: var(--r-sm);
    background: var(--bg-hover);
    isolation: isolate;
  }
  .seg.full {
    display: grid;
    width: 100%;
  }
  .thumb {
    position: absolute;
    z-index: -1;
    top: 3px;
    bottom: 3px;
    left: 3px;
    width: calc((100% - 6px) / var(--n));
    transform: translateX(calc(var(--i) * 100%));
    border-radius: calc(var(--r-sm) - 2px);
    background: var(--bg-elev);
    box-shadow: var(--shadow-1);
    transition: transform var(--dur) var(--ease-spring);
  }
  button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    height: 26px;
    padding: 0 10px;
    border: none;
    background: transparent;
    color: var(--ink-3);
    font-size: var(--fs-sm);
    font-weight: var(--fw-medium);
    border-radius: calc(var(--r-sm) - 2px);
    transition: color var(--dur-fast) var(--ease);
    white-space: nowrap;
  }
  .sm button {
    height: 22px;
    padding: 0 8px;
  }
  button:hover {
    color: var(--ink);
  }
  button.on {
    color: var(--primary-strong);
  }
</style>
