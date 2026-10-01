<script lang="ts">
  import { fade, scale } from 'svelte/transition';
  import { ui } from '$lib/state/ui.svelte';
  import { getCommand, commandTitle } from '$lib/commands/registry.svelte';
  import { primaryKey } from '$lib/keybindings/resolver.svelte';
  import Kbd from '$lib/components/Kbd.svelte';
  import { trapFocus } from '$lib/components/focusTrap';
  import { t } from '$lib/i18n/index.svelte';

  const groups: { id: string; commands: string[] }[] = [
    { id: 'navigation', commands: ['palette.commands', 'palette.quickOpen', 'nav.down', 'nav.right', 'card.open', 'board.filter', 'panel.search'] },
    { id: 'cards', commands: ['card.new', 'lane.new', 'card.moveUp', 'card.moveRight', 'card.indent', 'card.outdent', 'card.archive', 'card.delete', 'edit.undo', 'edit.redo'] },
    { id: 'editor', commands: ['editor.bold', 'editor.italic', 'editor.insertLink', 'editor.insertCardLink', 'editor.toggleChecklist', 'editor.togglePreview', 'editor.pastePlain'] },
    { id: 'windows', commands: ['board.openPicker', 'tab.close', 'tab.reopen', 'tab.next', 'view.splitRight', 'window.new', 'view.zoomIn'] },
    { id: 'panels', commands: ['panel.toggle', 'panel.explorer', 'panel.history', 'panel.integrations', 'app.openSettings', 'app.openKeybindings'] },
  ];
</script>

{#if ui.cheatsheet}
  <div class="scrim" role="presentation" transition:fade={{ duration: 140 }} onpointerdown={() => (ui.cheatsheet = false)}></div>
  <div
    class="sheet card-surface"
    data-overlay
    role="dialog"
    aria-modal="true"
    tabindex="-1"
    transition:scale={{ start: 0.97, duration: 180 }}
    use:trapFocus={{ initial: null }}
    onkeydown={(e) => e.key === 'Escape' && (ui.cheatsheet = false)}
  >
    <h2>{t('cheatsheet.title')}</h2>
    <div class="cols">
      {#each groups as g (g.id)}
        <section>
          <h3 class="section-title">{t(`cheatsheet.groups.${g.id}`)}</h3>
          {#each g.commands as id (id)}
            {@const c = getCommand(id)}
            {#if c && primaryKey(id)}
              <div class="line"><span>{commandTitle(c)}</span><Kbd command={id} /></div>
            {/if}
          {/each}
        </section>
      {/each}
    </div>
  </div>
{/if}

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 1000;
    background: var(--bg-overlay);
  }
  .sheet {
    position: fixed;
    z-index: 1001;
    top: 50%;
    left: 50%;
    translate: -50% -50%;
    width: min(880px, calc(100vw - 48px));
    max-height: calc(100vh - 80px);
    overflow: auto;
    padding: 28px;
    border-radius: var(--r-xl);
    outline: none;
  }
  h2 {
    margin: 0 0 18px;
    font-size: var(--fs-xl);
  }
  .cols {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(250px, 1fr));
    gap: 24px;
  }
  .line {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    padding: 5px 0;
    font-size: var(--fs-sm);
    color: var(--ink-2);
  }
</style>
