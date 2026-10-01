// Pure helpers for the history timeline: classify journal entries, describe them
// with human titles (never ids), group by day and filter. No Svelte / i18n here:
// `describeEntry` returns an i18n key suffix + params that the view translates
// under `history.desc.*`.

import type { JournalEntry } from '$lib/backend/types';

export type KindGroup = 'created' | 'moved' | 'edited' | 'deleted' | 'archived' | 'lanes' | 'external' | 'integrations' | 'other';

export const KIND_GROUPS: KindGroup[] = ['created', 'moved', 'edited', 'deleted', 'archived', 'lanes', 'external', 'integrations'];

const KIND_TO_GROUP: Record<string, KindGroup> = {
  createCard: 'created',
  move: 'moved',
  place: 'moved',
  moveBoard: 'moved',
  edit: 'edited',
  writeCard: 'edited',
  setCover: 'edited',
  trash: 'deleted',
  restore: 'deleted',
  purge: 'deleted',
  setArchived: 'archived',
  createLane: 'lanes',
  updateLane: 'lanes',
  moveLane: 'lanes',
};

const REMOTE_KIND = /^(remote|jira|trello|slack|github|linear|integration)/i;
const SERVICE_KIND = /^(jira|trello|slack|github|linear)/i;

export function kindGroup(e: Pick<JournalEntry, 'kind' | 'origin'>): KindGroup {
  if (e.origin === 'remote' || REMOTE_KIND.test(e.kind)) return 'integrations';
  if (e.origin === 'external' || e.kind.startsWith('external')) return 'external';
  return KIND_TO_GROUP[e.kind] ?? 'other';
}

type Obj = Record<string, unknown>;
const isObj = (v: unknown): v is Obj => !!v && typeof v === 'object' && !Array.isArray(v);
const str = (v: unknown): string | null => (typeof v === 'string' && v.trim() ? v : null);

/**
 * Who caused the change, as a filterable source: `you`, `external` or the
 * service name for remote changes (`jira`, `trello`, … or `remote` when unknown).
 * Integrations are expected to put `details.service` on their journal entries.
 */
export function sourceOf(e: Pick<JournalEntry, 'kind' | 'origin' | 'details'>): string {
  if (e.origin === 'you' && !REMOTE_KIND.test(e.kind)) return 'you';
  if (e.origin === 'external') return 'external';
  const d = isObj(e.details) ? e.details : {};
  const svc = str(d.service) ?? str(d.provider);
  if (svc && /^[a-z][a-z0-9-]{0,31}$/i.test(svc)) return svc.toLowerCase();
  const m = SERVICE_KIND.exec(e.kind);
  return m ? m[1].toLowerCase() : 'remote';
}

// --- details --------------------------------------------------------------------

export interface ItemCtx {
  id?: string;
  title?: string | null;
  lane?: string | null;
  parent?: string | null;
  archived?: boolean;
}

interface Ctx {
  items?: ItemCtx[];
  to?: { lane?: string | null; card?: string | null; root?: boolean };
  lanes?: unknown[];
  lane?: string | null;
  patch?: Record<string, unknown>;
  kind?: string;
}

function ctxOf(v: unknown): Ctx {
  return isObj(v) ? (v as Ctx) : {};
}

/** Items touched by an entry, merging before/after context (after wins, before fills gaps). */
export function entryItems(e: JournalEntry): ItemCtx[] {
  const d = isObj(e.details) ? e.details : {};
  const before = ctxOf(d.before).items ?? [];
  const after = ctxOf(d.after).items ?? [];
  const n = Math.max(before.length, after.length);
  const out: ItemCtx[] = [];
  for (let i = 0; i < n; i++) {
    const b = isObj(before[i]) ? before[i] : {};
    const a = isObj(after[i]) ? after[i] : {};
    out.push({
      id: str(a.id) ?? str(b.id) ?? undefined,
      title: str(a.title) ?? str(b.title),
      lane: str(a.lane) ?? str(b.lane),
      parent: str(a.parent) ?? str(b.parent),
      archived: typeof a.archived === 'boolean' ? a.archived : typeof b.archived === 'boolean' ? b.archived : undefined,
    });
  }
  return out;
}

