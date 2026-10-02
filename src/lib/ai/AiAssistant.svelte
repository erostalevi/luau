<script lang="ts">
  // Right-side drawer: "Quick summary…" (ask) and "AI…" (agent).
  import { fly, fade } from 'svelte/transition';
  import { Sparkles, X, Eraser, Copy, SendHorizontal, LoaderCircle, FileText } from '@lucide/svelte';
  import Segmented from '$lib/components/Segmented.svelte';
  import { assistant, closeAssistant, clearConversation, send, type Mode } from './assistant.svelte';
  import AgentPlanView from './AgentPlanView.svelte';
  import { renderMarkdown } from '$lib/markdown/render';
  import { resolveTitle } from '$lib/links/titles.svelte';
  import { openCardById } from '$lib/app/open';
  import { copyText } from '$lib/app/helpers';
  import { tip } from '$lib/components/tooltip';
  import { t } from '$lib/i18n/index.svelte';

  let input = $state<HTMLTextAreaElement>();
  let list = $state<HTMLElement>();

  const md = (s: string) => renderMarkdown(s, { titleOf: (id) => resolveTitle(id), fileUrl: (r) => r });

  // Focus the input when opened or switched; keep the newest turn in view.
  $effect(() => {
    if (assistant.open) {
      void assistant.mode;
      queueMicrotask(() => input?.focus());
    }
  });
  $effect(() => {
    void assistant.turns.length;
    void assistant.turns[assistant.turns.length - 1]?.text;
    queueMicrotask(() => list?.scrollTo({ top: list.scrollHeight }));
  });

  function onkey(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
      e.preventDefault();
      void send(assistant.draft);
    }
  }

  function onclick(e: MouseEvent) {
    const el = (e.target as HTMLElement).closest<HTMLElement>('[data-card]');
    if (el?.dataset.card) void openCardById(el.dataset.card);
  }

  function setMode(m: Mode) {
    assistant.mode = m;
  }
</script>

