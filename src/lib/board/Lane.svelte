<script lang="ts">
  import {
    Plus,
    MoreHorizontal,
    ChevronsLeftRight,
    ChevronsRight,
    Palette,
    Pencil,
    Gauge,
    Archive,
    ArchiveRestore,
    Trash2,
    MoveHorizontal,
  } from '@lucide/svelte';
  import type { LaneDto } from '$lib/backend/types';
  import { boards, apply } from '$lib/state/boards.svelte';
  import { openMenu, openMenuAt } from '$lib/state/menu.svelte';
  import { inputBox } from '$lib/quickinput/qi.svelte';
  import { pickColor } from '$lib/state/colorDialog.svelte';
  import { tintFromHex } from '$lib/theme/color';
  import { theme } from '$lib/theme/theme.svelte';
  import { selection } from '$lib/state/selection.svelte';
  import { toast } from '$lib/state/toasts.svelte';
  import { runCommand } from '$lib/commands/registry.svelte';
  import { tip } from '$lib/components/tooltip';
  import { t } from '$lib/i18n/index.svelte';
  import CardList from './CardList.svelte';
  import { boardUi, listKey } from './boardUi.svelte';
  import { startDrag } from './dnd.svelte';

  let {
    boardId,
    lane,
    index,
    orientation,
    spacing,
  }: { boardId: string; lane: LaneDto; index: number; orientation: 'columns' | 'rows'; spacing: 'fixedMain' | 'fixedCross' } = $props();

  const model = $derived(boards.get(boardId));
  const readOnly = $derived(!!model?.header.readOnly);
  const count = $derived(lane.order.filter((id) => !model?.nodes.get(id)?.archived).length);
  const overWip = $derived(!!lane.wip && count > lane.wip);
  const tint = $derived(lane.color ? tintFromHex(lane.color, theme.dark) : null);
  const renaming = $derived(boardUi.renamingLane === lane.id);
  const focused = $derived(selection.lane === lane.id && selection.boardId === boardId && !selection.ids.length);
  const flow = $derived(orientation === 'rows' ? 'x' : 'y');
  const wrap = $derived(spacing === 'fixedCross');

  let el: HTMLElement | undefined = $state();
  let name = $state('');
  let liveWidth = $state<number | null>(null);

  $effect(() => {
    if (renaming) name = lane.name;
  });

  const patch = (p: Record<string, unknown>, label: string) => apply(boardId, { op: 'updateLane', id: lane.id, patch: p }, label);

  function addCard(top = false) {
    const first = lane.order.find((id) => !model?.nodes.get(id)?.archived) ?? null;
    boardUi.quickAdd = { boardId, parent: { kind: 'lane', id: lane.id }, before: top ? first : null, key: listKey(boardId, { kind: 'lane', id: lane.id }) };
  }

  async function commitRename() {
    boardUi.renamingLane = null;
    const v = name.trim();
    if (v && v !== lane.name) await patch({ name: v }, t('ops.renameLane'));
  }

  async function setWip() {
    const v = await inputBox({
      title: t('lanes.wipLimit'),
      prompt: t('lanes.wipPrompt'),
      value: lane.wip ? String(lane.wip) : '',
      validate: (s) => (s === '' || /^\d{1,3}$/.test(s) ? null : t('validation.number')),
    });
    if (typeof v === 'string') await patch({ wip: v === '' ? 0 : Number(v) }, t('lanes.wipLimit'));
  }

  async function setColor() {
    const prev = lane.color ?? '#9aa6c4';
    const hex = await pickColor(t('lanes.color'), prev);
    if (hex) await patch({ color: hex }, t('lanes.color'));
  }

  async function removeLane() {
    await apply(boardId, { op: 'trash', nodes: [], lanes: [lane.id] }, t('ops.deleteLane'));
    toast.info(t('toasts.laneTrashed', { name: lane.name }), { action: { label: t('common.undo'), run: () => void runCommand('edit.undo') } });
  }

  function menu(e: MouseEvent) {
    const items = [
      { label: t('lanes.addCard'), icon: Plus, run: () => addCard(true), disabled: readOnly },
      { label: t('lanes.rename'), icon: Pencil, run: () => (boardUi.renamingLane = lane.id), disabled: readOnly },
      { label: t('lanes.color'), icon: Palette, run: setColor, disabled: readOnly },
      ...(lane.color ? [{ label: t('lanes.clearColor'), run: () => void patch({ color: '' }, t('lanes.color')), disabled: readOnly }] : []),
      { label: t('lanes.wipLimit'), icon: Gauge, run: setWip, disabled: readOnly },
      ...(lane.width ? [{ label: t('lanes.resetWidth'), icon: MoveHorizontal, run: () => void patch({ width: 0 }, t('lanes.resetWidth')) }] : []),
      {
        label: lane.collapsed ? t('lanes.expand') : t('lanes.collapse'),
        icon: ChevronsLeftRight,
        run: () => void patch({ collapsed: !lane.collapsed }, t('lanes.collapse')),
      },
      { separator: true },
      lane.archived
        ? {
            label: t('lanes.unarchive'),
            icon: ArchiveRestore,
            run: () => void apply(boardId, { op: 'setArchived', nodes: [], lanes: [[lane.id, false]] }, t('ops.unarchive')),
            disabled: readOnly,
          }
        : {
            label: t('lanes.archive'),
            icon: Archive,
            run: () => void apply(boardId, { op: 'setArchived', nodes: [], lanes: [[lane.id, true]] }, t('ops.archive')),
            disabled: readOnly,
          },
      { label: t('lanes.delete'), icon: Trash2, danger: true, run: removeLane, disabled: readOnly },
    ];
    if (e.type === 'contextmenu') openMenu(e, items);
    else openMenuAt(e.currentTarget as HTMLElement, items);
  }

  function onHeadPointerDown(e: PointerEvent) {
    if ((e.target as HTMLElement).closest('button, input') || readOnly) return;
    startDrag(e, { kind: 'lane', boardId, laneId: lane.id, label: lane.name }, el ?? null);
  }

  function startResize(e: PointerEvent) {
    e.stopPropagation();
    e.preventDefault();
    const startX = e.clientX;
    const startW = el?.getBoundingClientRect().width ?? 300;
    const zoom = Number(getComputedStyle(el!.closest('.lanes')!).zoom) || 1;
    const move = (ev: PointerEvent) => (liveWidth = Math.round(Math.min(720, Math.max(200, (startW + (ev.clientX - startX)) / zoom))));
    const up = async () => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
      document.documentElement.style.cursor = '';
      if (liveWidth) await patch({ width: liveWidth }, t('lanes.resize'));
      liveWidth = null;
    };
    document.documentElement.style.cursor = 'col-resize';
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
  }
