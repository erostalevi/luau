<script lang="ts">
  import { fade, scale } from 'svelte/transition';
  import { ArrowRight } from '@lucide/svelte';
  import { dialogs, closeDialog } from '$lib/state/dialogs.svelte';
  import { t } from '$lib/i18n/index.svelte';

  let dontAsk = $state(false);

  function focusCancel(node: HTMLElement, cancelFocused: boolean | undefined) {
    queueMicrotask(() => {
      const target = node.querySelector<HTMLButtonElement>(cancelFocused === false ? '.confirm' : '.cancel');
      target?.focus();
    });
  }
</script>

{#each dialogs.list as d (d.id)}
  <div class="scrim" transition:fade={{ duration: 150 }} role="presentation" onpointerdown={() => closeDialog(d.id, false)}></div>
  <div
    class="dialog card-surface"
    role="alertdialog"
    aria-modal="true"
    aria-labelledby="dlg-title-{d.id}"
    tabindex="-1"
    transition:scale={{ start: 0.96, duration: 180 }}
    use:focusCancel={d.cancelFocused ?? true}
    onkeydown={(e) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        closeDialog(d.id, false);
      }
    }}
  >
    <h2 id="dlg-title-{d.id}">{d.title}</h2>
    {#if d.message}<p class="msg">{d.message}</p>{/if}
    {#if d.changes?.length}
      <div class="changes">
        {#each d.changes as c, i (i)}
          <div class="field">{c.field}</div>
          <div class="before">{c.before ?? '—'}</div>
          <ArrowRight size={14} class="muted" />
          <div class="after">{c.after ?? '—'}</div>
        {/each}
      </div>
    {/if}
    {#if d.preview}<pre class="preview">{d.preview}</pre>{/if}
    <div class="actions">
      {#if d.dontAskLabel}
        <label class="dont"><input type="checkbox" bind:checked={dontAsk} /> {d.dontAskLabel}</label>
      {/if}
      <span class="spacer"></span>
      <button class="btn cancel" onclick={() => closeDialog(d.id, false)}>{d.cancelLabel ?? t('common.cancel')}</button>
      <button class="btn confirm {d.danger ? 'danger' : 'primary'}" onclick={() => closeDialog(d.id, true, dontAsk)}>{d.confirmLabel ?? t('common.confirm')}</button>
    </div>
  </div>
{/each}

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 1200;
    background: var(--bg-overlay);
    backdrop-filter: blur(2px);
  }
  .dialog {
    position: fixed;
    z-index: 1201;
    top: 50%;
    left: 50%;
    translate: -50% -50%;
    width: min(520px, calc(100vw - 48px));
    padding: var(--sp-6);
    border-radius: var(--r-xl);
    outline: none;
  }
  h2 {
    margin: 0 0 var(--sp-2);
    font-size: var(--fs-xl);
    font-weight: var(--fw-semibold);
  }
  .msg {
    margin: 0 0 var(--sp-4);
    color: var(--ink-2);
    line-height: 1.55;
    user-select: text;
  }
  .changes {
    display: grid;
    grid-template-columns: auto 1fr auto 1fr;
    align-items: center;
    gap: 8px 12px;
    padding: var(--sp-3) var(--sp-4);
    margin-bottom: var(--sp-4);
    border-radius: var(--r-md);
    background: var(--bg-sunken);
    font-size: var(--fs-md);
  }
  .field {
    color: var(--ink-3);
    font-weight: var(--fw-medium);
  }
  .before {
    color: var(--ink-3);
    text-decoration: line-through;
    text-decoration-color: var(--ink-4);
    overflow-wrap: anywhere;
  }
  .after {
    color: var(--ink);
    font-weight: var(--fw-medium);
    overflow-wrap: anywhere;
  }
  .preview {
    max-height: 240px;
    overflow: auto;
    padding: var(--sp-3);
    border-radius: var(--r-md);
    background: var(--bg-sunken);
    font-family: var(--font-mono);
    font-size: var(--fs-sm);
    white-space: pre-wrap;
    user-select: text;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
  }
  .spacer {
    flex: 1;
  }
  .dont {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: var(--fs-sm);
    color: var(--ink-3);
  }
</style>
