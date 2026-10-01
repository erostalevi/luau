<script lang="ts">
  import { PanelLeftOpen } from '@lucide/svelte';
  import LeftPanel from '$lib/shell/LeftPanel.svelte';
  import TabStrip from '$lib/shell/TabStrip.svelte';
  import PaneView from '$lib/shell/PaneView.svelte';
  import TopBarActions from '$lib/shell/TopBarActions.svelte';
  import WindowControls from '$lib/shell/WindowControls.svelte';
  import QuickInput from '$lib/quickinput/QuickInput.svelte';
  import ContextMenu from '$lib/components/ContextMenu.svelte';
  import Toasts from '$lib/components/Toasts.svelte';
  import Dialogs from '$lib/components/Dialogs.svelte';
  import ColorDialog from '$lib/components/ColorDialog.svelte';
  import CardEditorHost from '$lib/editor/CardEditorHost.svelte';
  import Cheatsheet from '$lib/views/Cheatsheet.svelte';
  import FirstRun from '$lib/views/FirstRun.svelte';
  import { ws } from '$lib/state/workspace.svelte';
  import { ui, persistUi } from '$lib/state/ui.svelte';
  import { isMac, isWindows } from '$lib/keybindings/keys';
  import { isTauri } from '$lib/backend/rpc';
  import { tip } from '$lib/components/tooltip';
  import { t } from '$lib/i18n/index.svelte';

  const macChrome = isTauri && isMac;
  const winChrome = isTauri && isWindows;
</script>

<div class="app" class:mac={macChrome}>
  <div class="body">
    {#if ui.left.visible}
      <LeftPanel />
    {/if}
    <main class="panes">
      {#each ws.panes as pane, i (pane.id)}
        <section class="pane" class:focused={ws.activePane === pane.id && ws.panes.length > 1}>
          <div class="pane-top">
            {#if i === 0 && !ui.left.visible}
              <div class="reopen drag-region" data-tauri-drag-region="deep" class:mac={macChrome}>
                <button
                  class="icon-btn no-drag"
                  onclick={() => ((ui.left.visible = true), persistUi())}
                  use:tip={{ text: t('panels.show'), command: 'panel.toggle' }}
                >
                  <PanelLeftOpen size={16} strokeWidth={1.8} />
                </button>
              </div>
            {/if}
            <TabStrip {pane}>
              {#snippet end()}
                {#if i === ws.panes.length - 1}
                  <TopBarActions />
                  {#if winChrome}<WindowControls />{/if}
                {/if}
              {/snippet}
            </TabStrip>
          </div>
          <PaneView {pane} />
        </section>
      {/each}
    </main>
    <CardEditorHost />
  </div>
</div>

<QuickInput />
<ContextMenu />
<Dialogs />
<ColorDialog />
<Toasts />
<Cheatsheet />
<FirstRun />

<style>
  .app {
    height: 100%;
    display: flex;
    flex-direction: column;
  }
  .body {
    flex: 1;
    min-height: 0;
    display: flex;
    /* Containing block for the docked card editor (laid over its slot). */
    position: relative;
  }
  .panes {
    flex: 1;
    min-width: 0;
    display: flex;
    background: var(--bg-glass);
  }
  :global(:root[data-glass='off']) .panes {
    background: var(--bg);
  }
  .pane {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    position: relative;
  }
  .pane + .pane {
    border-left: 1px solid var(--line);
  }
  .pane.focused::after {
    content: '';
    position: absolute;
    left: 0;
    right: 0;
    top: 0;
    height: 2px;
    background: var(--primary);
    opacity: 0.5;
    pointer-events: none;
  }
  .pane-top {
    display: flex;
    min-width: 0;
  }
  .reopen {
    display: flex;
    align-items: center;
    padding-left: 8px;
    height: var(--titlebar-h);
  }
  .reopen.mac {
    padding-left: 84px;
  }
</style>