</script>

<section
  bind:this={el}
  class="lane {orientation}"
  class:collapsed={lane.collapsed}
  class:archived={lane.archived}
  class:focused
  class:wrap
  data-lane={lane.id}
  data-board={boardId}
  data-index={index}
  data-flow={orientation}
  style:--lane-tint={tint?.bg}
  style:--lane-dot={tint?.dot}
  style:width={orientation === 'columns' && !lane.collapsed && !wrap && (liveWidth ?? lane.width) ? `${liveWidth ?? lane.width}px` : undefined}
  class:custom-w={orientation === 'columns' && !lane.collapsed && !wrap && !!(liveWidth ?? lane.width)}
  oncontextmenu={menu}
  role="group"
  aria-label={lane.name}
>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <header class="lhead" onpointerdown={onHeadPointerDown} ondblclick={() => !readOnly && (boardUi.renamingLane = lane.id)}>
    <span class="dot" class:has={!!lane.color}></span>
    {#if renaming}
      <!-- svelte-ignore a11y_autofocus -->
      <input
        class="rename"
        bind:value={name}
        autofocus
        onblur={commitRename}
        onkeydown={(e) => {
          e.stopPropagation();
          if (e.key === 'Enter') commitRename();
          if (e.key === 'Escape') boardUi.renamingLane = null;
        }}
      />
    {:else}
      <h2 class="name">{lane.name}</h2>
    {/if}
    <span class="count" class:over={overWip} use:tip={lane.wip ? t('lanes.wipTip', { count, wip: lane.wip }) : null}
      >{lane.wip ? `${count}/${lane.wip}` : count}</span
    >
    <span class="grow"></span>
    {#if !lane.collapsed && !readOnly}
      <button class="icon-btn sm act" onclick={() => addCard(true)} use:tip={t('lanes.addCard')}><Plus size={15} /></button>
    {/if}
    <button
      class="icon-btn sm act"
      onclick={() => patch({ collapsed: !lane.collapsed }, t('lanes.collapse'))}
      use:tip={lane.collapsed ? t('lanes.expand') : t('lanes.collapse')}
    >
      <ChevronsRight size={14} style="transform: rotate({lane.collapsed ? 0 : 180}deg)" />
    </button>
    <button class="icon-btn sm act" onclick={menu} use:tip={t('common.more')}><MoreHorizontal size={15} /></button>
  </header>
  {#if !lane.collapsed}
    <div class="lbody" data-autoscroll>
      <CardList {boardId} parent={{ kind: 'lane', id: lane.id }} ids={lane.order} {flow} {wrap}>
        {#snippet empty()}
          {#if !readOnly}
            <button class="empty-add" onclick={() => addCard()}>{t('lanes.emptyHint')}</button>
          {/if}
        {/snippet}
      </CardList>
    </div>
    {#if !readOnly}
      <button class="add" onclick={() => addCard()}><Plus size={14} /> {t('lanes.addCard')}</button>
    {/if}
  {/if}
  {#if orientation === 'columns' && !lane.collapsed && !wrap}
    <div class="resize" role="separator" aria-orientation="vertical" onpointerdown={startResize}></div>
  {/if}
</section>

<style>
  .lane {
    position: relative;
    display: flex;
    flex-direction: column;
    border-radius: var(--r-lg);
    background: var(--lane-tint, var(--bg-lane));
    transition:
      background var(--dur) var(--ease),
      box-shadow var(--dur-fast) var(--ease),
      opacity var(--dur) var(--ease);
    flex-shrink: 0;
  }
  :global(:root[data-theme='light']) .lane {
    background: color-mix(in srgb, var(--lane-tint, var(--bg-lane)) 55%, var(--bg-lane));
  }
  .lane.columns {
    width: var(--lane-w);
    max-height: 100%;
  }
  .lane.columns.custom-w {
    width: auto;
  }
  .lane.columns.wrap {
    width: auto;
    min-width: var(--lane-w);
  }
  .lane.rows {
    width: 100%;
  }
  .lane.focused {
    box-shadow: 0 0 0 2px var(--primary-ring);
  }
  .lane.archived {
    opacity: 0.6;
  }
  .lane.columns.collapsed {
    width: 48px;
  }
  .lane.columns.collapsed .lhead {
    flex-direction: column;
    height: 100%;
    padding: 12px 0;
    gap: 10px;
  }
  .lane.columns.collapsed .name {
    writing-mode: vertical-rl;
    transform: rotate(180deg);
  }
  .lane.columns.collapsed .grow {
    display: none;
  }
  .lhead {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 10px 10px 14px;
    cursor: grab;
    flex-shrink: 0;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--ink-4);
    opacity: 0.6;
    flex-shrink: 0;
  }
  .dot.has {
    background: var(--lane-dot);
    opacity: 1;
  }
  .name {
    margin: 0;
    font-size: var(--fs-md);
    font-weight: var(--fw-semibold);
    color: var(--ink);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }
  .rename {
    flex: 1;
    min-width: 0;
    border: none;
    outline: none;
    border-radius: 6px;
    padding: 2px 6px;
    margin: -2px -6px;
    background: var(--bg-elev);
    font: inherit;
    font-weight: var(--fw-semibold);
  }
  .count {
    font-size: var(--fs-xs);
    color: var(--ink-3);
    font-variant-numeric: tabular-nums;
    padding: 1px 7px;
    border-radius: var(--r-pill);
    background: var(--bg-hover);
  }
  .count.over {
    color: var(--danger);
    background: var(--danger-soft);
    font-weight: var(--fw-semibold);
  }
  .grow {
    flex: 1;
  }
  .act {
    opacity: 0;
    transition: opacity var(--dur-fast) var(--ease);
  }
  .lane:hover .act,
  .lane.collapsed .act {
    opacity: 1;
  }
  .lbody {
    flex: 1;
    min-height: 0;
    padding: 2px 10px 6px;
    overflow-y: auto;
    overflow-x: hidden;
  }
  .rows .lbody {
    overflow-x: auto;
    overflow-y: hidden;
    padding-bottom: 10px;
  }
  .rows.wrap .lbody {
    overflow: visible;
  }
  .columns.wrap .lbody {
    overflow-x: auto;
    overflow-y: hidden;
  }
  .add {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0 10px 10px;
    padding: 8px 10px;
    border: none;
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--ink-3);
    font-size: var(--fs-sm);
    font-weight: var(--fw-medium);
    text-align: left;
    transition:
      background var(--dur-fast) var(--ease),
      color var(--dur-fast) var(--ease);
  }
  .add:hover {
    background: var(--bg-hover);
    color: var(--ink);
  }
  .rows .add {
    display: none;
  }
  .empty-add {
    width: 100%;
    padding: 14px;
    border: 1.5px dashed var(--line-strong);
    border-radius: var(--r-md);
    background: transparent;
    color: var(--ink-4);
    font-size: var(--fs-sm);
  }
  .empty-add:hover {
    color: var(--ink-2);
    border-color: var(--ink-4);
  }
  .resize {
    position: absolute;
    top: 12px;
    bottom: 12px;
    right: -7px;
    width: 8px;
    cursor: col-resize;
    border-radius: 4px;
  }
  .resize:hover {
    background: var(--primary-ring);
  }
</style>
