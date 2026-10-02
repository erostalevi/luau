// Reactive board models fed by snapshots + deltas from the backend.

import { SvelteMap } from 'svelte/reactivity';
import { rpc, onCoreEvent } from '$lib/backend/rpc';
import type { ApplyResult, BoardDelta, BoardHeader, BoardSnapshot, LaneDto, NodeDto, Op, Parent, RemoteInfo, UndoResult } from '$lib/backend/types';
import { toast } from './toasts.svelte';
import { t } from '$lib/i18n/index.svelte';

export class BoardModel {
  id: string;
  header = $state<BoardHeader>(null!);
  lanes = $state<LaneDto[]>([]);
  rootOrder = $state<string[]>([]);
  nodes = new SvelteMap<string, NodeDto>();
  remote = new SvelteMap<string, RemoteInfo>();
  version = $state(0);
  undo = $state({ canUndo: false, canRedo: false, undoLabel: null as string | null, redoLabel: null as string | null });
  /** Ids changed externally since last seen (for editor merge prompts). */
  externalTick = $state(0);
  externalIds = $state<string[]>([]);

  constructor(snap: BoardSnapshot) {
    this.id = snap.header.id;
    this.load(snap);
  }

  load(snap: BoardSnapshot) {
    this.header = snap.header;
    this.lanes = snap.lanes;
    this.rootOrder = snap.rootOrder;
    this.nodes.clear();
    for (const n of snap.nodes) this.nodes.set(n.id, n);
    this.version = snap.version;
  }

  applyDelta(d: BoardDelta) {
    if (d.header) this.header = d.header;
    if (d.lanes) this.lanes = d.lanes;
    if (d.rootOrder) this.rootOrder = d.rootOrder;
    for (const n of d.nodes) this.nodes.set(n.id, n);
    for (const id of d.removed) this.nodes.delete(id);
    this.version = Math.max(this.version, d.version);
  }

  get kind() {
    return this.header.kind;
  }

  lane(id: string): LaneDto | undefined {
    return this.lanes.find((l) => l.id === id);
  }

  node(id: string): NodeDto | undefined {
    return this.nodes.get(id);
  }

  childrenOf(p: Parent): string[] {
    if (p.kind === 'lane') return this.lane(p.id)?.order ?? [];
    if (p.kind === 'card') return this.nodes.get(p.id)?.children ?? [];
    return this.rootOrder;
  }

  laneOf(id: string): string | null {
    let n = this.nodes.get(id);
    while (n) {
      if (n.parent.kind === 'lane') return n.parent.id;
      if (n.parent.kind === 'root') return null;
      n = this.nodes.get(n.parent.id);
    }
    return null;
  }

  /** Archived by itself, an ancestor, or its lane. */
  effectivelyArchived(id: string): boolean {
    let n = this.nodes.get(id);
    while (n) {
      if (n.archived) return true;
      if (n.parent.kind === 'lane') return !!this.lane(n.parent.id)?.archived;
      if (n.parent.kind === 'root') return false;
      n = this.nodes.get(n.parent.id);
    }
    return false;
  }

  isAncestor(anc: string, id: string): boolean {
    let cur: string | null = id;
    while (cur) {
      if (cur === anc) return true;
      const n: NodeDto | undefined = this.nodes.get(cur);
      cur = n && n.parent.kind === 'card' ? n.parent.id : null;
    }
    return false;
  }

  descendants(id: string): string[] {
    const out: string[] = [];
    const stack = [...(this.nodes.get(id)?.children ?? [])];
    while (stack.length) {
      const c = stack.pop()!;
      out.push(c);
      stack.push(...(this.nodes.get(c)?.children ?? []));
    }
    return out;
  }

  /** Path of ancestors (root first), for breadcrumbs. */
  ancestors(id: string): NodeDto[] {
    const out: NodeDto[] = [];
    let n = this.nodes.get(id);
    while (n && n.parent.kind === 'card') {
      const p = this.nodes.get(n.parent.id);
      if (!p) break;
      out.unshift(p);
      n = p;
    }
    return out;
  }

  /** Optimistic local move (replaced by the authoritative delta). */
  optimisticMove(ids: string[], to: Parent, before: string | null) {
    const moving = ids.filter((id) => this.nodes.has(id) && !ids.some((o) => o !== id && this.isAncestor(o, id)));
    if (to.kind === 'card' && moving.some((id) => this.isAncestor(id, to.id))) return;
    const touched = new Set<string>();
    for (const id of moving) {
      const n = this.nodes.get(id)!;
      this.detach(id, n.parent);
      touched.add(JSON.stringify(n.parent));
    }
    const list = [...this.childrenOf(to)];
    let at = before ? list.indexOf(before) : list.length;
    if (at < 0) at = list.length;
    list.splice(at, 0, ...moving);
    this.setChildren(to, list);
    for (const id of moving) {
      const n = this.nodes.get(id)!;
      this.nodes.set(id, { ...n, parent: to });
    }
    if (to.kind === 'card') {
      const t = this.nodes.get(to.id);
      if (t) this.nodes.set(to.id, { ...t, isGroup: true, children: list });
    }
  }

