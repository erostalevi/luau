// Pointer-based drag & drop engine (HTML5 DnD is unreliable on WebKitGTK).
//
// Drop targets are discovered by hit-testing DOM attributes:
//   [data-card]      a card (zones: before 30% / nest 40% / after 30% along the flow axis)
//   [data-list]      a card list (lane body, group children, files root)
//   [data-lane]      a lane column/row (lane reordering)
//   [data-tree]      an explorer row (board / lane / card)
//   [data-dropzone]  custom zones registered by features (tabs, integrations)

import type { Parent } from '$lib/backend/types';
import { boards, apply } from '$lib/state/boards.svelte';
import { rpc } from '$lib/backend/rpc';
import { toast } from '$lib/state/toasts.svelte';
import { t } from '$lib/i18n/index.svelte';
import { boardUi } from './boardUi.svelte';
import { ctx as cmdCtx } from '$lib/commands/context.svelte';

export type DragKind = 'cards' | 'lane' | 'external';

export interface DragSource {
  kind: DragKind;
  boardId?: string;
  ids?: string[];
  laneId?: string;
  /** For external payloads (e.g. a Jira issue). */
  payload?: unknown;
  /** Read-only source: only copying out is allowed (mirror boards). */
  copyOnly?: boolean;
  label?: string;
  /** Items carried by an external payload (ghost badge); defaults to `ids.length` or 1. */
  count?: number;
}

export type Zone = 'before' | 'after' | 'into';

export interface DropTarget {
  type: 'card' | 'list' | 'lane' | 'tree' | 'custom';
  boardId: string;
  parent: Parent;
  before: string | null;
  zone: Zone;
  cardId?: string;
  laneIndex?: number;
  el: HTMLElement;
  custom?: string;
  valid: boolean;
}

export const dnd = $state({
  active: false,
  source: null as DragSource | null,
  target: null as DropTarget | null,
  x: 0,
  y: 0,
  count: 0,
});

/** External drop handlers keyed by `[data-dropzone]` id or source payload type. */
type ExternalHandler = (src: DragSource, target: DropTarget) => Promise<void> | void;
const externalHandlers = new Map<string, ExternalHandler>();
export function onExternalDrop(type: string, fn: ExternalHandler) {
  externalHandlers.set(type, fn);
}

let ghost: HTMLElement | null = null;
let indicator: HTMLElement | null = null;
let scrollers: HTMLElement[] = [];
let raf = 0;
let hoverTimer: ReturnType<typeof setTimeout> | null = null;
let hoverId: string | null = null;

function ensureIndicator(): HTMLElement {
  if (!indicator) {
    indicator = document.createElement('div');
    indicator.className = 'dnd-indicator';
    document.body.appendChild(indicator);
  }
  return indicator;
}

function makeGhost(from: HTMLElement | null, count: number, label?: string) {
  ghost = document.createElement('div');
  ghost.className = 'dnd-ghost';
  if (from) {
    const r = from.getBoundingClientRect();
    const clone = from.cloneNode(true) as HTMLElement;
    clone.classList.add('dnd-clone');
    // The clone must never be mistaken for a real card (hit-testing, styling).
    for (const el of [clone, ...clone.querySelectorAll<HTMLElement>('[data-card],[data-list],[data-lane]')]) {
      el.removeAttribute('data-card');
      el.removeAttribute('data-list');
      el.removeAttribute('data-lane');
    }
    clone.style.width = `${r.width}px`;
    clone.style.maxHeight = '260px';
    ghost.appendChild(clone);
    if (count > 1) {
      const stack = document.createElement('div');
      stack.className = 'dnd-stack';
      ghost.appendChild(stack);
    }
  } else {
    const pill = document.createElement('div');
    pill.className = 'dnd-pill';
    pill.textContent = label ?? '';
    ghost.appendChild(pill);
  }
  if (count > 1) {
    const badge = document.createElement('div');
    badge.className = 'dnd-badge';
    badge.textContent = String(count);
    ghost.appendChild(badge);
  }
  document.body.appendChild(ghost);
}

function flowAxis(listEl: HTMLElement | null): 'y' | 'x' {
  return listEl?.dataset.flow === 'x' ? 'x' : 'y';
}

function parentFromList(el: HTMLElement): Parent {
  const kind = el.dataset.parentKind as Parent['kind'];
  if (kind === 'root') return { kind: 'root' };
  return { kind, id: el.dataset.parentId! } as Parent;
}

