<script lang="ts">
  // Editor header section for remote-linked cards: status, assignee,
  // transitions, pull/push, comments thread and (mirrors) private local notes.
  import { ArrowDownToLine, ArrowUpFromLine, ExternalLink, MessageSquare, ChevronDown, ChevronRight, RefreshCw, NotebookPen } from '@lucide/svelte';
  import type { RemoteInfo } from '$lib/backend/types';
  import { rpc } from '$lib/backend/rpc';
  import { t, relTime } from '$lib/i18n/index.svelte';
  import { tip } from '$lib/components/tooltip';
  import { openMenuAt } from '$lib/state/menu.svelte';
  import { runCommand } from '$lib/commands/registry.svelte';
  import { renderMarkdown } from '$lib/markdown/render';
  import { serviceName, errorText } from './gate';
  import { transitionCard } from './actions';
  import type { RemoteComment, Transition } from './types';

  const MD_OPTS = { titleOf: () => '', fileUrl: (r: string) => r };

  let { boardId, id, remote }: { boardId: string; id: string; remote?: RemoteInfo } = $props();

  let comments = $state<RemoteComment[] | null>(null);
  let showComments = $state(false);
  let loadingComments = $state(false);
  let commentsError = $state('');
  let notes = $state('');
  let notesLoaded = $state('');
  let saveTimer: ReturnType<typeof setTimeout> | null = null;

  async function loadComments() {
    if (!remote) return;
    loadingComments = true;
    commentsError = '';
    try {
      comments = await rpc<RemoteComment[]>('remote.comments', { account: remote.account, key: remote.key });
    } catch (e) {
      commentsError = errorText(e);
    } finally {
      loadingComments = false;
    }
  }

  function toggleComments() {
    showComments = !showComments;
    if (showComments && !comments) void loadComments();
  }

  async function pickTransition(e: MouseEvent) {
    if (!remote) return;
    const anchor = e.currentTarget as HTMLElement;
    let list: Transition[] = [];
    try {
      list = await rpc<Transition[]>('remote.transitions', { account: remote.account, key: remote.key });
    } catch (err) {
      commentsError = errorText(err);
      return;
    }
    openMenuAt(
      anchor,
      list.length ? list.map((tr) => ({ label: tr.name === tr.to ? tr.to : `${tr.name} → ${tr.to}`, run: () => void transitionCard(boardId, id, tr) })) : [{ label: t('integrations.noTransitions'), disabled: true }],
    );
  }

  $effect(() => {
    const key = `${boardId}:${id}`;
    if (remote?.mirror && notesLoaded !== key) {
      notesLoaded = key;
      rpc<string>('remote.notes.get', { board: boardId, card: id })
        .then((v) => (notes = v))
        .catch(() => (notes = ''));
    }
  });

  function onNotes() {
    if (saveTimer) clearTimeout(saveTimer);
    const text = notes;
    saveTimer = setTimeout(() => void rpc('remote.notes.set', { board: boardId, card: id, text }).catch(() => {}), 500);
  }
</script>