/** Human titles of the cards an entry touched (from details, else the resolver; never ids). */
export function entryTitles(e: JournalEntry, titleOf: (id: string) => string | null = () => null): string[] {
  const titles = entryItems(e)
    .map((i) => i.title)
    .filter((x): x is string => !!x);
  if (titles.length) return titles;
  const d = isObj(e.details) ? e.details : {};
  if (Array.isArray(d.titles)) {
    const list = d.titles.filter((x): x is string => typeof x === 'string' && !!x.trim());
    if (list.length) return list;
  }
  const t = str(d.title);
  if (t) return [t];
  return e.ids
    .filter((id) => id.startsWith('c'))
    .map(titleOf)
    .filter((x): x is string => !!x);
}

/** The card worth opening / filtering by for an entry (null for lane/board entries). */
export function entryCardId(e: JournalEntry): string | null {
  const g = kindGroup(e);
  if (g === 'lanes') return null;
  if (e.kind === 'updateBoard' || e.kind === 'setKind') return null;
  const it = entryItems(e).find((i) => i.id);
  if (it?.id) return it.id;
  return e.ids.find((id) => /^c[a-z0-9]{6}$/.test(id)) ?? null;
}

/** Entry carries text versions that can be diffed / restored. */
export function hasVersions(e: JournalEntry): boolean {
  return !!(e.before || e.after);
}

export interface Described {
  /** Suffix under `history.desc.*`. */
  key: string;
  params: Record<string, string | number>;
  group: KindGroup;
  /** Short secondary line pieces (lane / parent card). */
  lane?: string | null;
  parent?: string | null;
  /** +/- characters for edits. */
  chars?: number | null;
}

