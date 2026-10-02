<script lang="ts">
  // The changes an "AI…" turn proposes or made.
  import { Check, X, TriangleAlert, LoaderCircle, Ban } from '@lucide/svelte';
  import type { Turn } from './assistant.svelte';
  import { applyPlan, cancelPlan, type PlanState, type PlannedAction } from './agent.svelte';
  import { t } from '$lib/i18n/index.svelte';

  let { turn }: { turn: Turn } = $props();
  const st = $derived(turn.plan as PlanState);
  const chosen = $derived(st.plan.actions.filter((_, i) => !st.skip[i]).length);

  function describe(a: PlannedAction): string {
    const card = a.cardTitle || a.card;
    const p = { card, title: a.title, lane: a.laneName, board: a.boardName, key: t(`properties.${a.key}`), value: a.value, channel: a.channel };
    if (a.type === 'set_property' && !a.value) return t('assistant.actions.clear_property', p);
    return t(`assistant.actions.${a.type}`, p);
  }
</script>

<div class="plan" class:done={st.status === 'applied'}>
  {#if st.plan.actions.length}
    <ul>
      {#each st.plan.actions as a, i (i)}
        {@const r = st.results[i]}
        <li class:skipped={st.skip[i]} class:risky={a.risky}>
          {#if st.status === 'proposed'}
            <input
              type="checkbox"
              checked={!st.skip[i]}
              onchange={(e) => (st.skip[i] = !(e.currentTarget as HTMLInputElement).checked)}
              aria-label={describe(a)}
            />
          {:else if st.skip[i] || st.status === 'cancelled'}
            <span class="ic muted"><Ban size={13} /></span>
          {:else if r === null}
            <span class="ic"><LoaderCircle size={13} class="spin" /></span>
          {:else if r === ''}
            <span class="ic ok"><Check size={13} /></span>
          {:else}
            <span class="ic bad"><X size={13} /></span>
          {/if}
          <span class="what">
            {describe(a)}
            {#if a.risky}<span class="tag"><TriangleAlert size={11} />{t(`assistant.risk.${a.type === 'delete_card' ? 'delete' : 'outside'}`)}</span>{/if}
            {#if r}<span class="err">{r}</span>{/if}
          </span>
        </li>
      {/each}
    </ul>
  {/if}
  {#if st.plan.rejected.length}
    <details class="rejected">
      <summary>{t('assistant.plan.rejected', { count: st.plan.rejected.length })}</summary>
      <ul>
        {#each st.plan.rejected as x, i (i)}
          <li>
            {t(`assistant.actions.${x.type}`, { card: x.detail, title: '', lane: '', board: '', key: '', value: '', channel: '' })} — {t(
              `assistant.reasons.${x.reason}`,
            )}
          </li>
        {/each}
      </ul>
    </details>
  {/if}
  {#if st.status === 'proposed' && st.plan.actions.length}
    <p class="ask">{t('assistant.plan.confirm')}</p>
    <div class="buttons">
      <button class="btn sm" onclick={() => cancelPlan(turn)}>{t('common.cancel')}</button>
      <button class="btn sm primary" disabled={!chosen} onclick={() => void applyPlan(turn)}>{t('assistant.plan.apply', { count: chosen })}</button>
    </div>
  {:else if st.status === 'applied'}
    <p class="note">{t('assistant.plan.applied')}</p>
  {:else if st.status === 'failed'}
    <p class="note bad">{t('assistant.plan.someFailed')}</p>
  {:else if st.status === 'cancelled'}
    <p class="note">{t('assistant.plan.cancelled')}</p>
  {/if}
</div>

<style>
  .plan {
    display: flex;
    flex-direction: column;
    gap: var(--sp-2);
    padding: var(--sp-3);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    background: var(--bg-elev);
    font-size: var(--fs-sm);
  }
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  li {
    display: flex;
    align-items: flex-start;
    gap: 8px;
  }
  li.skipped .what {
    color: var(--ink-4);
    text-decoration: line-through;
  }
  input[type='checkbox'] {
    margin-top: 2px;
  }
  .ic {
    display: inline-flex;
    padding-top: 2px;
    color: var(--ink-3);
  }
  .ic.ok {
    color: var(--ok);
  }
  .ic.bad,
  .bad {
    color: var(--danger);
  }
  .muted {
    color: var(--ink-4);
  }
  .what {
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
    color: var(--ink);
  }
  .tag {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    margin-left: 6px;
    padding: 0 6px;
    border-radius: 999px;
    background: var(--warn-soft, var(--danger-soft));
    color: var(--warn, var(--danger));
    font-size: 0.92em;
  }
  .err {
    display: block;
    color: var(--danger);
  }
  .rejected {
    color: var(--ink-3);
  }
  .rejected summary {
    cursor: pointer;
  }
  .rejected ul {
    margin-top: 6px;
  }
  .ask,
  .note {
    margin: 0;
    color: var(--ink-2);
  }
  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: var(--sp-2);
  }
  :global(.plan .spin) {
    animation: spin 1s linear infinite;
  }
</style>
