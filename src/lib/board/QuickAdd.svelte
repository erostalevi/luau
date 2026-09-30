<script lang="ts">
  import { CornerDownLeft } from '@lucide/svelte';
  import { boardUi, type QuickAddTarget } from './boardUi.svelte';
  import { createCard } from './cardActions';
  import { select } from '$lib/state/selection.svelte';
  import { t } from '$lib/i18n/index.svelte';

  let { target }: { target: QuickAddTarget } = $props();
  let value = $state('');
  let input: HTMLTextAreaElement | undefined = $state();
  let busy = false;

  $effect(() => {
    void target.key;
    queueMicrotask(() => {
      input?.focus();
      input?.scrollIntoView({ block: 'nearest', inline: 'nearest' });
    });
  });

  async function submit() {
    const title = value.trim();
    if (!title || busy) return;
    busy = true;
    value = '';
    const id = await createCard(target.boardId, target.parent, target.before, `# ${title.replace(/\n+/g, ' ')}\n`);
    busy = false;
    if (id) select(target.boardId, id);
    input?.focus();
  }

  function onkeydown(e: KeyboardEvent) {
    e.stopPropagation();
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      void submit();
    } else if (e.key === 'Escape') {
      e.preventDefault();
      boardUi.quickAdd = null;
    }
  }
</script>

<div class="qa">
  <textarea
    bind:this={input}
    bind:value
    rows="1"
    placeholder={t('board.quickAddPlaceholder')}
    {onkeydown}
    onblur={() => {
      if (!value.trim()) setTimeout(() => (boardUi.quickAdd?.key === target.key ? (boardUi.quickAdd = null) : null), 120);
    }}
    oninput={() => {
      if (input) {
        input.style.height = 'auto';
        input.style.height = `${input.scrollHeight}px`;
      }
    }}
  ></textarea>
  <div class="hint"><CornerDownLeft size={11} /> {t('board.quickAddHint')}</div>
</div>

<style>
  .qa {
    padding: 10px 12px 8px;
    border-radius: var(--r-md);
    background: var(--bg-card);
    box-shadow:
      var(--shadow-2),
      0 0 0 2px var(--primary-ring);
    animation: pop-in var(--dur) var(--ease-out);
  }
  textarea {
    width: 100%;
    border: none;
    outline: none;
    resize: none;
    background: transparent;
    font: inherit;
    font-weight: var(--fw-medium);
    color: var(--ink);
    line-height: 1.4;
    padding: 0;
  }
  textarea::placeholder {
    color: var(--ink-4);
    font-weight: var(--fw-regular);
  }
  .hint {
    display: flex;
    align-items: center;
    gap: 4px;
    margin-top: 6px;
    font-size: var(--fs-xs);
    color: var(--ink-4);
  }
</style>
