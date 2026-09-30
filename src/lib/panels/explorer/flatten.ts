// Explorer tree → flat list of visible rows (depth-annotated) for the
// virtualized list. Pure: only walks expanded nodes, so cost is proportional to
// what is on screen-ish, not to the size of every board.

import type { BoardEntry, BoardKind, LaneDto, NodeDto, Parent } from '$lib/backend/types';

export type Section = 'boards' | 'mirrors';
export type RowType = 'section' | 'board' | 'lane' | 'card' | 'loading' | 'error' | 'empty' | 'hint';

export interface Row {
  key: string;
  type: RowType;
  depth: number;
  parentKey: string | null;
  boardId: string;
  /** Section / board / lane / card id. */
  id: string;
  label: string;
  expandable: boolean;
  expanded: boolean;
  section?: Section;
  entry?: BoardEntry;
  kind?: BoardKind;
  firstLane?: string;
  mirror?: boolean;
  parent?: Parent;
  /** Next visible sibling (for "drop after"). */
  next?: string;
  isGroup?: boolean;
  archived?: boolean;
  count?: number;
  color?: string | null;
  remoteKey?: string | null;
  /** Duplicate path for hint rows. */
  path?: string;
}

/** Minimal view of a BoardModel the flattener needs. */
export interface TreeBoard {
  kind: BoardKind;
  lanes: LaneDto[];
  rootOrder: string[];
  nodes: { get(id: string): NodeDto | undefined; readonly size: number };
  /** Bumped on every applied delta (memoizes counts). */
  version: number;
  remote?: { get(id: string): { key: string } | undefined };
}

export interface FlattenInput {
  sections: { id: Section; label: string; boards: BoardEntry[]; show: boolean }[];
  board: (id: string) => TreeBoard | undefined;
  expanded: { has(key: string): boolean };
  showArchived: boolean;
  loading: { has(id: string): boolean };
  errors: { has(id: string): boolean };
  untitled: string;
}

export const sectionKey = (s: Section) => `s:${s}`;
/** Sections are expanded by default: the set stores the *collapsed* marker. */
export const sectionCollapsedKey = (s: Section) => `s:${s}:collapsed`;
export const boardKey = (b: string) => `b:${b}`;
export const laneKey = (b: string, l: string) => `l:${b}:${l}`;
export const cardKey = (b: string, c: string) => `c:${b}:${c}`;

