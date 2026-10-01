<script lang="ts">
  import { getContext } from 'svelte';
  import { ChevronDown, Paperclip, Link2, Flag, CalendarDays, Archive, CircleCheck } from '@lucide/svelte';
  import { boards } from '$lib/state/boards.svelte';
  import { isSelected, select, selection, selectMany } from '$lib/state/selection.svelte';
  import { openMenu } from '$lib/state/menu.svelte';
  import { settings } from '$lib/settings/store.svelte';
  import { contributions } from '$lib/contributions/registry.svelte';
  import { openCard } from '$lib/app/open';
  import { fmtDate, t } from '$lib/i18n/index.svelte';
  import TagChip from '$lib/components/TagChip.svelte';
  import CardFace from './CardFace.svelte';
  import CardList from './CardList.svelte';
  import { boardUi } from './boardUi.svelte';
  import { startDrag } from './dnd.svelte';
  import { cardMenu } from './cardMenu';
  import { renameCard } from './cardActions';
  import { cardFileUrl } from './paths';
  import type { CompiledFilter } from './filter';

  let { boardId, id, depth = 0, flow: _flow = 'y' }: { boardId: string; id: string; depth?: number; flow?: 'x' | 'y' } = $props();

  const model = $derived(boards.get(boardId));
  const node = $derived(model?.nodes.get(id));
  const remote = $derived(model?.remote.get(id));
  const selected = $derived(isSelected(boardId, id));
  const collapsed = $derived(!!boardUi.collapsed[id]);
  const renaming = $derived(boardUi.renamingCard === id);
  const filter = getContext<() => CompiledFilter>('boardFilter');
  const dim = $derived(
    !!filter && node
      ? !filter().empty &&
          !filter().test(node, remote) &&
          !(
            node.isGroup &&
            model?.descendants(id).some((d) => {
              const dn = model.nodes.get(d);
              return dn && filter().test(dn, model.remote.get(d));
            })
          )
      : false,
  );
  const copyOnly = $derived(!!model?.header.readOnly && model?.header.readOnly.startsWith('mirror'));
  const today = new Date().toISOString().slice(0, 10);
  const due = $derived(node ? (node.footer.due ?? node.dates[0] ?? null) : null);
  const overdue = $derived(!!due && due < today && !(node && node.tasks.total > 0 && node.tasks.done === node.tasks.total));
  const coverFile = $derived(node?.cover ? node.attachments.find((a) => a.file === node.cover!.file) : null);
  const showTags = $derived(settings.get<boolean>('board.face.tags'));
  const showInd = $derived(settings.get<boolean>('board.face.indicators'));
  const faceProviders = $derived(contributions.cardFace.filter((p) => p.when({ boardId, id, remote })));
  // Providers default to "external" (data from Jira & co.); local ones opt in.
  const localProviders = $derived(faceProviders.filter((p) => p.placement === 'local'));
  const externalProviders = $derived(faceProviders.filter((p) => p.placement !== 'local'));
  const childCount = $derived(node?.isGroup && model ? model.descendants(id).length : 0);
  const priorityColor: Record<string, string> = { urgent: 'var(--danger)', high: '#e49a5a', medium: 'var(--warn)', low: 'var(--info)' };

  let el: HTMLElement | undefined = $state();
  let renameValue = $state('');

  $effect(() => {
    if (renaming) renameValue = node?.title ?? '';
  });

  function interactive(target: EventTarget | null) {
    return !!(target as HTMLElement)?.closest('button, input, a, textarea, [contenteditable], .no-card-drag');
  }

  function onpointerdown(e: PointerEvent) {
    if (e.button !== 0 || interactive(e.target) || renaming) return;
    e.stopPropagation();
    if (e.shiftKey || e.metaKey || e.ctrlKey) return; // handled on click
    const ids = selected && selection.ids.length > 1 ? [...selection.ids] : [id];
    const scroller = el?.closest<HTMLElement>('[data-board-scroll]');
    startDrag(e, { kind: 'cards', boardId, ids, copyOnly, label: node?.title }, el ?? null, scroller ? [scroller] : []);
  }

  function visibleSiblings(): string[] {
    const list = el?.closest('[data-list]');
    return [...(list?.querySelectorAll<HTMLElement>(':scope > .flip-item > [data-card]') ?? [])].map((x) => x.dataset.card!);
  }

  function onclick(e: MouseEvent) {
    if (interactive(e.target) || renaming) return;
    e.stopPropagation();
    if (e.metaKey || e.ctrlKey) {
      select(boardId, id, 'toggle');
      return;
    }
    if (e.shiftKey && selection.boardId === boardId && selection.anchor) {
      const sibs = visibleSiblings();
      const a = sibs.indexOf(selection.anchor);
      const b = sibs.indexOf(id);
      if (a >= 0 && b >= 0) {
        const [lo, hi] = a < b ? [a, b] : [b, a];
        selectMany(boardId, sibs.slice(lo, hi + 1), true);
        return;
      }
      select(boardId, id, 'add');
      return;
    }
    select(boardId, id);
    void openCard(boardId, id, { focusBoard: false });
  }

  function oncontextmenu(e: MouseEvent) {
    openMenu(e, cardMenu(boardId, id));
  }

  async function commitRename() {
    const v = renameValue.trim();
    boardUi.renamingCard = null;
    if (node && v !== node.title) await renameCard(boardId, id, v);
  }

  function initials(name: string) {
    return name
      .split(/[\s._-]+/)
      .filter(Boolean)
      .slice(0, 2)
      .map((p) => p[0]!.toUpperCase())
      .join('');
  }