/** Translate a kind + details into a description key and params. */
export function describeEntry(
  e: JournalEntry,
  boardName: (id: string) => string | null = () => null,
  titleOf: (id: string) => string | null = () => null,
): Described {
  const group = kindGroup(e);
  const d = isObj(e.details) ? e.details : {};
  const before = ctxOf(d.before);
  const after = ctxOf(d.after);
  const items = entryItems(e);
  const titles = entryTitles(e, titleOf);
  const title = titles[0] ?? '';
  const count = Math.max(titles.length, items.length, 1);
  const many = count > 1;
  const first = items[0];
  const base = {
    group,
    lane: first?.lane ?? null,
    parent: first?.parent ?? null,
  };
  const p = (extra: Record<string, string | number> = {}) => ({
    title,
    count,
    ...extra,
  });

  switch (e.kind) {
    case 'createCard':
      return {
        ...base,
        key: title ? 'created' : 'createdUntitled',
        params: p(),
      };
    case 'move':
    case 'place': {
      const to = after.to ?? before.to;
      const toLane = str(to?.lane);
      const toCard = str(to?.card);
      const from = str(before.items?.[0]?.lane);
      if (e.kind === 'place') return { ...base, key: many ? 'placedMany' : 'placed', params: p() };
      if (toCard)
        return {
          ...base,
          key: many ? 'nestedMany' : 'nested',
          params: p({ target: toCard }),
        };
      if (toLane) {
        if (from && from !== toLane && !many)
          return {
            ...base,
            lane: toLane,
            key: 'movedFromTo',
            params: p({ from, to: toLane }),
          };
        return {
          ...base,
          lane: toLane,
          key: many ? 'movedManyTo' : 'movedTo',
          params: p({ to: toLane }),
        };
      }
      if (to?.root)
        return {
          ...base,
          key: many ? 'movedManyTop' : 'movedTop',
          params: p(),
        };
      return { ...base, key: many ? 'movedMany' : 'moved', params: p() };
    }
    case 'moveBoard': {
      const fromB = str(d.from);
      const toB = str(d.to);
      const incoming = toB === e.board;
      const other = incoming ? fromB : toB;
      const name = (other && boardName(other)) || '';
      return {
        ...base,
        key: (incoming ? 'movedIn' : 'movedOut') + (name ? '' : 'Unknown'),
        params: p({ board: name }),
      };
    }
    case 'edit':
    case 'writeCard': {
      const chars = typeof d.chars === 'number' ? d.chars : null;
      return {
        ...base,
        key: title ? 'edited' : 'editedUntitled',
        params: p(),
        chars,
      };
    }
    case 'setCover':
      return { ...base, key: 'cover', params: p() };
    case 'trash': {
      const lanes = (before.lanes ?? []).map(str).filter((x): x is string => !!x);
      if (!items.length && lanes.length)
        return {
          ...base,
          key: 'laneDeleted',
          params: { lane: lanes[0], count: lanes.length },
        };
      return { ...base, key: many ? 'deletedMany' : 'deleted', params: p() };
    }
    case 'restore':
      return {
        ...base,
        key: title ? (many ? 'restoredMany' : 'restored') : 'restoredUntitled',
        params: p(),
      };
    case 'purge':
      return {
        ...base,
        key: title ? (many ? 'purgedMany' : 'purged') : 'purgedUntitled',
        params: p(),
      };
    case 'setArchived': {
      const laneItems = (after.lanes ?? before.lanes ?? []).filter(isObj) as {
        lane?: string | null;
        archived?: boolean;
      }[];
      if (!items.length && laneItems.length) {
        const l = laneItems[0];
        return {
          ...base,
          key: l.archived === false ? 'laneUnarchived' : 'laneArchived',
          params: { lane: str(l.lane) ?? '', count: laneItems.length },
        };
      }
      const archived = first?.archived !== false;
      return {
        ...base,
        key: (archived ? 'archived' : 'unarchived') + (many ? 'Many' : ''),
        params: p(),
      };
    }
    case 'createLane':
      return {
        ...base,
        key: 'laneCreated',
        params: { lane: str(after.lane) ?? str(before.lane) ?? '' },
      };
    case 'updateLane': {
      const patch = isObj(after.patch) ? after.patch : isObj(before.patch) ? before.patch : {};
      const lane = str(before.lane) ?? str(after.lane) ?? '';
      if (str(patch.name))
        return {
          ...base,
          key: 'laneRenamed',
          params: { from: lane, to: String(patch.name) },
        };
      if ('color' in patch) return { ...base, key: 'laneColor', params: { lane } };
      if ('wip' in patch)
        return {
          ...base,
          key: 'laneWip',
          params: { lane, wip: Number(patch.wip) || 0 },
        };
      if ('collapsed' in patch)
        return {
          ...base,
          key: patch.collapsed ? 'laneCollapsed' : 'laneExpanded',
          params: { lane },
        };
      return { ...base, key: 'laneUpdated', params: { lane } };
    }
    case 'moveLane':
      return {
        ...base,
        key: 'laneMoved',
        params: { lane: str(before.lane) ?? str(after.lane) ?? '' },
      };
    case 'updateBoard': {
      const patch = isObj(after.patch) ? after.patch : {};
      if (str(patch.name))
        return {
          ...base,
          key: 'boardRenamed',
          params: { name: String(patch.name) },
        };
      return { ...base, key: 'boardUpdated', params: {} };
    }
    case 'setKind':
      return { ...base, key: 'kindChanged', params: {} };
    case 'undo':
    case 'redo':
      return {
        ...base,
        key: e.kind,
        params: { label: e.label.replace(/^(Undo|Redo):\s*/i, '') },
      };
    default:
      if (group === 'external') {
        const removed = typeof d.removed === 'number' ? d.removed : 0;
        if (!titles.length)
          return {
            ...base,
            key: removed ? 'externalRemoved' : 'externalGeneric',
            params: { count: removed },
          };
        return {
          ...base,
          key: many ? 'externalMany' : 'external',
          params: p(),
        };
      }
      if (group === 'integrations') {
        const service = sourceOf(e);
        return {
          ...base,
          key: title ? (many ? 'remoteMany' : 'remote') : 'remoteGeneric',
          params: p({ service }),
        };
      }
      return { ...base, key: 'other', params: p({ label: e.label }) };
  }
}