/** Can the dragged cards go into `parent` on `boardId`? */
function validFor(boardId: string, parent: Parent, cardId?: string): boolean {
  const src = dnd.source;
  if (!src) return false;
  const b = boards.get(boardId);
  if (b?.header.readOnly) return false;
  if (src.kind === 'lane') return false;
  if (src.kind === 'external') return true;
  if (src.copyOnly && src.boardId === boardId) return false;
  if (src.boardId === boardId && b && src.ids) {
    const targetCard = parent.kind === 'card' ? parent.id : cardId;
    if (targetCard && src.ids.some((id) => id === targetCard || b.isAncestor(id, targetCard))) return false;
  }
  return true;
}

function computeTarget(x: number, y: number): DropTarget | null {
  const hit = document.elementFromPoint(x, y) as HTMLElement | null;
  if (!hit) return null;
  const src = dnd.source!;

  // Custom zones (tabs, integrations…)
  const custom = hit.closest<HTMLElement>('[data-dropzone]');
  if (custom) {
    return {
      type: 'custom',
      boardId: custom.dataset.board ?? '',
      parent: { kind: 'root' },
      before: null,
      zone: 'into',
      el: custom,
      custom: custom.dataset.dropzone,
      valid: true,
    };
  }

  // Explorer tree rows.
  const tree = hit.closest<HTMLElement>('[data-tree]');
  if (tree && src.kind !== 'lane') {
    const r = tree.getBoundingClientRect();
    const rel = (y - r.top) / r.height;
    const boardId = tree.dataset.board!;
    const type = tree.dataset.tree!;
    if (type === 'board') {
      const kind = boards.get(boardId)?.kind ?? tree.dataset.kind;
      const firstLane = tree.dataset.firstLane;
      const parent: Parent = kind === 'files' || !firstLane ? { kind: 'root' } : { kind: 'lane', id: firstLane };
      return { type: 'tree', boardId, parent, before: null, zone: 'into', el: tree, valid: validFor(boardId, parent) && (kind === 'files' || !!firstLane) };
    }
    if (type === 'lane') {
      const parent: Parent = { kind: 'lane', id: tree.dataset.id! };
      return { type: 'tree', boardId, parent, before: null, zone: 'into', el: tree, valid: validFor(boardId, parent) };
    }
    const id = tree.dataset.id!;
    const pk = tree.dataset.parentKind as Parent['kind'];
    const parent = (pk === 'root' ? { kind: 'root' } : { kind: pk, id: tree.dataset.parentId! }) as Parent;
    if (rel > 0.3 && rel < 0.7) {
      const into: Parent = { kind: 'card', id };
      return { type: 'tree', boardId, parent: into, before: null, zone: 'into', cardId: id, el: tree, valid: validFor(boardId, into, id) };
    }
    const zone: Zone = rel <= 0.3 ? 'before' : 'after';
    const before = zone === 'before' ? id : nextUndraggedSibling(tree, src.ids ?? []);
    return { type: 'tree', boardId, parent, before, zone, cardId: id, el: tree, valid: validFor(boardId, parent) };
  }

  // Lane reordering.
  if (src.kind === 'lane') {
    const laneEl = hit.closest<HTMLElement>('[data-lane]');
    if (!laneEl || laneEl.dataset.board !== src.boardId) return null;
    const r = laneEl.getBoundingClientRect();
    const horizontal = laneEl.dataset.flow !== 'rows';
    const after = horizontal ? x > r.left + r.width / 2 : y > r.top + r.height / 2;
    const idx = Number(laneEl.dataset.index);
    return {
      type: 'lane',
      boardId: src.boardId!,
      parent: { kind: 'root' },
      before: null,
      zone: after ? 'after' : 'before',
      laneIndex: after ? idx + 1 : idx,
      el: laneEl,
      valid: laneEl.dataset.lane !== src.laneId,
    };
  }

  // Cards.
  const cardEl = hit.closest<HTMLElement>('[data-card]');
  if (cardEl && !cardEl.classList.contains('dragging-source')) {
    const listEl = cardEl.parentElement?.closest<HTMLElement>('[data-list]') ?? null;
    const boardId = cardEl.dataset.board!;
    const id = cardEl.dataset.card!;
    const isGroupHeader = !!hit.closest('[data-group-head]');
    const area = isGroupHeader ? (cardEl.querySelector<HTMLElement>('[data-group-head]') ?? cardEl) : cardEl;
    const r = area.getBoundingClientRect();
    const axis = flowAxis(listEl);
    const rel = axis === 'y' ? (y - r.top) / r.height : (x - r.left) / r.width;
    const parent = listEl ? parentFromList(listEl) : ({ kind: 'root' } as Parent);
    const isGroup = cardEl.dataset.group === '1';
    if ((rel > 0.3 && rel < 0.7) || (isGroup && isGroupHeader && rel > 0.3)) {
      const into: Parent = { kind: 'card', id };
      return { type: 'card', boardId, parent: into, before: null, zone: 'into', cardId: id, el: area, valid: validFor(boardId, into, id) };
    }
    const zone: Zone = rel <= 0.3 ? 'before' : 'after';
    let before: string | null = id;
    if (zone === 'after') {
      const sibs = [...(listEl?.querySelectorAll<HTMLElement>(':scope > [data-card], :scope > .flip-item > [data-card]') ?? [])].map((e) => e.dataset.card!);
      const own = sibs.filter((s) => !dnd.source?.ids?.includes(s) || s === id);
      const i = own.indexOf(id);
      before = own[i + 1] ?? null;
      // Skip dragged items when computing "after".
      while (before && dnd.source?.ids?.includes(before)) before = own[own.indexOf(before) + 1] ?? null;
    }
    return { type: 'card', boardId, parent, before, zone, cardId: id, el: cardEl, valid: validFor(boardId, parent) };
  }

  // Empty space in a list: nearest position by flow axis. Padding around a
  // lane's list (or the lane header) counts as that lane's list.
  let listEl = hit.closest<HTMLElement>('[data-list]');
  if (!listEl) {
    const laneEl = hit.closest<HTMLElement>('[data-lane]');
    listEl = laneEl?.querySelector<HTMLElement>(':scope > .lbody > [data-list]') ?? null;
    if (!listEl && laneEl) {
      // Collapsed lane: append.
      const boardId = laneEl.dataset.board!;
      const parent: Parent = { kind: 'lane', id: laneEl.dataset.lane! };
      return { type: 'list', boardId, parent, before: null, zone: 'into', el: laneEl, valid: validFor(boardId, parent) };
    }
    const filesRoot = hit.closest<HTMLElement>('[data-files-root]');
    if (!listEl && filesRoot) listEl = filesRoot.querySelector<HTMLElement>('[data-list]');
  }
  if (listEl) {
    const boardId = listEl.dataset.board!;
    const parent = parentFromList(listEl);
    const axis = flowAxis(listEl);
    const items = [...listEl.querySelectorAll<HTMLElement>(':scope > [data-card], :scope > .flip-item > [data-card]')].filter(
      (e) => !dnd.source?.ids?.includes(e.dataset.card!),
    );
    let before: string | null = null;
    for (const it of items) {
      const r = it.getBoundingClientRect();
      const mid = axis === 'y' ? r.top + r.height / 2 : r.left + r.width / 2;
      if ((axis === 'y' ? y : x) < mid && (axis === 'y' || Math.abs(y - (r.top + r.height / 2)) < r.height)) {
        before = it.dataset.card!;
        break;
      }
    }
    return { type: 'list', boardId, parent, before, zone: 'before', el: listEl, valid: validFor(boardId, parent) };
  }
  return null;
}

