// In-browser mock backend used by `pnpm dev:web` (UI development & QA).
// Implements the RPC surface with simplified in-memory semantics.

import { parseMeta } from '$lib/markdown/meta';
import type { Transport } from './rpc';
import { RpcError } from './rpc';
import type { BoardDelta, BoardEntry, BoardKind, BoardSnapshot, CoreEvent, JournalEntry, LaneDto, NodeDto, Op, Parent, Registry, SearchHit } from './types';

interface MockNode {
  id: string;
  parent: Parent;
  children: string[];
  content: string;
  archived: boolean;
  cover: NodeDto['cover'];
  mtime: number;
}

interface MockBoard {
  id: string;
  name: string;
  kind: BoardKind;
  root: string;
  view: BoardSnapshot['header']['view'];
  tagColors: Record<string, string>;
  lanes: LaneDto[];
  rootOrder: string[];
  nodes: Map<string, MockNode>;
  version: number;
  undo: string[];
  redo: string[];
  journal: JournalEntry[];
}

const rid = (p: string) => p + Math.random().toString(36).slice(2, 8).padEnd(6, '0');

function dto(b: MockBoard, n: MockNode): NodeDto {
  const m = parseMeta(n.content);
  return {
    id: n.id,
    parent: n.parent,
    isGroup: n.children.length > 0,
    children: [...n.children],
    archived: n.archived,
    cover: n.cover,
    title: m.title,
    hasTitleLine: m.hasTitleLine,
    tags: m.tags,
    links: m.links,
    mentions: m.mentions,
    dates: m.dates,
    footer: m.footer,
    tasks: m.tasks,
    face: m.face,
    headings: m.headings,
    attachments: [],
    mtime: n.mtime,
    wordCount: m.wordCount,
    hasCode: m.hasCode,
    remote: null,
  };
  void b;
}

function snapshot(b: MockBoard): BoardSnapshot {
  return {
    header: {
      id: b.id,
      name: b.name,
      kind: b.kind,
      root: b.root,
      view: { ...b.view },
      tagColors: { ...b.tagColors },
      readOnly: null,
      warnings: [],
      schema: 1,
    },
    lanes: b.lanes.map((l) => ({ ...l, order: [...l.order] })),
    rootOrder: [...b.rootOrder],
    nodes: [...b.nodes.values()].map((n) => dto(b, n)),
    version: b.version,
  };
}

function serialize(b: MockBoard): string {
  return JSON.stringify({
    lanes: b.lanes,
    rootOrder: b.rootOrder,
    nodes: [...b.nodes.values()],
    name: b.name,
    kind: b.kind,
    view: b.view,
    tagColors: b.tagColors,
  });
}

function restore(b: MockBoard, s: string) {
  const o = JSON.parse(s);
  b.lanes = o.lanes;
  b.rootOrder = o.rootOrder;
  b.nodes = new Map(o.nodes.map((n: MockNode) => [n.id, n]));
  b.name = o.name;
  b.kind = o.kind;
  b.view = o.view;
  b.tagColors = o.tagColors;
}

function childrenOf(b: MockBoard, p: Parent): string[] {
  if (p.kind === 'lane') {
    const l = b.lanes.find((x) => x.id === p.id);
    if (!l) throw new RpcError('not_found', p.id);
    return l.order;
  }
  if (p.kind === 'card') {
    const n = b.nodes.get(p.id);
    if (!n) throw new RpcError('not_found', p.id);
    return n.children;
  }
  return b.rootOrder;
}

function isAncestor(b: MockBoard, anc: string, id: string): boolean {
  let cur: string | null = id;
  while (cur) {
    if (cur === anc) return true;
    const n = b.nodes.get(cur);
    cur = n && n.parent.kind === 'card' ? n.parent.id : null;
  }
  return false;
}

function descendants(b: MockBoard, id: string): string[] {
  const out: string[] = [];
  const stack = [...(b.nodes.get(id)?.children ?? [])];
  while (stack.length) {
    const c = stack.pop()!;
    out.push(c);
    stack.push(...(b.nodes.get(c)?.children ?? []));
  }
  return out;
}

