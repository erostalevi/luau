<script lang="ts">
  import {
    ChevronRight,
    X,
    Pin,
    PinOff,
    PanelRight,
    Maximize2,
    MoreHorizontal,
    Link,
    Archive,
    Trash2,
    FolderSearch,
    History,
    Eye,
    EyeOff,
    ArrowLeft,
    ArrowRight,
    Paperclip,
    FileText,
    Image as ImageIcon,
    Plus,
    CornerDownRight,
    TriangleAlert,
  } from '@lucide/svelte';
  import { rpc } from '$lib/backend/rpc';
  import { boards } from '$lib/state/boards.svelte';
  import { ui, closeEditor } from '$lib/state/ui.svelte';
  import { settings } from '$lib/settings/store.svelte';
  import { openMenuAt } from '$lib/state/menu.svelte';
  import { contributions } from '$lib/contributions/registry.svelte';
  import { runCommand } from '$lib/commands/registry.svelte';
  import { tip } from '$lib/components/tooltip';
  import { copyText, reveal } from '$lib/app/helpers';
  import { openCard, openCardById } from '$lib/app/open';
  import { cardFileUrl } from '$lib/board/paths';
  import { trashCards, setArchived, createCard } from '$lib/board/cardActions';
  import { t, relTime } from '$lib/i18n/index.svelte';
  import Editor from './Editor.svelte';

  let { boardId, cardId, variant }: { boardId: string; cardId: string; variant: 'modal' | 'sidebar' | 'page' } = $props();

  const model = $derived(boards.get(boardId));
  const node = $derived(model?.nodes.get(cardId));
  const remote = $derived(model?.remote.get(cardId));
  const readOnly = $derived(!!model?.header.readOnly);
  const laneName = $derived.by(() => {
    const k = model?.laneOf(cardId);
    return k ? model?.lane(k)?.name : null;
  });
  const ancestors = $derived(model?.ancestors(cardId) ?? []);
  const headers = $derived(contributions.editorHeader.filter((h) => h.when({ boardId, id: cardId, remote })));
  const pinned = $derived(settings.get<boolean>('editor.sidebarPinned'));
  const preview = $derived(settings.get<boolean>('editor.livePreview'));
  const showBacklinks = $derived(settings.get<boolean>('links.showBacklinks'));

  let editor: Editor | undefined = $state();
  let conflict = $state<{ mine: string; theirs: string } | null>(null);
  let backlinks = $state<{ id: string; board: string; title: string }[]>([]);

  $effect(() => {
    void cardId;
    void node?.mtime;
    if (!showBacklinks) return;
    void rpc<{ id: string; board: string; title: string }[]>('search.backlinks', { id: cardId })
      .then((r) => (backlinks = r.filter((x) => x.id !== cardId)))
      .catch(() => (backlinks = []));
  });

  function moreMenu(e: MouseEvent) {
    openMenuAt(e.currentTarget as HTMLElement, [
      { label: t('cards.copyLink'), icon: Link, run: () => void copyText(`[[${cardId}]]`) },
      {
        label: t('cards.reveal'),
        icon: FolderSearch,
        run: async () => {
          const r = await rpc<{ file: string | null }>('card.path', { board: boardId, id: cardId });
          if (r.file) await reveal(r.file);
        },
      },
      { label: t('editor.history'), icon: History, run: () => void runCommand('history.showCard', { boardId, cardId }) },
      { separator: true },
      { label: t('commands.app.changeEditorWidth'), command: 'app.changeEditorWidth' },
      { label: settings.get('editor.fullWidth') ? t('editor.readableWidth') : t('editor.fullWidth'), run: () => settings.toggle('editor.fullWidth') },
      { label: t('commands.app.toggleEditorMode'), command: 'app.toggleEditorMode' },
      { separator: true },
      node?.archived
        ? { label: t('cards.unarchive'), icon: Archive, run: () => void setArchived(boardId, [cardId], false), disabled: readOnly }
        : { label: t('cards.archive'), icon: Archive, run: () => void setArchived(boardId, [cardId], true), disabled: readOnly },
      {
        label: t('cards.delete'),
        icon: Trash2,
        danger: true,
        disabled: readOnly,
        run: async () => {
          if (variant !== 'page') closeEditor();
          await trashCards(boardId, [cardId]);
        },
      },
    ]);
  }

  async function addSubcard() {
    const id = await createCard(boardId, { kind: 'card', id: cardId }, null, '# \n');
    if (id) await openCard(boardId, id, { focusBoard: false });
  }

  function attIcon(kind: string) {
    return kind === 'image' ? ImageIcon : kind === 'pdf' ? FileText : Paperclip;
  }