function paintTarget(tg: DropTarget | null) {
  const ind = ensureIndicator();
  document.querySelectorAll('.dnd-into').forEach((e) => e.classList.remove('dnd-into'));
  document.querySelectorAll('.dnd-list-over').forEach((e) => e.classList.remove('dnd-list-over'));
  document.documentElement.dataset.dragging = tg && !tg.valid ? 'deny' : 'on';
  if (!tg || !tg.valid) {
    ind.style.opacity = '0';
    return;
  }
  if (tg.zone === 'into' || tg.type === 'custom') {
    tg.el.classList.add('dnd-into');
    ind.style.opacity = '0';
    return;
  }
  if (tg.type === 'lane') {
    const r = tg.el.getBoundingClientRect();
    const horizontal = tg.el.dataset.flow !== 'rows';
    const at = tg.zone === 'after' ? (horizontal ? r.right + 6 : r.bottom + 6) : horizontal ? r.left - 6 : r.top - 6;
    Object.assign(
      ind.style,
      horizontal
        ? { left: `${at - 1.5}px`, top: `${r.top}px`, width: '3px', height: `${r.height}px`, opacity: '1' }
        : { left: `${r.left}px`, top: `${at - 1.5}px`, width: `${r.width}px`, height: '3px', opacity: '1' },
    );
    return;
  }
  const listEl = tg.type === 'tree' ? null : tg.type === 'list' ? tg.el : (tg.el.parentElement?.closest<HTMLElement>('[data-list]') ?? null);
  if (listEl) listEl.classList.add('dnd-list-over');
  const axis = flowAxis(listEl);
  let rect: DOMRect;
  let edge: 'start' | 'end' = 'start';
  if (tg.type === 'tree') {
    rect = tg.el.getBoundingClientRect();
    edge = tg.zone === 'after' ? 'end' : 'start';
  } else if (tg.before) {
    const el = (listEl ?? document).querySelector<HTMLElement>(`[data-card="${tg.before}"]`);
    rect = (el ?? tg.el).getBoundingClientRect();
  } else {
    const items = listEl
      ? [...listEl.querySelectorAll<HTMLElement>(':scope > [data-card], :scope > .flip-item > [data-card]')].filter(
          (e) => !dnd.source?.ids?.includes(e.dataset.card!),
        )
      : [];
    const last = items[items.length - 1];
    if (last) {
      rect = last.getBoundingClientRect();
      edge = 'end';
    } else {
      rect = (listEl ?? tg.el).getBoundingClientRect();
      rect = new DOMRect(rect.left + 8, rect.top + 8, rect.width - 16, 0);
    }
  }
  const gap = 5;
  if (axis === 'y') {
    const yy = edge === 'start' ? rect.top - gap : rect.bottom + gap;
    Object.assign(ind.style, { left: `${rect.left + 4}px`, top: `${yy - 1.5}px`, width: `${rect.width - 8}px`, height: '3px', opacity: '1' });
  } else {
    const xx = edge === 'start' ? rect.left - gap : rect.right + gap;
    Object.assign(ind.style, { left: `${xx - 1.5}px`, top: `${rect.top + 4}px`, width: '3px', height: `${rect.height - 8}px`, opacity: '1' });
  }
}

