<script lang="ts">
  import { fade, scale } from 'svelte/transition';
  import ColorPicker from './ColorPicker.svelte';
  import { colorDialog, closeColorDialog } from '$lib/state/colorDialog.svelte';
  import { t } from '$lib/i18n/index.svelte';
</script>

{#if colorDialog.open}
  <div class="scrim" role="presentation" transition:fade={{ duration: 120 }} onpointerdown={() => closeColorDialog(false)}></div>
  <div
    class="dlg card-surface"
    role="dialog"
    aria-modal="true"
    tabindex="-1"
    transition:scale={{ start: 0.96, duration: 160 }}
    onkeydown={(e) => {
      if (e.key === 'Escape') closeColorDialog(false);
      if (e.key === 'Enter') closeColorDialog(true);
    }}
  >
    <h3>{colorDialog.title}</h3>
    <ColorPicker bind:value={colorDialog.value} onchange={(v) => colorDialog.live?.(v)} />
    <div class="actions">
      <button class="btn" onclick={() => closeColorDialog(false)}>{t('common.cancel')}</button>
      <button class="btn primary" onclick={() => closeColorDialog(true)}>{t('common.apply')}</button>
    </div>
  </div>
{/if}

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 1300;
    background: var(--bg-overlay);
  }
  .dlg {
    position: fixed;
    z-index: 1301;
    top: 50%;
    left: 50%;
    translate: -50% -50%;
    padding: var(--sp-5);
    border-radius: var(--r-xl);
    outline: none;
  }
  h3 {
    margin: 0 0 var(--sp-4);
    font-size: var(--fs-lg);
    font-weight: var(--fw-semibold);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: var(--sp-4);
  }
</style>
