<script lang="ts">
  import { ChevronRight, Check } from '@lucide/svelte';
  import { menu, closeMenu, type MenuItem } from '$lib/state/menu.svelte';
  import { runCommand } from '$lib/commands/registry.svelte';
  import Kbd from './Kbd.svelte';
  import ContextMenu from './ContextMenu.svelte';

  let { items = null, x = 0, y = 0, nested = false, onclose }: { items?: MenuItem[] | null; x?: number; y?: number; nested?: boolean; onclose?: () => void } = $props();

  let box: HTMLDivElement | undefined = $state();
  let pos = $state({ x: 0, y: 0 });
  let active = $state(-1);
  let sub = $state<{ i: number; x: number; y: number } | null>(null);

  const list = $derived(items ?? menu.items);
  const visible = $derived(nested ? true : menu.open);

  $effect(() => {
    void menu.gen;
    const bx = nested ? x : menu.x;
    const by = nested ? y : menu.y;
    pos = { x: bx, y: by };
    active = -1;
    sub = null;
    queueMicrotask(() => {
      if (!box) return;
      const r = box.getBoundingClientRect();
      pos = {
        x: Math.max(6, Math.min(bx, window.innerWidth - r.width - 6)),
        y: Math.max(6, Math.min(by, window.innerHeight - r.height - 6)),
      };
      if (!nested) box.focus();
    });
  });

  function activate(it: MenuItem, i: number, ev?: MouseEvent) {
    if (it.disabled || it.separator) return;
    if (it.submenu) {
      const el = (ev?.currentTarget as HTMLElement) ?? box?.children[i];
      const r = (el as HTMLElement).getBoundingClientRect();
      sub = { i, x: r.right - 4, y: r.top - 4 };
      return;
    }
    close();
    if (it.run) it.run();
    else if (it.command) void runCommand(it.command, it.args);
  }

  function close() {
    closeMenu();
    onclose?.();
  }

  function onkey(e: KeyboardEvent) {
    const n = list.length;
    if (e.key === 'Escape') {
      e.preventDefault();
      close();
    } else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      let i = active;
      for (let k = 0; k < n; k++) {
        i = (i + (e.key === 'ArrowDown' ? 1 : -1) + n) % n;
        if (!list[i].separator && !list[i].disabled) break;
      }
      active = i;
    } else if (e.key === 'Enter' || e.key === ' ' || e.key === 'ArrowRight') {
      e.preventDefault();
      if (active >= 0) activate(list[active], active);
    }
  }
</script>

{#if visible && list.length}
  {#if !nested}
    <div class="scrim" role="presentation" onpointerdown={close} oncontextmenu={(e) => (e.preventDefault(), close())}></div>
  {/if}
  <div
    bind:this={box}
    class="menu card-surface glass"
    class:nested
    role="menu"
    tabindex="-1"
    style:transform="translate({pos.x}px, {pos.y}px)"
    onkeydown={onkey}
  >
    {#each list as it, i (i)}
      {#if it.separator}
        <div class="sep"></div>
      {:else}
        <button
          class="item"
          class:danger={it.danger}
          class:active={active === i || sub?.i === i}
          disabled={it.disabled}
          role="menuitem"
          onpointerenter={(e) => {
            active = i;
            if (it.submenu) activate(it, i, e);
            else sub = null;
          }}
          onclick={(e) => activate(it, i, e)}
        >
          <span class="ic">
            {#if it.checked}
              <Check size={14} strokeWidth={2.2} />
            {:else if it.color}
              <span class="dot" style:background={it.color}></span>
            {:else if it.icon}
              <it.icon size={15} strokeWidth={1.8} />
            {/if}
          </span>
          <span class="label">{it.label}</span>
          {#if it.submenu}
            <ChevronRight size={14} />
          {:else if it.command}
            <Kbd command={it.command} />
          {/if}
        </button>
      {/if}
    {/each}
  </div>
  {#if sub && list[sub.i]?.submenu}
    <ContextMenu items={list[sub.i].submenu} x={sub.x} y={sub.y} nested onclose={close} />
  {/if}
{/if}

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 900;
  }
  .menu {
    position: fixed;
    top: 0;
    left: 0;
    z-index: 901;
    min-width: 200px;
    max-width: 320px;
    padding: 5px;
    border-radius: var(--r-md);
    animation: pop-in var(--dur) var(--ease-out);
    outline: none;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    height: 30px;
    padding: 0 10px 0 6px;
    border: none;
    background: transparent;
    border-radius: var(--r-xs);
    color: var(--ink);
    font-size: var(--fs-md);
    text-align: left;
  }
  .item.active {
    background: var(--primary-soft);
    color: var(--primary-strong);
  }
  .item:disabled {
    opacity: 0.45;
  }
  .item.danger {
    color: var(--danger);
  }
  .item.danger.active {
    background: var(--danger-soft);
  }
  .ic {
    width: 18px;
    display: grid;
    place-items: center;
    color: var(--ink-3);
  }
  .item.active .ic {
    color: inherit;
  }
  .label {
    flex: 1;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
  }
  .sep {
    height: 1px;
    margin: 5px 6px;
    background: var(--line);
  }
</style>