function applyOp(b: MockBoard, op: Op) {
  switch (op.op) {
    case 'createCard': {
      const list = childrenOf(b, op.parent);
      b.nodes.set(op.id, { id: op.id, parent: op.parent, children: [], content: op.content, archived: false, cover: null, mtime: Date.now() });
      list.splice(op.index ?? list.length, 0, op.id);
      break;
    }
    case 'writeCard': {
      const n = b.nodes.get(op.id);
      if (!n) throw new RpcError('not_found', op.id);
      n.content = op.content.endsWith('\n') ? op.content : op.content + '\n';
      n.mtime = Date.now();
      break;
    }
    case 'move': {
      const ids = op.ids.filter((id) => b.nodes.has(id) && !op.ids.some((o) => o !== id && isAncestor(b, o, id)));
      if (op.to.kind === 'card' && ids.some((id) => isAncestor(b, id, (op.to as { id: string }).id))) throw new RpcError('invalid', 'cycle');
      for (const id of ids) {
        const n = b.nodes.get(id)!;
        const from = childrenOf(b, n.parent);
        from.splice(from.indexOf(id), 1);
      }
      const target = childrenOf(b, op.to);
      let at = op.before ? target.indexOf(op.before) : target.length;
      if (at < 0) at = target.length;
      target.splice(at, 0, ...ids);
      for (const id of ids) b.nodes.get(id)!.parent = op.to;
      break;
    }
    case 'place': {
      for (const it of op.items) {
        const n = b.nodes.get(it.id);
        if (!n) continue;
        const from = childrenOf(b, n.parent);
        from.splice(from.indexOf(it.id), 1);
      }
      for (const it of [...op.items].sort((a, c) => a.index - c.index)) {
        const n = b.nodes.get(it.id);
        if (!n) continue;
        const list = childrenOf(b, it.parent);
        list.splice(Math.min(it.index, list.length), 0, it.id);
        n.parent = it.parent;
      }
      break;
    }
    case 'trash': {
      for (const id of op.nodes) {
        const n = b.nodes.get(id);
        if (!n) continue;
        const from = childrenOf(b, n.parent);
        from.splice(from.indexOf(id), 1);
        for (const d of descendants(b, id)) b.nodes.delete(d);
        b.nodes.delete(id);
      }
      for (const k of op.lanes) {
        const l = b.lanes.find((x) => x.id === k);
        if (!l) continue;
        for (const c of l.order) {
          for (const d of descendants(b, c)) b.nodes.delete(d);
          b.nodes.delete(c);
        }
        b.lanes = b.lanes.filter((x) => x.id !== k);
      }
      break;
    }
    case 'setArchived':
      for (const [id, a] of op.nodes) {
        const n = b.nodes.get(id);
        if (n) n.archived = a;
      }
      for (const [id, a] of op.lanes) {
        const l = b.lanes.find((x) => x.id === id);
        if (l) l.archived = a;
      }
      break;
    case 'createLane':
      b.lanes.splice(op.index ?? b.lanes.length, 0, {
        id: op.id,
        name: op.name,
        order: [],
        color: null,
        width: null,
        wip: null,
        collapsed: false,
        archived: false,
      });
      break;
    case 'updateLane': {
      const l = b.lanes.find((x) => x.id === op.id);
      if (!l) throw new RpcError('not_found', op.id);
      if (op.patch.name !== undefined) l.name = op.patch.name;
      if (op.patch.color !== undefined) l.color = op.patch.color || null;
      if (op.patch.width !== undefined) l.width = op.patch.width || null;
      if (op.patch.wip !== undefined) l.wip = op.patch.wip || null;
      if (op.patch.collapsed !== undefined) l.collapsed = op.patch.collapsed;
      break;
    }
    case 'moveLane': {
      const i = b.lanes.findIndex((x) => x.id === op.id);
      const [l] = b.lanes.splice(i, 1);
      b.lanes.splice(Math.min(op.index, b.lanes.length), 0, l);
      break;
    }
    case 'updateBoard':
      if (op.patch.name) b.name = op.patch.name;
      if (op.patch.view) b.view = op.patch.view;
      if (op.patch.tagColors) b.tagColors = op.patch.tagColors;
      break;
    case 'setCover': {
      const n = b.nodes.get(op.id);
      if (n) n.cover = op.cover;
      break;
    }
    case 'setKind':
      b.kind = op.kind;
      break;
    case 'batch':
      for (const o of op.ops) applyOp(b, o);
      break;
    case 'restore':
      break;
  }
}