// --- grouping -------------------------------------------------------------------

export interface DayGroup<T> {
  /** Local date `YYYY-MM-DD`. */
  key: string;
  rel: 'today' | 'yesterday' | 'date';
  entries: T[];
}

export function localDayKey(d: Date): string {
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, '0');
  const day = String(d.getDate()).padStart(2, '0');
  return `${y}-${m}-${day}`;
}

/** Group entries (any order in, newest-first out) by local calendar day. */
export function groupByDay<T extends { ts: string }>(entries: T[], now: Date = new Date()): DayGroup<T>[] {
  const today = localDayKey(now);
  const y = new Date(now);
  y.setDate(y.getDate() - 1);
  const yesterday = localDayKey(y);
  const sorted = [...entries].sort((a, b) => Date.parse(b.ts) - Date.parse(a.ts));
  const out: DayGroup<T>[] = [];
  for (const e of sorted) {
    const t = Date.parse(e.ts);
    if (Number.isNaN(t)) continue;
    const key = localDayKey(new Date(t));
    let g = out[out.length - 1];
    if (!g || g.key !== key) {
      g = {
        key,
        rel: key === today ? 'today' : key === yesterday ? 'yesterday' : 'date',
        entries: [],
      };
      out.push(g);
    }
    g.entries.push(e);
  }
  return out;
}

// --- filtering ------------------------------------------------------------------

export type RangePreset = 'all' | 'today' | 'week' | 'month';

/** Inclusive ISO lower bound for a timeline range preset (local calendar days). */
export function rangeFrom(preset: RangePreset, now: Date = new Date()): string | undefined {
  const d = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  switch (preset) {
    case 'today':
      return d.toISOString();
    case 'week':
      d.setDate(d.getDate() - 6);
      return d.toISOString();
    case 'month':
      d.setDate(d.getDate() - 29);
      return d.toISOString();
    default:
      return undefined;
  }
}

/** Case- and accent-insensitive folding for text search. */
export function fold(s: string): string {
  return s.normalize('NFD').replace(/[̀-ͯ]/g, '').toLowerCase();
}

export interface EntryFilter {
  groups?: KindGroup[];
  /** Values from `sourceOf` (`you`, `external`, `jira`, …). */
  sources?: string[];
  text?: string;
}

/** Client-side filter (kind groups, sources and free text; dates/ids/boards go to the backend). */
export function filterEntries(
  entries: JournalEntry[],
  f: EntryFilter,
  boardName: (id: string) => string | null = () => null,
  titleOf: (id: string) => string | null = () => null,
): JournalEntry[] {
  const groups = f.groups?.length ? new Set(f.groups) : null;
  const sources = f.sources?.length ? new Set(f.sources) : null;
  const q = fold((f.text ?? '').trim());
  return entries.filter((e) => {
    if (groups && !groups.has(kindGroup(e))) return false;
    if (sources && !sources.has(sourceOf(e))) return false;
    if (q) {
      const items = entryItems(e);
      const hay = fold([e.label, ...entryTitles(e, titleOf), boardName(e.board) ?? '', ...items.map((i) => `${i.lane ?? ''} ${i.parent ?? ''}`)].join(' '));
      if (!q.split(/\s+/).every((w) => hay.includes(w))) return false;
    }
    return true;
  });
}

// --- trash ----------------------------------------------------------------------

const DAY_MS = 86_400_000;

/** Whole days left before a trash entry is purged (0 = goes at the next cleanup). */
export function daysLeft(deletedAt: string, ttlDays: number, now: number = Date.now()): number {
  const t = Date.parse(deletedAt);
  if (Number.isNaN(t)) return 0;
  const ttl = Math.max(1, Math.floor(ttlDays) || 1);
  return Math.max(0, Math.ceil((t + ttl * DAY_MS - now) / DAY_MS));
}
