<script lang="ts">
  // Before/after preview for "Change with AI…".
  import { fade, scale } from 'svelte/transition';
  import { Sparkles, RotateCcw, Columns2, AlignLeft, LoaderCircle } from '@lucide/svelte';
  import { aiChange, acceptChange, closeChange, retry } from './change.svelte';
  import { trapFocus } from '$lib/components/focusTrap';
  import { t } from '$lib/i18n/index.svelte';

  let side = $state(false);
  let sheet = $state<HTMLElement>();

  // Ready to apply: put focus on Replace so Enter accepts.
  $effect(() => {
    if (aiChange.open && !aiChange.streaming && aiChange.after && !aiChange.error) sheet?.querySelector<HTMLButtonElement>('.primary')?.focus();
  });

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      closeChange();
    } else if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      acceptChange(e.shiftKey ? 'below' : 'replace');
    }
  }
</script>

{#if aiChange.open}
  <div class="scrim" transition:fade={{ duration: 150 }} role="presentation" onpointerdown={closeChange}></div>
  <div
    class="sheet card-surface"
    data-overlay
    role="dialog"
    aria-modal="true"
    aria-labelledby="aichange-title"
    tabindex="-1"
    transition:scale={{ start: 0.96, duration: 180 }}
    bind:this={sheet}
    use:trapFocus={{ initial: null }}
    {onkeydown}
  >
    <header>
      <Sparkles size={16} strokeWidth={1.8} />
      <h2 id="aichange-title">{aiChange.wholeCard ? t('aiChange.titleWhole') : t('aiChange.title')}</h2>
      <span class="spacer"></span>
      <button
        class="icon-btn sm"
        class:active={side}
        aria-pressed={side}
        aria-label={t('aiChange.sideBySide')}
        title={t('aiChange.sideBySide')}
        onclick={() => (side = !side)}
      >
        {#if side}<AlignLeft size={14} />{:else}<Columns2 size={14} />{/if}
      </button>
    </header>
    <p class="instr">“{aiChange.instruction}”</p>

    {#if aiChange.error}
      <p class="error" role="alert">{t('aiChange.failed', { message: aiChange.error })}</p>
    {:else if side || aiChange.streaming}
      <div class="cols" class:single={!side}>
        {#if side}
          <div class="col">
            <span class="label">{t('aiChange.before')}</span>
            <div class="text before">{aiChange.before}</div>
          </div>
        {/if}
        <div class="col">
          <span class="label"
            >{t('aiChange.after')}{#if aiChange.streaming}<LoaderCircle size={12} class="spin" />{/if}</span
          >
          <div class="text" aria-live="polite">{aiChange.after || '…'}</div>
        </div>
      </div>
    {:else}
      <div class="text diff" aria-label={t('aiChange.changes')}>
        {#each aiChange.diff as d, i (i)}
          {#if d.op === 'eq'}<span>{d.text}</span>{:else if d.op === 'del'}<del>{d.text}</del>{:else}<ins>{d.text}</ins>{/if}
        {/each}
      </div>
    {/if}

    <footer>
      {#if aiChange.model}<span class="model">{aiChange.model}</span>{/if}
      <span class="spacer"></span>
      <button class="btn" onclick={closeChange}>{t('common.cancel')}</button>
      <button class="btn" onclick={retry} disabled={aiChange.streaming}><RotateCcw size={13} />{t('aiChange.retry')}</button>
      <button class="btn" onclick={() => acceptChange('below')} disabled={aiChange.streaming || !aiChange.after || !!aiChange.error}
        >{t('aiChange.insertBelow')}</button
      >
      <button class="btn primary" onclick={() => acceptChange('replace')} disabled={aiChange.streaming || !aiChange.after || !!aiChange.error}
        >{t('aiChange.replace')}</button
      >
    </footer>
  </div>
{/if}

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 1200;
    background: var(--bg-overlay);
    backdrop-filter: blur(2px);
  }
  .sheet {
    position: fixed;
    z-index: 1201;
    top: 50%;
    left: 50%;
    translate: -50% -50%;
    display: flex;
    flex-direction: column;
    gap: var(--sp-3);
    width: min(760px, calc(100vw - 48px));
    max-height: calc(100vh - 96px);
    padding: var(--sp-5) var(--sp-6);
    border-radius: var(--r-xl);
    outline: none;
  }
  header {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    color: var(--primary-strong);
  }
  h2 {
    margin: 0;
    font-size: var(--fs-lg);
    font-weight: var(--fw-semibold);
    color: var(--ink);
  }
  .spacer {
    flex: 1;
  }
  .instr {
    margin: 0;
    color: var(--ink-2);
    font-style: italic;
    overflow-wrap: anywhere;
  }
  .cols {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--sp-3);
    min-height: 0;
  }
  .cols.single {
    grid-template-columns: 1fr;
  }
  .col {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
    min-height: 0;
  }
  .label {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: var(--fs-sm);
    color: var(--ink-3);
  }
  .text {
    flex: 1;
    min-height: 80px;
    max-height: 52vh;
    overflow: auto;
    padding: var(--sp-3) var(--sp-4);
    border-radius: var(--r-md);
    background: var(--bg-sunken);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    line-height: 1.55;
    user-select: text;
  }
  .text.before {
    color: var(--ink-2);
  }
  del {
    color: var(--danger);
    background: var(--danger-soft);
    text-decoration-color: color-mix(in srgb, var(--danger) 60%, transparent);
    border-radius: 3px;
  }
  ins {
    color: var(--ok);
    background: color-mix(in srgb, var(--ok) 14%, transparent);
    text-decoration: none;
    border-radius: 3px;
  }
  .error {
    margin: 0;
    padding: var(--sp-3) var(--sp-4);
    border-radius: var(--r-md);
    background: var(--danger-soft);
    color: var(--danger);
  }
  footer {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
  }
  footer .btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .model {
    font-size: var(--fs-sm);
    color: var(--ink-3);
  }
  :global(.sheet .spin) {
    animation: spin 1s linear infinite;
  }
</style>