function seed(): MockBoard[] {
  const now = Date.now();
  const k = ['kbacklg', 'kprogrs', 'kreview', 'kdone00'];
  const cards: [string, number, string, string | null][] = [
    [
      'cwelcom',
      0,
      '# Welcome to Luau\n\nEvery card is a plain Markdown file on disk. Drag cards around, drop one *onto* another to group them, and press ⌘⇧P for commands.\n\n#guide\n',
      null,
    ],
    ['conbrd1', 0, '# Onboarding flow\n\nA calm first-run experience. #design #q4\n', null],
    ['conbsub', 0, '# Welcome screen copy\n\n- [x] Draft tone of voice\n- [ ] Review with @ana\n- [ ] Translate to es / pt\n', 'conbrd1'],
    ['conbsu2', 0, '# Pick keybinding preset\n\nLet people choose VS Code, Vim or Trello style on first launch.\n', 'conbrd1'],
    [
      'csearch',
      1,
      '# Fast search everywhere\n\nSubstring + case-sensitive search across all boards with a small query language.\n\n| Filter | Example |\n|---|---|\n| tag | tag:backend |\n| due | due:<2026-10-10 |\n| board | board:"Roadmap" |\n\n#backend #search\n',
      null,
    ],
    [
      'cdragdr',
      1,
      '# Buttery drag and drop\n\n- [x] Pointer-based engine\n- [x] Nest by dropping onto a card\n- [ ] Lasso selection with ⌘-drag\n- [ ] Auto-scroll edges\n\n---\npriority: high\ndue: 2026-10-08\nassignees: @eros\n',
      null,
    ],
    [
      'chistry',
      2,
      '# History & daily summaries\n\nSee what changed yesterday, restore any version, and get a Monday digest in Slack. #history\n\n---\npriority: medium\nlabels: ai\n',
      null,
    ],
    ['cjiraxx', 2, '# Jira mirror boards\n\nMirror a Jira board locally; copy issues into your own boards with live status. #integrations\n', null],
    ['cshippd', 3, '# Markdown cards on disk\n\nCards are `c123abc.md` files; groups are folders with an `index.md`. See [[cwelcom]].\n\n#core\n', null],
    ['cthemes', 3, '# Pastel themes\n\nIndigo + pale green, with Liquid Glass translucency on macOS. #design\n', null],
  ];
  const lanes: LaneDto[] = [
    { id: k[0], name: 'Backlog', order: [], color: null, width: null, wip: null, collapsed: false, archived: false },
    { id: k[1], name: 'In progress', order: [], color: '#c7c8fa', width: null, wip: 3, collapsed: false, archived: false },
    { id: k[2], name: 'Review', order: [], color: null, width: null, wip: null, collapsed: false, archived: false },
    { id: k[3], name: '✅ Done', order: [], color: '#cfe8d3', width: null, wip: null, collapsed: false, archived: false },
  ];
  const nodes = new Map<string, MockNode>();
  for (const [id, lane, content, parent] of cards) {
    const p: Parent = parent ? { kind: 'card', id: parent } : { kind: 'lane', id: k[lane] };
    nodes.set(id, { id, parent: p, children: [], content, archived: false, cover: null, mtime: now - Math.random() * 1e8 });
    if (parent) nodes.get(parent)!.children.push(id);
    else lanes[lane].order.push(id);
  }
  const roadmap: MockBoard = {
    id: 'broadmp',
    name: 'Product Roadmap',
    kind: 'kanban',
    root: '~/Documents/Roadmap',
    view: { orientation: 'columns', spacing: 'fixedMain' },
    tagColors: {},
    lanes,
    rootOrder: [],
    nodes,
    version: 1,
    undo: [],
    redo: [],
    journal: [],
  };
  const notesNodes = new Map<string, MockNode>();
  const docs: [string, string, string | null][] = [
    ['cnote01', '# Reading list\n\n- [ ] *The Timeless Way of Building*\n- [x] *Calm Technology*\n', null],
    ['cnote02', '# Recipes\n\nFavourite things to cook on weekends.\n', null],
    ['cnote03', '# Pasta al limone\n\nZest, butter, parmesan. Done in 12 minutes.\n', 'cnote02'],
  ];
  const rootOrder: string[] = [];
  for (const [id, content, parent] of docs) {
    notesNodes.set(id, {
      id,
      parent: parent ? { kind: 'card', id: parent } : { kind: 'root' },
      children: [],
      content,
      archived: false,
      cover: null,
      mtime: now,
    });
    if (parent) notesNodes.get(parent)!.children.push(id);
    else rootOrder.push(id);
  }
  const notes: MockBoard = {
    id: 'bnotes0',
    name: 'Notes',
    kind: 'files',
    root: '~/Notes',
    view: { orientation: 'columns', spacing: 'fixedMain' },
    tagColors: {},
    lanes: [],
    rootOrder,
    nodes: notesNodes,
    version: 1,
    undo: [],
    redo: [],
    journal: [],
  };
  return [roadmap, notes];
}