</script>

{#if model && node}
  <div class="ce {variant}">
    <header class="bar" class:drag-region={variant === 'page'}>
      {#if variant !== 'page'}
        <button
          class="icon-btn sm no-drag"
          disabled={!ui.editor.back.length}
          onclick={() => runCommand('card.back')}
          use:tip={{ text: t('commands.card.back'), command: 'card.back' }}><ArrowLeft size={14} /></button
        >
        <button
          class="icon-btn sm no-drag"
          disabled={!ui.editor.fwd.length}
          onclick={() => runCommand('card.forward')}
          use:tip={{ text: t('commands.card.forward'), command: 'card.forward' }}><ArrowRight size={14} /></button
        >
      {/if}
      <nav class="crumbs no-drag" aria-label="breadcrumb">
        <button class="crumb" onclick={() => runCommand('board.openById', boardId)}>{model.header.name}</button>
        {#if laneName}<ChevronRight size={12} class="muted" /><span class="crumb plain">{laneName}</span>{/if}
        {#each ancestors as a (a.id)}
          <ChevronRight size={12} class="muted" />
          <button class="crumb" onclick={() => openCard(boardId, a.id, { focusBoard: false })}>{a.title || t('common.untitled')}</button>
        {/each}
      </nav>
      <span class="grow"></span>
      <span class="saved muted" title={new Date(node.mtime).toLocaleString()}>{relTime(node.mtime)}</span>
      <button
        class="icon-btn sm no-drag"
        class:active={!preview}
        onclick={() => runCommand('editor.togglePreview')}
        use:tip={{ text: preview ? t('editor.sourceMode') : t('editor.livePreview'), command: 'editor.togglePreview' }}
      >
        {#if preview}<Eye size={15} />{:else}<EyeOff size={15} />{/if}
      </button>
      {#if variant === 'sidebar'}
        <button
          class="icon-btn sm no-drag"
          class:active={pinned}
          onclick={() => settings.toggle('editor.sidebarPinned')}
          use:tip={pinned ? t('editor.unpin') : t('editor.pin')}
        >
          {#if pinned}<PinOff size={15} />{:else}<Pin size={15} />{/if}
        </button>
        <button class="icon-btn sm no-drag" onclick={() => settings.set('editor.openMode', 'modal')} use:tip={t('editor.asModal')}
          ><Maximize2 size={14} /></button
        >
      {:else if variant === 'modal'}
        <button class="icon-btn sm no-drag" onclick={() => settings.set('editor.openMode', 'sidebar')} use:tip={t('editor.asSidebar')}
          ><PanelRight size={15} /></button
        >
      {/if}
      <button class="icon-btn sm no-drag" onclick={moreMenu} use:tip={t('common.more')}><MoreHorizontal size={15} /></button>
      {#if variant !== 'page'}
        <button class="icon-btn sm no-drag" onclick={() => closeEditor()} use:tip={{ text: t('common.close'), keys: 'escape' }}><X size={15} /></button>
      {/if}
    </header>

    {#if conflict}
      <div class="conflict">
        <TriangleAlert size={15} />
        <span>{t('editor.conflict')}</span>
        <span class="grow"></span>
        <button class="btn sm" onclick={() => editor?.keepMine()}>{t('editor.keepMine')}</button>
        <button class="btn sm soft" onclick={() => conflict && editor?.takeTheirs(conflict.theirs)}>{t('editor.takeTheirs')}</button>
      </div>
    {/if}

    {#if headers.length}
      <div class="headers">
        {#each headers as h (h.id)}<h.component {boardId} id={cardId} {remote} />{/each}
      </div>
    {/if}

    <div class="body">
      <Editor
        bind:this={editor}
        {boardId}
        {cardId}
        autofocus={variant !== 'page' || !node.title}
        {readOnly}
        fullWidth={variant === 'page' && false}
        onconflict={(c) => (conflict = c)}
      />

      <div class="extras">
        {#if node.isGroup || model.kind === 'files'}
          <section class="section">
            <h4 class="section-title">{t('editor.children')} <span class="n">{node.children.length}</span></h4>
            <div class="children">
              {#each node.children as c (c)}
                {@const child = model.nodes.get(c)}
                {#if child}
                  <button class="child" onclick={() => openCard(boardId, c, { focusBoard: false })}>
                    <CornerDownRight size={13} class="muted" />
                    <span class="grow">{child.title || t('common.untitled')}</span>
                    {#if child.tasks.total}<span class="muted small">{child.tasks.done}/{child.tasks.total}</span>{/if}
                  </button>
                {/if}
              {/each}
              {#if !readOnly}
                <button class="child add" onclick={addSubcard}
                  ><Plus size={13} /> {model.kind === 'files' ? t('editor.addSubdoc') : t('cards.newSubcard')}</button
                >
              {/if}
            </div>
          </section>
        {/if}

        {#if node.attachments.length}
          <section class="section">
            <h4 class="section-title">{t('cards.attachments')} <span class="n">{node.attachments.length}</span></h4>
            <div class="atts">
              {#each node.attachments as a (a.file)}
                {@const Icon = attIcon(a.kind)}
                <div class="att" title={a.display}>
                  {#if a.kind === 'image'}
                    <img src={cardFileUrl(model, cardId, a.file, 160)} alt="" loading="lazy" />
                  {:else}
                    <span class="ic"><Icon size={18} /></span>
                  {/if}
                  <span class="name">{a.display}</span>
                </div>
              {/each}
            </div>
          </section>
        {/if}

        {#if showBacklinks && backlinks.length}
          <section class="section">
            <h4 class="section-title">{t('editor.backlinks')} <span class="n">{backlinks.length}</span></h4>
            <div class="children">
              {#each backlinks as b (b.board + b.id)}
                <button class="child" onclick={() => openCardById(b.id)}>
                  <Link size={13} class="muted" />
                  <span class="grow">{b.title || t('common.untitled')}</span>
                  {#if b.board !== boardId}<span class="muted small">{boards.get(b.board)?.header.name ?? ''}</span>{/if}
                </button>
              {/each}
            </div>
          </section>
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
  .ce {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    background: var(--bg-elev);
  }
  .ce.page {
    background: transparent;
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 4px;
    height: 46px;
    padding: 0 10px 0 14px;
    flex-shrink: 0;
  }
  .crumbs {
    display: flex;
    align-items: center;
    gap: 4px;
    min-width: 0;
    overflow: hidden;
    margin-left: 4px;
  }
  .crumb {
    border: none;
    background: transparent;
    padding: 3px 6px;
    border-radius: 6px;
    font-size: var(--fs-sm);
    color: var(--ink-3);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 200px;
  }
  button.crumb:hover {
    background: var(--bg-hover);
    color: var(--ink);
  }
  .crumb.plain {
    padding: 3px 4px;
  }
  .grow {
    flex: 1;
  }
  .saved {
    font-size: var(--fs-xs);
    margin-right: 6px;
    white-space: nowrap;
  }
  .conflict {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 16px 8px;
    padding: 8px 12px;
    border-radius: var(--r-md);
    background: var(--warn-soft);
    color: var(--warn);
    font-size: var(--fs-sm);
  }
  .headers {
    padding: 0 20px;
  }
  .body {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow-y: auto;
    padding: 0 28px;
  }
  .body :global(.luau-editor .cm-scroller) {
    overflow: visible;
  }
  .body :global(.luau-editor .cm-content) {
    padding-bottom: 24px;
  }
  .extras {
    max-width: var(--editor-w);
    width: 100%;
    margin: 0 auto 48px;
    display: flex;
    flex-direction: column;
    gap: 22px;
  }
  .section-title {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0 0 8px;
  }
  .n {
    font-weight: var(--fw-medium);
    color: var(--ink-4);
  }
  .children {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .child {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 10px;
    border: none;
    border-radius: var(--r-sm);
    background: transparent;
    text-align: left;
    color: var(--ink-2);
    font-size: var(--fs-md);
  }
  .child:hover {
    background: var(--bg-hover);
    color: var(--ink);
  }
  .child.add {
    color: var(--ink-3);
  }
  .small {
    font-size: var(--fs-xs);
  }
  .atts {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(110px, 1fr));
    gap: 10px;
  }
  .att {
    display: flex;
    flex-direction: column;
    border-radius: var(--r-md);
    overflow: hidden;
    background: var(--bg-sunken);
    box-shadow: inset 0 0 0 1px var(--line);
  }
  .att img {
    width: 100%;
    height: 80px;
    object-fit: cover;
  }
  .att .ic {
    height: 80px;
    display: grid;
    place-items: center;
    color: var(--ink-3);
  }
  .att .name {
    padding: 6px 8px;
    font-size: var(--fs-xs);
    color: var(--ink-2);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