function autoScroll() {
  cancelAnimationFrame(raf);
  const step = () => {
    if (!dnd.active) return;
    const edge = 56;
    for (const s of scrollers) {
      const r = s.getBoundingClientRect();
      if (dnd.x < r.left || dnd.x > r.right || dnd.y < r.top || dnd.y > r.bottom) continue;
      const dx = dnd.x < r.left + edge ? -(r.left + edge - dnd.x) : dnd.x > r.right - edge ? dnd.x - (r.right - edge) : 0;
      const dy = dnd.y < r.top + edge ? -(r.top + edge - dnd.y) : dnd.y > r.bottom - edge ? dnd.y - (r.bottom - edge) : 0;
      if (dx || dy) {
        s.scrollBy(dx * 0.35, dy * 0.35);
        update(dnd.x, dnd.y);
      }
    }
    raf = requestAnimationFrame(step);
  };
  raf = requestAnimationFrame(step);
}

function update(x: number, y: number) {
  dnd.x = x;
  dnd.y = y;
  if (ghost) ghost.style.transform = `translate(${x + 12}px, ${y + 10}px) rotate(1.5deg)`;
  const tg = computeTarget(x, y);
  dnd.target = tg;
  paintTarget(tg);
  // Hovering a collapsed group expands it.
  const gid = tg?.zone === 'into' && tg.cardId && boardUi.collapsed[tg.cardId] ? tg.cardId : null;
  if (gid !== hoverId) {
    hoverId = gid;
    if (hoverTimer) clearTimeout(hoverTimer);
    if (gid) hoverTimer = setTimeout(() => (boardUi.collapsed[gid] = false), 600);
  }
}

function cleanup() {
  dnd.active = false;
  cancelAnimationFrame(raf);
  ghost?.remove();
  ghost = null;
  if (indicator) indicator.style.opacity = '0';
  document.querySelectorAll('.dnd-into, .dnd-list-over').forEach((e) => e.classList.remove('dnd-into', 'dnd-list-over'));
  document.querySelectorAll('.dragging-source').forEach((e) => e.classList.remove('dragging-source'));
  delete document.documentElement.dataset.dragging;
  if (hoverTimer) clearTimeout(hoverTimer);
  hoverId = null;
}