  private detach(id: string, p: Parent) {
    const list = this.childrenOf(p).filter((c) => c !== id);
    this.setChildren(p, list);
    if (p.kind === 'card' && list.length === 0) {
      const g = this.nodes.get(p.id);
      if (g) this.nodes.set(p.id, { ...g, isGroup: false, children: [] });
    }
  }

  private setChildren(p: Parent, list: string[]) {
    if (p.kind === 'lane') {
      this.lanes = this.lanes.map((l) => (l.id === p.id ? { ...l, order: list } : l));
    } else if (p.kind === 'card') {
      const n = this.nodes.get(p.id);
      if (n) this.nodes.set(p.id, { ...n, children: list, isGroup: list.length > 0 });
    } else {
      this.rootOrder = list;
    }
  }
}

export const boards = new SvelteMap<string, BoardModel>();
const opening = new Map<string, Promise<BoardModel>>();

export function initBoardEvents() {
  onCoreEvent((e) => {
    if (e.type === 'boardDelta') boards.get(e.delta.boardId)?.applyDelta(e.delta);
    else if (e.type === 'undoState') {
      const b = boards.get(e.boardId);
      if (b) b.undo = { canUndo: e.canUndo, canRedo: e.canRedo, undoLabel: e.undoLabel, redoLabel: e.redoLabel };
    } else if (e.type === 'externalChange') {
      const b = boards.get(e.boardId);
      if (b) {
        b.externalIds = e.ids;
        b.externalTick++;
      }
    }
  });
}

export async function openBoard(ref: { id?: string; path?: string }): Promise<BoardModel> {
  const key = ref.id ?? ref.path ?? '';
  const existing = ref.id ? boards.get(ref.id) : undefined;
  if (existing) return existing;
  const pending = opening.get(key);
  if (pending) return pending;
  const p = (async () => {
    const snap = await rpc<BoardSnapshot>('board.open', ref.id ? { id: ref.id } : { path: ref.path });
    let m = boards.get(snap.header.id);
    if (m) m.load(snap);
    else {
      m = new BoardModel(snap);
      boards.set(m.id, m);
    }
    return m;
  })();
  opening.set(key, p);
  try {
    return await p;
  } finally {
    opening.delete(key);
  }
}

export async function reloadBoard(id: string) {
  const snap = await rpc<BoardSnapshot>('board.snapshot', { id });
  boards.get(id)?.load(snap);
}

export async function apply(board: string, op: Op, label?: string, coalesce?: string): Promise<ApplyResult | null> {
  const m = boards.get(board);
  if (m && op.op === 'move') m.optimisticMove(op.ids, op.to, op.before);
  try {
    return await rpc<ApplyResult>('board.apply', { board, op, label: label ?? op.op, coalesce: coalesce ?? null });
  } catch (e) {
    toast.error(t('errors.opFailed', { message: (e as Error).message }));
    await reloadBoard(board).catch(() => {});
    return null;
  }
}

/** Last AI assistant run that changed several boards: ⌘Z undoes all of it at once. */
let aiRun: { label: string; boards: string[] } | null = null;

export function rememberAiRun(label: string, ids: string[]) {
  aiRun = ids.length > 1 ? { label, boards: ids } : null;
}

export async function undo(board: string): Promise<UndoResult | null> {
  const run = aiRun;
  if (run && run.boards.includes(board) && boards.get(board)?.undo.undoLabel === run.label) {
    aiRun = null;
    // Undo the run on every board whose newest step is still that run.
    const others = run.boards.filter((b) => b !== board && boards.get(b)?.undo.undoLabel === run.label);
    const first = await rpc<UndoResult>('board.undo', { board }).catch((e: Error) => (toast.warn(t('errors.undoFailed', { message: e.message })), null));
    for (const b of others) await rpc<UndoResult>('board.undo', { board: b }).catch(() => null);
    return first;
  }
  try {
    return await rpc<UndoResult>('board.undo', { board });
  } catch (e) {
    toast.warn(t('errors.undoFailed', { message: (e as Error).message }));
    return null;
  }
}

export async function redo(board: string): Promise<UndoResult | null> {
  try {
    return await rpc<UndoResult>('board.redo', { board });
  } catch (e) {
    toast.warn(t('errors.undoFailed', { message: (e as Error).message }));
    return null;
  }
}

export async function newCardId(): Promise<string> {
  return rpc<string>('board.newCardId');
}

export async function newLaneId(board: string): Promise<string> {
  return rpc<string>('board.newLaneId', { board });
}
