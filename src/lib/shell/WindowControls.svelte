<script lang="ts">
  // Custom window controls (Windows only; macOS uses native traffic lights,
  // Linux uses native decorations).
  import { Minus, Square, X, Copy } from '@lucide/svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { onMount } from 'svelte';

  let maximized = $state(false);
  const win = getCurrentWindow();

  onMount(() => {
    void win.isMaximized().then((m) => (maximized = m));
    const un = win.onResized(async () => (maximized = await win.isMaximized()));
    return () => void un.then((f) => f());
  });
</script>

<div class="wc no-drag">
  <button onclick={() => win.minimize()} aria-label="Minimize"><Minus size={15} strokeWidth={1.5} /></button>
  <button onclick={() => win.toggleMaximize()} aria-label="Maximize">
    {#if maximized}<Copy size={13} strokeWidth={1.5} />{:else}<Square size={12} strokeWidth={1.5} />{/if}
  </button>
  <button class="close" onclick={() => win.close()} aria-label="Close"><X size={16} strokeWidth={1.5} /></button>
</div>

<style>
  .wc {
    display: flex;
    height: var(--titlebar-h);
    margin-left: 6px;
  }
  button {
    width: 46px;
    height: 100%;
    display: grid;
    place-items: center;
    border: none;
    background: transparent;
    color: var(--ink-2);
  }
  button:hover {
    background: var(--bg-hover);
  }
  .close:hover {
    background: #e81123;
    color: #fff;
  }
</style>