export interface MockApi {
  boards: Map<string, unknown>;
  emit: (e: CoreEvent) => void;
  ls: (k: string, d: unknown) => any;
  lsSet: (k: string, v: unknown) => void;
  snapshot: (id: string) => BoardSnapshot;
}

export function createMockTransport(): Transport {
  const boards = new Map<string, MockBoard>(seed().map((b) => [b.id, b]));
  const listeners = new Set<(e: CoreEvent) => void>();
  const menuListeners = new Set<(c: string) => void>();
  const emit = (e: CoreEvent) => queueMicrotask(() => listeners.forEach((l) => l(e)));
  const ls = (k: string, d: unknown) => {
    try {
      const v = localStorage.getItem('luau.mock.' + k);
      return v ? JSON.parse(v) : d;
    } catch {
      return d;
    }
  };
  const lsSet = (k: string, v: unknown) => {
    try {
      localStorage.setItem('luau.mock.' + k, JSON.stringify(v));
    } catch {
      /* ignore */
    }
  };
  // Registry overrides (pin/hide/section/order) persisted like the real core's registry.
  type RegOverlay = {
    entries: Record<string, { pinned?: boolean; hidden?: boolean; section?: string | null; removed?: boolean }>;
    order: string[];
    mirrorOrder: string[];
  };
  const regOverlay = (): RegOverlay => ls('registry', { entries: {}, order: [], mirrorOrder: [] }) as RegOverlay;
  const registry = (): Registry => {
    const o = regOverlay();
    return {
      boards: [...boards.values()]
        .filter((b) => !o.entries[b.id]?.removed)
        .map((b): BoardEntry => {
          const e = o.entries[b.id] ?? {};
          return {
            id: b.id,
            path: b.root,
            name: b.name,
            kind: b.kind,
            pinned: e.pinned ?? b.id === 'broadmp',
            hidden: e.hidden ?? false,
            ...(e.section ? { section: e.section } : {}),
            missing: false,
            mirror: false,
            lastSeen: Date.now(),
          };
        }),
      order: o.order,
      mirrorOrder: o.mirrorOrder,
    };
  };
  const updateRegistry = (p: Record<string, any>) => {
    const o = regOverlay();
    if (typeof p.id === 'string') {
      const e = (o.entries[p.id] ??= {});
      if (p.remove) e.removed = true;
      if (typeof p.pinned === 'boolean') e.pinned = p.pinned;
      if (typeof p.hidden === 'boolean') e.hidden = p.hidden;
      if (typeof p.section === 'string') e.section = p.section || null;
    }
    if (Array.isArray(p.order)) o.order = p.order;
    if (Array.isArray(p.mirrorOrder)) o.mirrorOrder = p.mirrorOrder;
    lsSet('registry', o);
    const r = registry();
    emit({ type: 'registryChanged', registry: r });
    return r;
  };
  const board = (id: string) => {
    const b = boards.get(id);
    if (!b) throw new RpcError('not_found', `board ${id}`);
    return b;
  };
  const fullDelta = (b: MockBoard): BoardDelta => {
    const s = snapshot(b);
    return { boardId: b.id, version: b.version, header: s.header, lanes: s.lanes, rootOrder: s.rootOrder, nodes: s.nodes, removed: [] };
  };
  const undoState = (b: MockBoard) =>
    emit({ type: 'undoState', boardId: b.id, canUndo: b.undo.length > 0, canRedo: b.redo.length > 0, undoLabel: null, redoLabel: null });
  const coalesceKeys = new Map<string, string>();

  const methods: Record<string, (p: Record<string, any>) => unknown> = {
    'app.info': () => ({
      name: 'Luau',
      version: '0.1.0-web',
      platform: 'web',
      arch: 'wasm',
      dataDir: '',
      configDir: '',
      logsDir: '',
      window: 'main',
      home: '~',
    }),
    'settings.get': () => ls('settings', {}),
    'settings.set': (p) => (lsSet('settings', p.value), true),
    'keybindings.get': () => ls('keybindings', []),
    'keybindings.set': (p) => (lsSet('keybindings', p.value), true),
    'uiState.get': () => ls('uiState', null),
    'uiState.set': (p) => (lsSet('uiState', p.value), true),
    'fonts.list': () => ['Inter', 'Georgia', 'Helvetica Neue', 'Menlo', 'SF Pro Text', 'Times New Roman'],
    'registry.get': () => registry(),
    'registry.update': (p) => updateRegistry(p),
    'discovery.rescan': () => true,
    'board.open': (p) => {
      const b = p.id ? board(p.id) : [...boards.values()].find((x) => x.root === p.path);
      if (!b) throw new RpcError('not_a_board', String(p.path));
      return snapshot(b);
    },
    'board.create': (p) => {
      const id = rid('b');
      const b: MockBoard = {
        id,
        name: p.name || 'New board',
        kind: p.kind ?? 'kanban',
        root: p.path,
        view: { orientation: 'columns', spacing: 'fixedMain' },
        tagColors: {},
        lanes: [],
        rootOrder: [],
        nodes: new Map(),
        version: 1,
        undo: [],
        redo: [],
        journal: [],
      };
      for (const name of p.lanes ?? []) b.lanes.push({ id: rid('k'), name, order: [], color: null, width: null, wip: null, collapsed: false, archived: false });
      boards.set(id, b);
      emit({ type: 'registryChanged', registry: registry() });
      return snapshot(b);
    },
    'board.close': () => true,
    'board.snapshot': (p) => snapshot(board(p.id)),
    'board.claim': () => null,
    'board.release': () => true,
    'board.newCardId': () => rid('c'),
    'board.newLaneId': () => rid('k'),
    'board.apply': (p) => {
      const b = board(p.board);
      const before = serialize(b);
      const key: string | undefined = p.coalesce ?? undefined;
      applyOp(b, p.op as Op);
      if (!(key && coalesceKeys.get(b.id) === key)) b.undo.push(before);
      coalesceKeys.set(b.id, key ?? '');
      b.redo = [];
      b.version++;
      b.journal.unshift({ ts: new Date().toISOString(), board: b.id, kind: (p.op as Op).op, origin: 'you', label: p.label ?? (p.op as Op).op, ids: [] });
      emit({ type: 'boardDelta', delta: fullDelta(b) });
      undoState(b);
      return { version: b.version, created: (p.op as Op).op === 'createCard' ? [(p.op as { id: string }).id] : [], trashed: [] };
    },
    'board.undo': (p) => {
      const b = board(p.board);
      const s = b.undo.pop();
      if (!s) return { label: null, done: false };
      b.redo.push(serialize(b));
      restore(b, s);
      b.version++;
      coalesceKeys.delete(b.id);
      emit({ type: 'boardDelta', delta: fullDelta(b) });
      undoState(b);
      return { label: 'Undo', done: true };
    },
    'board.redo': (p) => {
      const b = board(p.board);
      const s = b.redo.pop();
      if (!s) return { label: null, done: false };
      b.undo.push(serialize(b));
      restore(b, s);
      b.version++;
      emit({ type: 'boardDelta', delta: fullDelta(b) });
      undoState(b);
      return { label: 'Redo', done: true };
    },
    'board.seal': (p) => (coalesceKeys.delete(p.board), true),
    'board.moveAcross': (p) => {
      const from = board(p.from);
      const to = board(p.to);
      const moved: MockNode[] = [];
      for (const id of p.ids as string[]) {
        const n = from.nodes.get(id);
        if (!n) continue;
        for (const d of [id, ...descendants(from, id)]) {
          moved.push(from.nodes.get(d)!);
          from.nodes.delete(d);
        }
        const list = childrenOf(from, n.parent);
        list.splice(list.indexOf(id), 1);
        n.parent = p.parent;
      }
      for (const n of moved) to.nodes.set(n.id, n);
      const list = childrenOf(to, p.parent);
      const at = p.before ? list.indexOf(p.before) : list.length;
      list.splice(at < 0 ? list.length : at, 0, ...(p.ids as string[]));
      from.version++;
      to.version++;
      emit({ type: 'boardDelta', delta: fullDelta(from) });
      emit({ type: 'boardDelta', delta: fullDelta(to) });
      return true;
    },
    'card.read': (p) => board(p.board).nodes.get(p.id)?.content ?? '',
    'card.write': (p) =>
      methods['board.apply']({
        board: p.board,
        op: { op: 'writeCard', id: p.id, content: p.content },
        label: 'Edit',
        coalesce: p.session ? `edit:${p.id}:${p.session}` : null,
      }),
    'attachment.add': (p) => ({ file: `${p.card}.mock-${p.name}`, display: p.name, kind: 'image', size: 0 }),
    'attachment.cleanup': () => 0,
    'trash.list': () => [],
    'trash.purge': () => 0,
    'history.query': (p) => board(p.board).journal.slice(0, p.filter?.limit ?? 200),
    'history.blob': () => '',
    'search.query': (p) => {
      const q = String(p.q ?? '')
        .replace(/\b(in|tag|is|board|lane):\S+/g, '')
        .trim()
        .toLowerCase();
      const hits: SearchHit[] = [];
      for (const b of boards.values())
        for (const n of b.nodes.values()) {
          const text = n.content.toLowerCase();
          if (!q || text.includes(q)) {
            const m = parseMeta(n.content);
            const i = text.indexOf(q);
            hits.push({
              board: b.id,
              boardName: b.name,
              id: n.id,
              title: m.title,
              snippet: n.content.slice(Math.max(0, i - 30), i + 90).replace(/\n/g, ' '),
              laneName: b.lanes.find((l) => l.order.includes(n.id))?.name ?? null,
              tags: m.tags,
              kind: b.kind === 'files' ? 'doc' : 'card',
              isGroup: n.children.length > 0,
              archived: n.archived,
              mtime: n.mtime,
              remoteKey: null,
              status: null,
            });
          }
        }
      return hits.slice(0, 100);
    },
    'search.tags': () => {
      const counts = new Map<string, number>();
      for (const b of boards.values())
        for (const n of b.nodes.values()) for (const t of parseMeta(n.content).tags) counts.set(t.toLowerCase(), (counts.get(t.toLowerCase()) ?? 0) + 1);
      return [...counts.entries()].sort((a, c) => c[1] - a[1]);
    },
    'search.people': () => ['ana', 'eros', 'luis'],
    'search.titles': (p) =>
      (p.ids as string[]).flatMap((id) => {
        for (const b of boards.values()) {
          const n = b.nodes.get(id);
          if (n) return [{ id, board: b.id, title: parseMeta(n.content).title }];
        }
        return [];
      }),
    'search.backlinks': () => [],
    'search.locate': (p) => {
      for (const b of boards.values()) if (b.nodes.has(p.id)) return { board: b.id, title: parseMeta(b.nodes.get(p.id)!.content).title };
      return null;
    },
    'path.exists': () => ({ exists: true, isDir: true, isBoard: false }),
  };

  // Feature mocks: `mocks/*.ts` export `register(methods, api)`.
  const featureMocks = import.meta.glob<{ register: (m: typeof methods, api: MockApi) => void }>('./mocks/*.ts', { eager: true });
  const api: MockApi = { boards, emit, ls, lsSet, snapshot: (id: string) => snapshot(board(id)) };
  for (const m of Object.values(featureMocks)) m.register(methods, api);

  return {
    async call<T>(method: string, params: Record<string, unknown> = {}) {
      const fn = methods[method];
      if (!fn) {
        console.warn('[mock] unimplemented', method, params);
        throw new RpcError('invalid', `mock: ${method} not implemented`);
      }
      await new Promise((r) => setTimeout(r, 0));
      return structuredClone(fn(params as Record<string, any>)) as T;
    },
    onEvent(fn) {
      listeners.add(fn);
      return () => listeners.delete(fn);
    },
    onMenu(fn) {
      menuListeners.add(fn);
      return () => menuListeners.delete(fn);
    },
  };
}