async function drop(src: DragSource, tg: DropTarget) {
  if (tg.type === 'custom' || src.kind === 'external') {
    const key = tg.type === 'custom' ? tg.custom! : String((src.payload as { type?: string })?.type ?? 'external');
    const fn = externalHandlers.get(key) ?? externalHandlers.get(tg.custom ?? '');
    if (fn) await fn(src, tg);
    return;
  }
  if (src.kind === 'lane') {
    const b = boards.get(src.boardId!);
    if (!b) return;
    const from = b.lanes.findIndex((l) => l.id === src.laneId);
    let to = tg.laneIndex ?? from;
    if (to > from) to--;
    if (to === from) return;
    const lanes = [...b.lanes];
    const [l] = lanes.splice(from, 1);
    lanes.splice(to, 0, l);
    b.lanes = lanes;
    await apply(b.id, { op: 'moveLane', id: src.laneId!, index: to }, t('ops.moveLane'));
    return;
  }
  const ids = src.ids ?? [];
  if (!ids.length) return;
  if (src.copyOnly) {
    const fn = externalHandlers.get('copyFromMirror');
    if (fn) await fn(src, tg);
    return;
  }
  if (src.boardId === tg.boardId) {
    await apply(
      tg.boardId,
      { op: 'move', ids, to: tg.parent, before: tg.before },
      ids.length > 1 ? t('ops.moveCards', { count: ids.length }) : t('ops.moveCard'),
    );
  } else {
    try {
      await rpc('board.moveAcross', { from: src.boardId, ids, to: tg.boardId, parent: tg.parent, before: tg.before });
      toast.success(t('toasts.movedToBoard', { count: ids.length, board: boards.get(tg.boardId)?.header.name ?? '' }));
    } catch (e) {
      toast.error(t('errors.moveAcross', { message: (e as Error).message }));
    }
  }
}

/** "After" in the explorer tree: the next sibling row that is not being dragged
 *  (dropping "before" a dragged card makes the move fail). */
function nextUndraggedSibling(row: HTMLElement, dragged: string[]): string | null {
  const next = row.dataset.next ?? null;
  if (!next || !dragged.includes(next)) return next;
  const same = [...document.querySelectorAll<HTMLElement>('[data-tree][data-id]')].filter(
    (r) => r.dataset.board === row.dataset.board && r.dataset.parentKind === row.dataset.parentKind && r.dataset.parentId === row.dataset.parentId,
  );
  const ids = same.map((r) => r.dataset.id!);
  for (let i = ids.indexOf(row.dataset.id!) + 1; i < ids.length; i++) if (!dragged.includes(ids[i])) return ids[i];
  return null;
}

/** Begin tracking a potential drag from a pointerdown. */
export function startDrag(e: PointerEvent, source: DragSource, sourceEl: HTMLElement | null, scrollEls: HTMLElement[] = []) {
  if (e.button !== 0) return;
  const sx = e.clientX;
  const sy = e.clientY;
  let started = false;
  const move = (ev: PointerEvent) => {
    if (!started) {
      if (Math.hypot(ev.clientX - sx, ev.clientY - sy) < 5) return;
      started = true;
      dnd.active = true;
      cmdCtx.dragging = true;
      dnd.source = source;
      dnd.count = source.count ?? source.ids?.length ?? 1;
      scrollers = [...scrollEls, ...document.querySelectorAll<HTMLElement>('[data-autoscroll]')];
      makeGhost(sourceEl, dnd.count, source.label);
      for (const id of source.ids ?? []) document.querySelectorAll(`[data-card="${id}"]`).forEach((el) => el.classList.add('dragging-source'));
      if (source.kind === 'lane') sourceEl?.classList.add('dragging-source');
      window.getSelection()?.removeAllRanges();
      autoScroll();
    }
    update(ev.clientX, ev.clientY);
  };
  const up = async (ev: PointerEvent) => {
    teardown();
    if (!started) return;
    // Swallow the click the browser fires right after this pointerup — and
    // only that one (registered before any await; dropped if it never comes).
    const swallow = (c: MouseEvent) => c.stopPropagation();
    window.addEventListener('click', swallow, { capture: true, once: true });
    setTimeout(() => window.removeEventListener('click', swallow, { capture: true }), 0);
    update(ev.clientX, ev.clientY);
    const tg = dnd.target;
    const src = dnd.source;
    cancel();
    if (src && tg?.valid) await drop(src, tg).catch((err) => toast.error(String(err)));
  };
  /** Abort without dropping (Escape, pointer cancelled, window lost focus). */
  const cancel = () => {
    teardown();
    cleanup();
    dnd.source = null;
    dnd.target = null;
    cmdCtx.dragging = false;
  };
  const key = (ev: KeyboardEvent) => {
    if (ev.key === 'Escape' && started) {
      ev.preventDefault();
      ev.stopPropagation();
      cancel();
    }
  };
  const abort = () => {
    if (started) cancel();
    else teardown();
  };
  const teardown = () => {
    window.removeEventListener('pointermove', move);
    window.removeEventListener('pointerup', up);
    window.removeEventListener('pointercancel', abort);
    window.removeEventListener('blur', abort);
    window.removeEventListener('keydown', key, true);
  };
  window.addEventListener('pointermove', move);
  window.addEventListener('pointerup', up);
  window.addEventListener('pointercancel', abort);
  window.addEventListener('blur', abort);
  window.addEventListener('keydown', key, true);
}
