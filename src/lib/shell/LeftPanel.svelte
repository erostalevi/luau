<script lang="ts">
  import { FolderTree, Search, History, Plug, Blocks, PanelLeftClose } from '@lucide/svelte';
  import Segmented from '$lib/components/Segmented.svelte';
  import { ui, persistUi, type PanelSection } from '$lib/state/ui.svelte';
  import { t } from '$lib/i18n/index.svelte';
  import { tip } from '$lib/components/tooltip';
  import { isMac } from '$lib/keybindings/keys';
  import { isTauri } from '$lib/backend/rpc';
  import Explorer from '$lib/panels/explorer/Explorer.svelte';
  import SearchPanel from '$lib/panels/search/SearchPanel.svelte';
  const HistoryPanel = () => import('$lib/panels/history/HistoryPanel.svelte');
  const IntegrationsPanel = () => import('$lib/panels/integrations/IntegrationsPanel.svelte');
  import ExtensionsPanel from '$lib/panels/extensions/ExtensionsPanel.svelte';

  const sections = $derived([
    { value: 'explorer' as PanelSection, icon: FolderTree, title: t('panels.explorer') },
    { value: 'search' as PanelSection, icon: Search, title: t('panels.search') },
    { value: 'history' as PanelSection, icon: History, title: t('panels.history') },
    { value: 'integrations' as PanelSection, icon: Plug, title: t('panels.integrations') },
    { value: 'extensions' as PanelSection, icon: Blocks, title: t('panels.extensions') },
  ]);

  const macChrome = isTauri && isMac;

  function startResize(e: PointerEvent) {
    const startX = e.clientX;
    const startW = ui.left.width;
    const move = (ev: PointerEvent) => {
      ui.left.width = Math.round(Math.min(520, Math.max(220, startW + ev.clientX - startX)));
    };
    const up = () => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
      document.documentElement.style.cursor = '';
      persistUi();
    };
    document.documentElement.style.cursor = 'col-resize';
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
  }
</script>

<aside class="left" style:width="{ui.left.width}px">
  <div class="top drag-region" class:mac={macChrome}>
    <span class="grow"></span>
    <button class="icon-btn no-drag" onclick={() => ((ui.left.visible = false), persistUi())} use:tip={{ text: t('panels.hide'), command: 'panel.toggle' }}>
      <PanelLeftClose size={16} strokeWidth={1.8} />
    </button>
  </div>
  <div class="selector">
    <Segmented options={sections} bind:value={ui.left.section} onchange={() => persistUi()} full />
  </div>
  <div class="content" data-scroll-scope="left">
    {#if ui.left.section === 'explorer'}
      <Explorer />
    {:else if ui.left.section === 'search'}
      <SearchPanel />
    {:else if ui.left.section === 'history'}
      {#await HistoryPanel() then m}<m.default />{/await}
    {:else if ui.left.section === 'integrations'}
      {#await IntegrationsPanel() then m}<m.default />{/await}
    {:else}
      <ExtensionsPanel />
    {/if}
  </div>
  <div class="resize" role="separator" aria-orientation="vertical" onpointerdown={startResize}></div>
</aside>

<style>
  .left {
    position: relative;
    display: flex;
    flex-direction: column;
    flex-shrink: 0;
    height: 100%;
    background: var(--bg-sidebar);
    border-right: 1px solid var(--line);
    min-width: 220px;
  }
  :global(:root[data-glass='off']) .left {
    background: var(--bg-sunken);
  }
  .top {
    display: flex;
    align-items: center;
    height: var(--titlebar-h);
    padding: 0 8px;
    flex-shrink: 0;
  }
  .top.mac {
    padding-left: 84px;
  }
  .grow {
    flex: 1;
  }
  .selector {
    padding: 2px 10px 8px;
  }
  .content {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .resize {
    position: absolute;
    top: 0;
    right: -3px;
    width: 6px;
    height: 100%;
    cursor: col-resize;
    z-index: 5;
  }
  .resize:hover {
    background: linear-gradient(90deg, transparent 2px, var(--primary-ring) 2px, var(--primary-ring) 4px, transparent 4px);
  }
</style>