export function flatten(inp: FlattenInput): Row[] {
  const rows: Row[] = [];
  const { expanded, showArchived, untitled } = inp;

  const visibleIds = (m: TreeBoard, ids: string[]) =>
    showArchived ? ids.filter((id) => m.nodes.get(id)) : ids.filter((id) => { const n = m.nodes.get(id); return n && !n.archived; });

  const pushCards = (boardId: string, m: TreeBoard, parent: Parent, ids: string[], depth: number, parentKey: string) => {
    const list = visibleIds(m, ids);
    for (let i = 0; i < list.length; i++) {
      const n = m.nodes.get(list[i])!;
      const key = cardKey(boardId, n.id);
      const kids = n.children.length ? visibleIds(m, n.children) : [];
      const open = kids.length > 0 && expanded.has(key);
      rows.push({
        key,
        type: 'card',
        depth,
        parentKey,
        boardId,
        id: n.id,
        label: n.title || untitled,
        expandable: kids.length > 0,
        expanded: open,
        parent,
        next: list[i + 1] ?? '',
        isGroup: n.isGroup,
        archived: n.archived,
        count: kids.length || undefined,
        remoteKey: m.remote?.get(n.id)?.key ?? n.remote?.key ?? null,
        kind: m.kind,
      });
      if (open) pushCards(boardId, m, { kind: 'card', id: n.id }, kids, depth + 1, key);
    }
  };

  for (const sec of inp.sections) {
    if (!sec.show) continue;
    const sKey = sectionKey(sec.id);
    const sOpen = !expanded.has(sectionCollapsedKey(sec.id));
    rows.push({ key: sKey, type: 'section', depth: 0, parentKey: null, boardId: '', id: sec.id, label: sec.label, expandable: true, expanded: sOpen, section: sec.id, count: sec.boards.length });
    if (!sOpen) continue;
    for (const e of sec.boards) {
      const bKey = boardKey(e.id);
      const m = e.missing ? undefined : inp.board(e.id);
      const open = !e.missing && expanded.has(bKey);
      const lanes = m && m.kind !== 'files' ? m.lanes.filter((l) => showArchived || !l.archived) : [];
      rows.push({
        key: bKey,
        type: 'board',
        depth: 1,
        parentKey: sKey,
        boardId: e.id,
        id: e.id,
        label: e.name,
        expandable: !e.missing,
        expanded: open,
        section: sec.id,
        entry: e,
        kind: m?.kind ?? e.kind,
        firstLane: lanes[0]?.id ?? '',
        mirror: e.mirror,
        count: m ? countCards(m, showArchived) : undefined,
      });
      for (const path of e.duplicates ?? []) {
        rows.push({ key: `h:${e.id}:${path}`, type: 'hint', depth: 2, parentKey: bKey, boardId: e.id, id: e.id, label: path, expandable: false, expanded: false, path });
      }
      if (!open) continue;
      if (!m) {
        const err = inp.errors.has(e.id);
        rows.push({ key: `x:${e.id}`, type: err ? 'error' : 'loading', depth: 2, parentKey: bKey, boardId: e.id, id: e.id, label: '', expandable: false, expanded: false });
        continue;
      }
      if (m.kind === 'files') {
        if (!visibleIds(m, m.rootOrder).length) rows.push({ key: `x:${e.id}`, type: 'empty', depth: 2, parentKey: bKey, boardId: e.id, id: e.id, label: '', expandable: false, expanded: false });
        pushCards(e.id, m, { kind: 'root' }, m.rootOrder, 2, bKey);
        continue;
      }
      if (!lanes.length) rows.push({ key: `x:${e.id}`, type: 'empty', depth: 2, parentKey: bKey, boardId: e.id, id: e.id, label: '', expandable: false, expanded: false });
      for (const l of lanes) {
        const lKey = laneKey(e.id, l.id);
        const kids = visibleIds(m, l.order);
        const lOpen = kids.length > 0 && expanded.has(lKey);
        rows.push({
          key: lKey,
          type: 'lane',
          depth: 2,
          parentKey: bKey,
          boardId: e.id,
          id: l.id,
          label: l.name || untitled,
          expandable: kids.length > 0,
          expanded: lOpen,
          color: l.color,
          archived: l.archived,
          count: kids.length,
          kind: m.kind,
        });
        if (lOpen) pushCards(e.id, m, { kind: 'lane', id: l.id }, kids, 3, lKey);
      }
    }
  }
  return rows;
}

const counts = new WeakMap<TreeBoard, { v: number; a: boolean; size: number; n: number }>();

function countCards(m: TreeBoard, showArchived: boolean): number {
  if (showArchived) return m.nodes.size;
  const c = counts.get(m);
  if (c && c.v === m.version && c.size === m.nodes.size && c.a === showArchived) return c.n;
  const n = countWalk(m);
  counts.set(m, { v: m.version, a: showArchived, size: m.nodes.size, n });
  return n;
}

function countWalk(m: TreeBoard): number {
  let n = 0;
  const walk = (ids: string[]) => {
    for (const id of ids) {
      const c = m.nodes.get(id);
      if (!c || c.archived) continue;
      n++;
      if (c.children.length) walk(c.children);
    }
  };
  if (m.kind === 'files') walk(m.rootOrder);
  else for (const l of m.lanes) if (!l.archived) walk(l.order);
  return n;
}

/** Keys of every ancestor needed to make a card visible. */
export function ancestorKeys(boardId: string, cardId: string, m: TreeBoard): string[] {
  const keys: string[] = [];
  let n = m.nodes.get(cardId);
  const seen = new Set<string>();
  while (n && !seen.has(n.id)) {
    seen.add(n.id);
    const p: Parent = n.parent;
    if (p.kind === 'card') {
      keys.push(cardKey(boardId, p.id));
      n = m.nodes.get(p.id);
    } else {
      if (p.kind === 'lane') keys.push(laneKey(boardId, p.id));
      break;
    }
  }
  keys.push(boardKey(boardId));
  return keys;
}