{#if remote}
  <section class="rh">
    <div class="line">
      <button class="key" onclick={() => runCommand('remote.openInBrowser')} use:tip={t('commands.remote.openInBrowser')}>
        <span class="svc">{serviceName(remote.provider)}</span>
        <strong>{remote.key}</strong>
        <ExternalLink size={12} />
      </button>
      {#if remote.status}
        <button class="chip status {remote.statusCategory ?? 'todo'}" onclick={pickTransition} use:tip={t('commands.remote.transition')}>
          {remote.status} <ChevronDown size={11} />
        </button>
      {/if}
      {#if remote.assignee}<span class="who">{remote.assignee.name}</span>{:else}<span class="who muted">{t('integrations.unassigned')}</span>{/if}
      {#if remote.updated}<span class="muted small">{relTime(remote.updated)}</span>{/if}
      <span class="grow"></span>
      <button class="icon-btn sm" onclick={() => runCommand('remote.pull')} use:tip={t('commands.remote.pull')}><ArrowDownToLine size={14} /></button>
      {#if !remote.mirror}
        <button class="icon-btn sm" onclick={() => runCommand('remote.push')} use:tip={t('commands.remote.push')}><ArrowUpFromLine size={14} /></button>
      {/if}
      <button class="icon-btn sm" onclick={() => runCommand('remote.comment')} use:tip={t('commands.remote.comment')}><MessageSquare size={14} /></button>
    </div>
    {#if remote.unavailable}<p class="warn small">{t('integrations.strip.unavailableTip')}</p>{/if}

    <button class="toggle-comments" onclick={toggleComments} aria-expanded={showComments}>
      {#if showComments}<ChevronDown size={13} />{:else}<ChevronRight size={13} />{/if}
      {t('integrations.comments')}
      {#if comments}<span class="muted">{comments.length}</span>{/if}
      {#if showComments}
        <span class="grow"></span>
        <span
          class="icon-btn sm"
          role="button"
          tabindex="0"
          onclick={(e) => {
            e.stopPropagation();
            void loadComments();
          }}
          onkeydown={(e) => {
            if (e.key === 'Enter') void loadComments();
          }}
          use:tip={t('integrations.refresh')}><RefreshCw size={12} class={loadingComments ? 'spin' : ''} /></span
        >
      {/if}
    </button>
    {#if showComments}
      <div class="comments">
        {#if commentsError}<p class="warn small">{commentsError}</p>{/if}
        {#if comments && !comments.length}<p class="muted small">{t('integrations.noComments')}</p>{/if}
        {#each comments ?? [] as c (c.id)}
          <article class="comment">
            <header><strong>{c.author?.name ?? '—'}</strong>{#if c.created}<span class="muted small">{relTime(c.created)}</span>{/if}</header>
            <!-- Raw HTML is disabled in the renderer, so remote text cannot inject markup. -->
            <div class="md">{@html renderMarkdown(c.bodyMd, MD_OPTS)}</div>
          </article>
        {/each}
      </div>
    {/if}

    {#if remote.mirror}
      <label class="notes">
        <span class="section-title"><NotebookPen size={12} /> {t('integrations.notes')}</span>
        <textarea class="field" rows="3" bind:value={notes} oninput={onNotes} placeholder={t('integrations.notesPlaceholder')}></textarea>
      </label>
    {/if}
  </section>
{/if}

<style>
  /* Plain buttons: no native chrome (zero specificity so shared classes win). */
  :where(.rh button) {
    border: none;
    background: transparent;
    padding: 0;
  }
  .rh {
    display: flex;
    flex-direction: column;
    gap: var(--sp-2);
    padding: var(--sp-3);
    border-radius: var(--r-md);
    background: var(--bg-sunken);
  }
  .line {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    flex-wrap: wrap;
  }
  .grow {
    flex: 1;
  }
  .key {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--ink);
    font-size: var(--fs-sm);
  }
  .key:hover strong {
    text-decoration: underline;
  }
  .svc {
    color: var(--ink-3);
    font-size: var(--fs-xs);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .status {
    cursor: pointer;
  }
  .status.inProgress {
    background: var(--info-soft);
    color: var(--info);
  }
  .status.done {
    background: var(--ok-soft);
    color: var(--ok);
  }
  .who {
    font-size: var(--fs-sm);
    color: var(--ink-2);
  }
  .muted {
    color: var(--ink-3);
  }
  .small {
    font-size: var(--fs-xs);
  }
  .warn {
    color: var(--warn);
    margin: 0;
  }
  .toggle-comments {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: var(--fs-sm);
    color: var(--ink-2);
    width: 100%;
  }
  .comments {
    display: flex;
    flex-direction: column;
    gap: var(--sp-2);
    max-height: 320px;
    overflow: auto;
  }
  .comment {
    padding: var(--sp-2) var(--sp-3);
    border-radius: var(--r-sm);
    background: var(--bg-card);
    box-shadow: 0 0 0 1px var(--line);
    font-size: var(--fs-sm);
  }
  .comment header {
    display: flex;
    gap: var(--sp-2);
    align-items: baseline;
    margin-bottom: 4px;
  }
  .md :global(p) {
    margin: 0 0 4px;
  }
  .notes {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .notes textarea {
    resize: vertical;
    min-height: 60px;
    font: inherit;
  }
</style>