{#if assistant.open}
  <div
    class="assistant card-surface glass"
    data-overlay
    role="dialog"
    tabindex="-1"
    aria-label={t('assistant.title')}
    transition:fly={{ x: 40, duration: 200 }}
    onkeydown={(e) => e.key === 'Escape' && (e.preventDefault(), closeAssistant())}
  >
    <header>
      <Sparkles size={16} strokeWidth={1.8} />
      <Segmented
        size="sm"
        value={assistant.mode}
        onchange={setMode}
        options={[
          { value: 'ask', label: t('assistant.modes.ask') },
          { value: 'agent', label: t('assistant.modes.agent') },
        ]}
      />
      <span class="spacer"></span>
      {#if assistant.turns.length}
        <button class="icon-btn sm" aria-label={t('assistant.clear')} use:tip={t('assistant.clear')} onclick={clearConversation}><Eraser size={14} /></button>
      {/if}
      <button class="icon-btn sm" aria-label={t('common.close')} use:tip={t('common.close')} onclick={closeAssistant}><X size={15} /></button>
    </header>

    <!-- Card links in answers are delegated here; each answer also lists its sources as buttons. -->
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
    <div class="turns" bind:this={list} {onclick} role="log" aria-live="polite">
      {#if !assistant.turns.length}
        <div class="empty" in:fade>
          <strong>{t(`assistant.empty.${assistant.mode}.title`)}</strong>
          <span>{t(`assistant.empty.${assistant.mode}.hint`)}</span>
          <div class="examples">
            {#each [1, 2, 3] as n (n)}
              <button class="chip" onclick={() => void send(t(`assistant.empty.${assistant.mode}.ex${n}`))}
                >{t(`assistant.empty.${assistant.mode}.ex${n}`)}</button
              >
            {/each}
          </div>
        </div>
      {/if}
      {#each assistant.turns as turn (turn.id)}
        {#if turn.role === 'user'}
          <div class="q">{turn.text}</div>
        {:else}
          <div class="a">
            {#if turn.error}
              <p class="error" role="alert">{turn.error}</p>
            {:else if turn.text}
              <!-- eslint-disable-next-line svelte/no-at-html-tags -- renderMarkdown: markdown-it with html:false -->
              <div class="md">{@html md(turn.text)}</div>
            {:else if turn.streaming}
              <span class="thinking"><LoaderCircle size={13} class="spin" />{t('assistant.thinking')}</span>
            {/if}
            {#if turn.plan}<AgentPlanView {turn} />{/if}
            {#if turn.sources?.length}
              <div class="sources">
                <span class="label">{t('assistant.sources')}</span>
                {#each turn.sources as s (s.board + s.id)}
                  <button class="chip src" data-card={s.id} title={s.boardName}><FileText size={11} />{s.title}</button>
                {/each}
              </div>
            {/if}
            {#if !turn.streaming && turn.text && !turn.error}
              <div class="meta">
                {#if turn.model}<span>{turn.model}</span>{/if}
                <button class="icon-btn xs" aria-label={t('assistant.copy')} use:tip={t('assistant.copy')} onclick={() => void copyText(turn.text)}
                  ><Copy size={12} /></button
                >
              </div>
            {/if}
          </div>
        {/if}
      {/each}
    </div>

    <form class="compose" onsubmit={(e) => (e.preventDefault(), void send(assistant.draft))}>
      <textarea
        bind:this={input}
        bind:value={assistant.draft}
        rows="2"
        placeholder={t(`assistant.placeholder.${assistant.mode}`)}
        aria-label={t(`assistant.placeholder.${assistant.mode}`)}
        onkeydown={onkey}></textarea>
      <button class="icon-btn primary" type="submit" aria-label={t('assistant.send')} disabled={assistant.busy || !assistant.draft.trim()}>
        {#if assistant.busy}<LoaderCircle size={15} class="spin" />{:else}<SendHorizontal size={15} />{/if}
      </button>
    </form>
  </div>
{/if}

<style>
  .assistant {
    position: fixed;
    z-index: 1100;
    top: 52px;
    right: 12px;
    bottom: 12px;
    width: min(420px, calc(100vw - 24px));
    display: flex;
    flex-direction: column;
    border-radius: var(--r-xl);
    overflow: hidden;
  }
  header {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    padding: var(--sp-3) var(--sp-3) var(--sp-2) var(--sp-4);
    color: var(--primary-strong);
  }
  .spacer {
    flex: 1;
  }
  .turns {
    flex: 1;
    min-height: 0;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: var(--sp-3);
    padding: var(--sp-2) var(--sp-4) var(--sp-4);
  }
  .empty {
    display: flex;
    flex-direction: column;
    gap: var(--sp-2);
    margin: auto 0;
    color: var(--ink-2);
    font-size: var(--fs-md);
  }
  .empty strong {
    color: var(--ink);
    font-size: var(--fs-lg);
    font-weight: var(--fw-semibold);
  }
  .examples {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 6px;
    margin-top: var(--sp-2);
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    max-width: 100%;
    padding: 4px 10px;
    border: 1px solid var(--line);
    border-radius: 999px;
    background: var(--bg-elev);
    color: var(--ink-2);
    font-size: var(--fs-sm);
    text-align: left;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    cursor: pointer;
  }
  .chip:hover {
    color: var(--primary-strong);
    border-color: var(--primary-soft);
  }
  .q {
    align-self: flex-end;
    max-width: 85%;
    padding: 8px 12px;
    border-radius: 14px 14px 4px 14px;
    background: var(--primary-soft);
    color: var(--ink);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .a {
    display: flex;
    flex-direction: column;
    gap: var(--sp-2);
    min-width: 0;
  }
  .md {
    line-height: 1.55;
    overflow-wrap: anywhere;
    user-select: text;
  }
  .md :global(p) {
    margin: 0 0 0.5em;
  }
  .md :global(ul),
  .md :global(ol) {
    margin: 0 0 0.5em;
    padding-left: 1.3em;
  }
  .md :global(.card-link) {
    color: var(--primary-strong);
    text-decoration: underline;
    text-decoration-color: var(--primary-soft);
    cursor: pointer;
  }
  .thinking {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--ink-3);
    font-size: var(--fs-sm);
  }
  .error {
    margin: 0;
    padding: 8px 12px;
    border-radius: var(--r-md);
    background: var(--danger-soft);
    color: var(--danger);
    font-size: var(--fs-sm);
  }
  .sources {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 5px;
  }
  .label,
  .meta {
    font-size: var(--fs-sm);
    color: var(--ink-3);
  }
  .meta {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .compose {
    display: flex;
    align-items: flex-end;
    gap: var(--sp-2);
    padding: var(--sp-3);
    border-top: 1px solid var(--line);
  }
  textarea {
    flex: 1;
    resize: none;
    max-height: 160px;
    padding: 8px 10px;
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    background: var(--bg-elev);
    color: var(--ink);
    font: inherit;
    line-height: 1.45;
  }
  :global(.assistant .spin) {
    animation: spin 1s linear infinite;
  }
</style>
