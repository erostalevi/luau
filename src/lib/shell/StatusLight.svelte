<script lang="ts">
  // Bottom-left status light: gray idle, green active, yellow warnings, red errors.
  // All logic lives in `$lib/status/activity` (pure, unit-tested); this only renders it.
  import { CircleAlert, TriangleAlert, RefreshCw } from '@lucide/svelte';
  import { activity } from '$lib/status/activity.svelte';
  import { barMessage, issueMessage, summarize, taskMessage, type Msg } from '$lib/status/activity';
  import type { TaskIssue } from '$lib/backend/types';
  import { integrationStatus } from '$lib/integrations/status.svelte';
  import { runCommand } from '$lib/commands/registry.svelte';
  import { tip } from '$lib/components/tooltip';
  import { t } from '$lib/i18n/index.svelte';

  const extra = $derived<TaskIssue[]>(integrationStatus.offline ? [{ level: 'warn', code: 'integrationOffline', subject: integrationStatus.offline }] : []);
  const sum = $derived(summarize(activity.value, extra));
  const bar = $derived(barMessage(sum));
  const tr = (m: Msg) => t(m.key, m.params);
  const label = $derived(`${t(`status.light.${sum.light}`)}${bar ? ` · ${tr(bar)}` : ''}`);

  let open = $state(false);
  let root = $state<HTMLElement | null>(null);

  function onDocPointer(e: PointerEvent) {
    if (open && root && !root.contains(e.target as Node)) open = false;
  }
  function onKey(e: KeyboardEvent) {
    if (open && e.key === 'Escape') {
      e.stopPropagation();
      open = false;
    }
  }
</script>

<svelte:document onpointerdown={onDocPointer} />

<span class="wrap" bind:this={root}>
  <button
    class="light-btn"
    aria-label={label}
    aria-haspopup="dialog"
    aria-expanded={open}
    onclick={() => (open = !open)}
    onkeydown={onKey}
    use:tip={open ? null : { text: label, placement: 'top' }}
  >
    <span class="dot {sum.light}" class:busy={sum.busy}></span>
  </button>
  {#if bar}<span class="bar-text">{tr(bar)}</span>{/if}

  {#if open}
    <div class="pop card-surface" role="dialog" aria-label={t('status.title')} tabindex="-1" onkeydown={onKey}>
      <header>
        <span class="dot {sum.light}" class:busy={sum.busy}></span>
        <strong>{t(`status.light.${sum.light}`)}</strong>
      </header>
      <p class="desc">{t(`status.explain.${sum.light}`)}</p>

      {#if sum.running.length}
        <div class="section-title">{t('status.running')}</div>
        <ul>
          {#each sum.running as task (task.task)}
            <li class="run">{tr(taskMessage(task))}</li>
          {/each}
        </ul>
      {/if}

      <div class="section-title">{t('status.issuesTitle')}</div>
      {#if sum.issues.length}
        <ul>
          {#each sum.issues as issue (`${issue.level}|${issue.code}|${issue.subject}`)}
            <li class={issue.level}>
              {#if issue.level === 'error'}<CircleAlert size={14} strokeWidth={1.8} />{:else}<TriangleAlert size={14} strokeWidth={1.8} />{/if}
              <span>{tr(issueMessage(issue))}</span>
            </li>
          {/each}
        </ul>
      {:else}
        <p class="desc">{t('status.noIssues')}</p>
      {/if}

      <footer>
        <button
          class="btn sm soft"
          onclick={() => {
            open = false;
            void runCommand('board.rescan');
          }}
        >
          <RefreshCw size={13} strokeWidth={1.8} />
          {t('commands.board.rescan')}
        </button>
      </footer>
    </div>
  {/if}
</span>

<style>
  .wrap {
    position: relative;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }
  .light-btn {
    display: inline-grid;
    place-items: center;
    width: 18px;
    height: 18px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: transparent;
    cursor: pointer;
  }
  .light-btn:hover {
    background: var(--bg-hover);
  }
  .light-btn:focus-visible {
    outline: 2px solid var(--primary-ring);
    outline-offset: 1px;
  }
  .dot {
    --c: var(--ink-4);
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--c);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--c) 18%, transparent);
    transition:
      background var(--dur) var(--ease),
      box-shadow var(--dur) var(--ease);
    flex-shrink: 0;
  }
  .dot.active {
    --c: var(--ok);
  }
  .dot.warn {
    --c: var(--warn);
  }
  .dot.error {
    --c: var(--danger);
  }
  .dot.busy {
    animation: pulse 1.8s var(--ease) infinite;
  }
  @keyframes pulse {
    0%,
    100% {
      box-shadow: 0 0 0 2px color-mix(in srgb, var(--c) 22%, transparent);
    }
    50% {
      box-shadow: 0 0 0 5px color-mix(in srgb, var(--c) 8%, transparent);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .dot.busy {
      animation: none;
    }
  }
  :global(:root[data-motion='reduced']) .dot.busy,
  :global(:root[data-motion='none']) .dot.busy {
    animation: none;
  }
  .bar-text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .pop {
    position: absolute;
    left: -4px;
    bottom: calc(100% + 8px);
    z-index: 50;
    width: 300px;
    max-width: calc(100vw - 24px);
    padding: 12px 14px;
    font-size: var(--fs-sm);
    color: var(--ink-2);
    animation: pop-in var(--dur) var(--ease-out);
  }
  header {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--ink);
  }
  .desc {
    margin: 6px 0 10px;
    color: var(--ink-3);
    line-height: 1.45;
  }
  .section-title {
    margin-top: 4px;
  }
  ul {
    list-style: none;
    margin: 4px 0 10px;
    padding: 0;
    display: grid;
    gap: 6px;
  }
  li {
    display: flex;
    gap: 6px;
    align-items: flex-start;
    line-height: 1.4;
  }
  li :global(svg) {
    flex-shrink: 0;
    margin-top: 1px;
  }
  li.warn :global(svg) {
    color: var(--warn);
  }
  li.error :global(svg) {
    color: var(--danger);
  }
  li.run::before {
    content: '';
    width: 6px;
    height: 6px;
    margin: 6px 2px 0;
    border-radius: 50%;
    background: var(--ok);
    flex-shrink: 0;
  }
  footer {
    display: flex;
    justify-content: flex-end;
  }
</style>