</script>

{#if node}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
  <article
    bind:this={el}
    class="card"
    class:group={node.isGroup}
    class:selected
    class:archived={node.archived}
    class:dim
    class:flash={boardUi.flash === id}
    class:has-cover={coverFile?.kind === 'image' && node.cover?.mode === 'cover'}
    class:deep={depth >= 4}
    data-card={id}
    data-board={boardId}
    data-group={node.isGroup ? '1' : '0'}
    tabindex="-1"
    data-selected={selected ? '' : undefined}
    {onpointerdown}
    {onclick}
    {oncontextmenu}
  >
    {#if node.isGroup}
      <header class="ghead" data-group-head>
        <button
          class="icon-btn sm chev"
          class:collapsed
          aria-label={collapsed ? t('cards.expand') : t('cards.collapse')}
          onclick={(e) => {
            e.stopPropagation();
            boardUi.collapsed[id] = !collapsed;
          }}><ChevronDown size={14} strokeWidth={2.2} /></button
        >
        {#if renaming}
          <!-- svelte-ignore a11y_autofocus -->
          <input
            class="rename"
            bind:value={renameValue}
            autofocus
            onblur={commitRename}
            onkeydown={(e) => {
              if (e.key === 'Enter') commitRename();
              if (e.key === 'Escape') boardUi.renamingCard = null;
              e.stopPropagation();
            }}
          />
        {:else}
          <h3 class="gtitle">{node.title || t('common.untitled')}</h3>
        {/if}
        {#if remote}<span class="chip outline key">{remote.key}</span>{/if}
        <span class="count" title={t('cards.cardsInside', { count: childCount })}>{childCount}</span>
        {#if node.tasks.total}
          <span class="progress" class:done={node.tasks.done === node.tasks.total}>{node.tasks.done}/{node.tasks.total}</span>
        {/if}
      </header>
      {#if !collapsed}
        <div class="gbody">
          <CardList {boardId} parent={{ kind: 'card', id }} ids={node.children} depth={depth + 1} flow="y" />
        </div>
      {/if}
      {@const gLocal = showTags && (node.tags.length > 0 || node.footer.labels.length > 0)}
      {#if gLocal || localProviders.length || externalProviders.length}
        <footer class="foot gfoot">
          {#if gLocal || localProviders.length}
            <div class="meta-local">
              {#if gLocal}
                {#each node.tags.slice(0, 4) as tag (tag)}<TagChip {tag} />{/each}
                {#each node.footer.labels.slice(0, 3) as l (l)}<TagChip tag={l} outline prefix="" />{/each}
              {/if}
              {#each localProviders as p (p.id)}<p.component {boardId} {id} {remote} compact />{/each}
            </div>
          {/if}
          {#if externalProviders.length}
            <div class="meta-external">
              {#each externalProviders as p (p.id)}<p.component {boardId} {id} {remote} compact />{/each}
            </div>
          {/if}
        </footer>
      {/if}
    {:else}
      {#if coverFile && model && node.cover?.mode === 'cover'}
        <div class="cover">
          {#if coverFile.kind === 'image'}
            <img src={cardFileUrl(model, id, coverFile.file, 560)} alt="" loading="lazy" draggable="false" />
          {/if}
        </div>
      {/if}
      <div class="head">
        {#if renaming}
          <!-- svelte-ignore a11y_autofocus -->
          <input
            class="rename"
            bind:value={renameValue}
            autofocus
            onblur={commitRename}
            onkeydown={(e) => {
              if (e.key === 'Enter') commitRename();
              if (e.key === 'Escape') boardUi.renamingCard = null;
              e.stopPropagation();
            }}
          />
        {:else}
          <h3 class="title" class:untitled={!node.title}>{node.title || t('common.untitled')}</h3>
        {/if}
        {#if coverFile && model && node.cover?.mode === 'thumb' && coverFile.kind === 'image'}
          <img class="thumb" src={cardFileUrl(model, id, coverFile.file, 96)} alt="" loading="lazy" draggable="false" />
        {/if}
      </div>
      <CardFace {node} {boardId} />
      <!-- Footer: local metadata (markdown tags/labels, priority, due, tasks, mentions…)
           is left-aligned; metadata from external services (Jira status, type,
           priority, assignee — card-face providers) is right-aligned. -->
      {@const hasTags = showTags && (node.tags.length > 0 || node.footer.labels.length > 0)}
      {@const hasInd =
        showInd && !!(due || node.footer.priority || node.mentions.length || node.attachments.length || node.links.length || node.tasks.total || node.archived)}
      {@const hasLocal = hasTags || hasInd || localProviders.length > 0}
      {#if hasLocal || externalProviders.length}
        <footer class="foot">
          {#if hasLocal}
            <div class="meta-local">
              {#if showInd && node.archived}<span class="i" title={t('cards.archived')}><Archive size={12} /></span>{/if}
              {#if showInd && node.footer.priority && node.footer.priority !== 'none'}
                <span class="i" style:color={priorityColor[node.footer.priority]} title={t(`priority.${node.footer.priority}`)}
                  ><Flag size={12} fill="currentColor" /></span
                >
              {/if}
              {#if showTags}
                {#each node.tags.slice(0, 5) as tag (tag)}<TagChip {tag} />{/each}
                {#if node.tags.length > 5}<span class="chip">+{node.tags.length - 5}</span>{/if}
                {#each node.footer.labels.slice(0, 3) as l (l)}<TagChip tag={l} outline prefix="" />{/each}
              {/if}
              {#if showInd}
                {#if due}
                  <span class="i due" class:overdue title={t('cards.due')}><CalendarDays size={12} /> {fmtDate(due)}</span>
                {/if}
                {#if node.tasks.total}
                  <span class="i" class:done={node.tasks.done === node.tasks.total} title={t('cards.tasks')}
                    ><CircleCheck size={12} /> {node.tasks.done}/{node.tasks.total}</span
                  >
                {/if}
                {#if node.attachments.length}<span class="i" title={t('cards.attachments')}><Paperclip size={12} /> {node.attachments.length}</span>{/if}
                {#if node.links.length}<span class="i" title={t('cards.links')}><Link2 size={12} /> {node.links.length}</span>{/if}
                {#if node.mentions.length}
                  <span class="avatars">
                    {#each node.mentions.slice(0, 3) as m (m)}
                      <span class="avatar" title="@{m}">{initials(m)}</span>
                    {/each}
                  </span>
                {/if}
              {/if}
              {#each localProviders as p (p.id)}<p.component {boardId} {id} {remote} />{/each}
            </div>
          {/if}
          {#if externalProviders.length}
            <div class="meta-external">
              {#each externalProviders as p (p.id)}<p.component {boardId} {id} {remote} />{/each}
            </div>
          {/if}
        </footer>
      {/if}
    {/if}
  </article>
{/if}

<style>
  .card {
    position: relative;
    padding: 12px 14px;
    border-radius: var(--r-md);
    background: var(--bg-card);
    box-shadow:
      var(--shadow-1),
      0 0 0 1px var(--line);
    color: var(--ink);
    transition:
      box-shadow var(--dur-fast) var(--ease),
      transform var(--dur-fast) var(--ease),
      opacity var(--dur) var(--ease),
      background var(--dur-fast) var(--ease);
    outline: none;
    overflow: hidden;
  }
  .card:hover {
    box-shadow:
      var(--shadow-2),
      0 0 0 1px var(--line-strong);
  }
  .card.selected {
    box-shadow:
      var(--shadow-2),
      0 0 0 2px var(--primary);
  }
  .card.dim {
    opacity: 0.28;
    filter: saturate(0.4);
  }
  .card.archived {
    background: color-mix(in srgb, var(--bg-card) 70%, transparent);
    box-shadow: inset 0 0 0 1px var(--line-strong);
    opacity: 0.75;
  }
  .card.flash {
    animation: flash 900ms var(--ease-out);
  }
  @keyframes flash {
    0% {
      box-shadow:
        0 0 0 3px var(--primary-ring),
        var(--shadow-2);
      transform: scale(1.015);
    }
  }
  .card.has-cover {
    padding-top: 0;
  }
  .cover {
    margin: 0 -14px 10px;
    height: 132px;
    background: var(--bg-sunken);
    overflow: hidden;
  }
  .cover img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
  .head {
    display: flex;
    gap: 10px;
    align-items: flex-start;
  }
  .title {
    flex: 1;
    min-width: 0;
    margin: 0;
    font-size: var(--fs-md);
    font-weight: var(--fw-medium);
    line-height: 1.4;
    overflow-wrap: anywhere;
  }
  .title.untitled {
    color: var(--ink-4);
  }
  .thumb {
    width: 40px;
    height: 40px;
    border-radius: var(--r-xs);
    object-fit: cover;
    flex-shrink: 0;
  }
  .rename {
    flex: 1;
    min-width: 0;
    border: none;
    outline: none;
    background: var(--primary-softer);
    border-radius: 5px;
    padding: 2px 6px;
    margin: -2px -6px;
    font: inherit;
    font-weight: var(--fw-medium);
  }
  /* Two explicit groups: local metadata hugs the left padding, external
     metadata hugs the right edge. When narrow the external group wraps
     onto its own line and stays right-aligned (margin-left: auto). */
  .foot {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 8px;
    margin-top: 10px;
  }
  .meta-local,
  .meta-external {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 5px 8px;
    min-width: 0;
  }
  .meta-local {
    justify-content: flex-start;
    color: var(--ink-3);
    font-size: var(--fs-xs);
  }
  .meta-external {
    justify-content: flex-end;
    margin-left: auto;
  }
  .i {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    font-variant-numeric: tabular-nums;
  }
  .i.done {
    color: var(--ok);
  }
  .due.overdue {
    color: var(--danger);
    font-weight: var(--fw-medium);
  }
  .avatar {
    width: 20px;
    height: 20px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    background: var(--secondary);
    color: var(--secondary-ink);
    font-size: 9.5px;
    font-weight: var(--fw-semibold);
    box-shadow: 0 0 0 2px var(--bg-card);
  }
  .avatars {
    display: inline-flex;
    align-items: center;
  }
  .avatar + .avatar {
    margin-left: -4px;
  }

  /* Group card: a quiet container with compact header/footer. */
  .card.group {
    padding: 6px 8px 8px;
    background: var(--bg-lane);
    box-shadow: inset 0 0 0 1px var(--line);
  }
  :global(:root[data-theme='light']) .card.group {
    background: color-mix(in srgb, var(--primary-softer) 55%, var(--bg-sunken));
  }
  .card.group:hover {
    box-shadow: inset 0 0 0 1px var(--line-strong);
  }
  .card.group.selected {
    box-shadow: inset 0 0 0 2px var(--primary);
  }
  .ghead {
    display: flex;
    align-items: center;
    gap: 6px;
    min-height: 28px;
    padding: 0 4px 4px 0;
  }
  .gtitle {
    flex: 1;
    min-width: 0;
    margin: 0;
    font-size: var(--fs-sm);
    font-weight: var(--fw-semibold);
    color: var(--ink-2);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .chev {
    transition: transform var(--dur) var(--ease-spring);
  }
  .chev.collapsed {
    transform: rotate(-90deg);
  }
  .count,
  .progress {
    font-size: var(--fs-xs);
    color: var(--ink-3);
    font-variant-numeric: tabular-nums;
    padding: 0 6px;
    border-radius: var(--r-pill);
    background: var(--bg-hover);
  }
  .progress.done {
    color: var(--ok);
    background: var(--ok-soft);
  }
  .key {
    font-family: var(--font-mono);
    font-size: 10.5px;
  }
  .gbody {
    padding-left: calc(2px + max(0, 4 - var(--depth, 0)) * 0px);
  }
  .foot.gfoot {
    margin: 6px 4px 0;
  }
  .deep .ghead {
    min-height: 24px;
  }
</style>
