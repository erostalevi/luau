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

  // Full-width segments share the row equally; when a label no longer fits its
  // segment, segments with an icon drop to icon-only (the label stays as the
  // tooltip and accessible name). Measured, so it adapts to any locale.
  let el = $state<HTMLDivElement>();
  let compact = $state(false);
  const canCompact = $derived(full && options.length > 0 && options.every((o) => o.icon && o.label));

  function measure() {
    if (!el) return;
    compact = [...el.querySelectorAll<HTMLElement>(':scope > button')].some((b) => {
      const label = b.querySelector<HTMLElement>('.label');
      const icon = b.querySelector<SVGElement>('svg');
      if (!label) return false;
      const cs = getComputedStyle(b);
      const need =
        label.scrollWidth +
        (icon ? icon.getBoundingClientRect().width + (parseFloat(cs.columnGap) || 0) : 0) +
        parseFloat(cs.paddingLeft) +
        parseFloat(cs.paddingRight);
      return need > b.clientWidth + 0.5;
    });
  }

  $effect(() => {
    void options.map((o) => o.label);
    if (!canCompact || !el || typeof ResizeObserver === 'undefined') {
      compact = false;
      return;
    }
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    el.querySelectorAll('.label').forEach((l) => ro.observe(l));
    measure();
    return () => ro.disconnect();
  });
</script>

<div bind:this={el} class="seg {size}" class:full class:compact role="tablist" style:--n={options.length} style:--i={idx}>
  <span class="thumb" aria-hidden="true"></span>
  {#each options as o (o.value)}
    <button
      role="tab"
      aria-selected={o.value === value}
      class:on={o.value === value}
      title={o.title ?? o.label}
      aria-label={o.label ?? o.title}
      onclick={() => {
        value = o.value;
        onchange?.(o.value);
      }}
    >
      {#if o.icon}<o.icon size={size === 'sm' ? 14 : 15} strokeWidth={1.9} />{/if}
      {#if o.label}<span class="label">{o.label}</span>{/if}
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
    grid-template-columns: repeat(var(--n), minmax(0, 1fr));
    width: 100%;
    min-width: 0;
  }
  .full button {
    position: relative;
    min-width: 0;
    overflow: hidden;
  }
  .full button :global(svg) {
    flex-shrink: 0;
  }
  .full .label {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  /* Icon-only: the label leaves the flow but keeps its natural width so it can be re-measured. */
  .compact .label {
    position: absolute;
    left: 0;
    top: 0;
    opacity: 0;
    pointer-events: none;
    overflow: visible;
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
