<script lang="ts">
  // Local AI status at the top of Settings › AI: the writer in use and why.
  import { Sparkles, RefreshCw, Circle } from '@lucide/svelte';
  import { ai, type AiStatus } from '$lib/summaries/api';
  import { describeStatus } from '$lib/summaries/aiStatus';
  import { settings } from '$lib/settings/store.svelte';
  import { tip } from '$lib/components/tooltip';
  import { t } from '$lib/i18n/index.svelte';

  let status = $state<AiStatus | null>(null);
  let busy = $state(false);
  const view = $derived(status ? describeStatus(status) : null);

  async function refresh() {
    busy = true;
    status = await ai.status().catch(() => null);
    busy = false;
  }

  // Re-check when the provider / address / model settings change.
  $effect(() => {
    void settings.get('ai.provider');
    void settings.get('ai.endpoint');
    void settings.get('ai.model');
    void refresh();
  });
</script>

<div class="ai-card" aria-live="polite">
  <Sparkles size={18} strokeWidth={1.7} />
  <span class="grow">
    <span class="k">{t('ai.writer')}</span>
    {#if view && status}
      <strong>
        <span class="dot" class:ok={view.ok}><Circle size={8} strokeWidth={0} fill="currentColor" /></span>
        {t(`ai.providerName.${view.provider}`)}{#if view.ok && status.model && view.provider !== 'apple'}&nbsp;· {status.model}{/if}
      </strong>
      {#if view.hint}<span class="hint">{t(view.hint)}</span>{/if}
      {#if view.ok && status.contextSize}<span class="hint">{t('ai.contextSize', { n: status.contextSize })}</span>{/if}
    {:else}
      <strong>{t('ai.testing')}</strong>
    {/if}
  </span>
  <button class="icon-btn sm" aria-label={t('ai.checkAgain')} use:tip={t('ai.checkAgain')} onclick={refresh} disabled={busy}>
    <RefreshCw size={14} strokeWidth={1.8} />
  </button>
</div>

<style>
  .ai-card {
    display: flex;
    align-items: flex-start;
    gap: var(--sp-3);
    width: calc(100% - var(--sp-6));
    margin: var(--sp-2) 0 var(--sp-2) var(--sp-6);
    padding: var(--sp-3) var(--sp-4);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    background: var(--primary-softer);
    color: var(--primary-strong);
  }
  .grow {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .k {
    font-size: var(--fs-sm);
    color: var(--ink-3);
  }
  strong {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    font-weight: var(--fw-medium);
    color: var(--ink);
  }
  .dot {
    display: inline-flex;
    color: var(--ink-4);
  }
  .dot.ok {
    color: var(--ok);
  }
  .hint {
    font-size: var(--fs-sm);
    color: var(--ink-2);
  }
</style>
