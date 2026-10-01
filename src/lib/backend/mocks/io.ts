// Browser mock for import / export, templates, schema upgrade and settings bundles.
import type { MockApi } from '../mock';

interface MockNode {
  content: string;
  children: string[];
}
interface MockBoardLike {
  id: string;
  name: string;
  kind: 'kanban' | 'files';
  lanes: { id: string; name: string; order: string[] }[];
  rootOrder: string[];
  nodes: Map<string, MockNode>;
}

const esc = (s: string) => s.replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`);
const rid = (p: string) => p + Math.random().toString(36).slice(2, 8).padEnd(6, '0');

function download(name: string, text: string, type: string) {
  if (typeof document === 'undefined') return;
  const url = URL.createObjectURL(new Blob([text], { type }));
  const a = Object.assign(document.createElement('a'), { href: url, download: name.split(/[\\/]/).pop() ?? name });
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

function markdown(b: MockBoardLike, only?: string): string {
  const out: string[] = only ? [] : [`# ${b.name}`];
  const card = (id: string) => {
    const n = b.nodes.get(id);
    if (!n) return;
    out.push(n.content.trim());
    n.children.forEach(card);
  };
  if (only) card(only);
  else {
    for (const l of b.lanes) {
      out.push(`## ${l.name}`);
      l.order.forEach(card);
    }
    b.rootOrder.forEach(card);
  }
  return out.join('\n\n') + '\n';
}

function html(b: MockBoardLike, only?: string): string {
  const body = markdown(b, only)
    .split('\n')
    .map((l) => (l.startsWith('#') ? `<h3>${esc(l.replace(/^#+\s*/, ''))}</h3>` : l ? `<p>${esc(l)}</p>` : ''))
    .join('\n');
  return `<!doctype html><html><head><meta charset="utf-8"><title>${esc(b.name)}</title><style>body{font:15px/1.6 system-ui;max-width:860px;margin:40px auto;padding:0 16px;color:#2b2d33;background:#fbfaf7}</style></head><body>${body}</body></html>`;
}

export function register(methods: Record<string, (p: Record<string, any>) => unknown>, api: MockApi) {
  const get = (id: string) => {
    const b = api.boards.get(id) as MockBoardLike | undefined;
    if (!b) throw new Error(`board ${id} not open`);
    return b;
  };

  methods['io.renderHtml'] = (p) => html(get(p.board), p.card ?? undefined);
  methods['io.export'] = (p) => {
    const b = get(p.board);
    const text =
      p.format === 'html'
        ? html(b, p.card ?? undefined)
        : p.format === 'json'
          ? JSON.stringify({ format: 'luau-interchange', version: 1, board: { name: b.name, kind: b.kind } }, null, 2)
          : markdown(b, p.card ?? undefined);
    download(String(p.dest), text, p.format === 'html' ? 'text/html' : 'text/plain');
    return { path: p.dest, files: 1, bytes: text.length };
  };
  methods['io.inspect'] = (p) => ({
    kind: 'folder',
    name: String(p.path).split(/[\\/]/).filter(Boolean).pop() ?? 'Imported',
    boardId: null,
    registered: false,
    schema: null,
    boardKind: 'kanban',
    lanes: 1,
    notes: 1,
    truncated: false,
  });
  methods['io.import'] = (p) => {
    const name = p.name || String(p.path).split(/[\\/]/).filter(Boolean).pop() || 'Imported';
    if (p.mode === 'overwrite') return { boardId: p.target, cards: 0, lanes: 0, attachments: 0, warnings: [] };
    const snap = methods['board.create']({ path: p.dest ?? p.path, name, kind: 'kanban', lanes: [p.inboxLane ?? 'Inbox'] }) as { header: { id: string } };
    return { boardId: snap.header.id, cards: 0, lanes: 1, attachments: 0, warnings: [] };
  };
  methods['templates.list'] = () => [];
  methods['board.createFromTemplate'] = (p) => {
    const tpl = p.template as {
      kind: 'kanban' | 'files';
      lanes: { name: string; cards: string[] }[];
      notes: string[];
      assets?: { card: number; name: string; data: string }[];
    };
    const snap = methods['board.create']({ path: p.path, name: p.name, kind: tpl.kind, lanes: [] }) as { header: { id: string } };
    const ids = [...tpl.lanes.flatMap((l) => l.cards), ...tpl.notes].map(() => rid('c'));
    // No files in the mock: assets become data URLs.
    const mime = (n: string) =>
      n.endsWith('.svg') ? 'image/svg+xml' : n.endsWith('.png') ? 'image/png' : n.endsWith('.pdf') ? 'application/pdf' : 'application/octet-stream';
    let n = 0;
    const fill = (text: string) => {
      const i = n++;
      return text
        .replace(/\{\{card:(\d+)\}\}/g, (m, k: string) => ids[Number(k)] ?? m)
        .replace(/\{\{asset:([^}]+)\}\}/g, (m, name: string) => {
          const a = tpl.assets?.find((x) => x.card === i && x.name === name.trim());
          return a ? `data:${mime(a.name)};base64,${a.data}` : m;
        });
    };
    const ops: unknown[] = [];
    for (const l of tpl.lanes) {
      const k = rid('k');
      ops.push({ op: 'createLane', id: k, name: l.name, index: null });
      for (const c of l.cards) ops.push({ op: 'createCard', id: ids[n], parent: { kind: 'lane', id: k }, index: null, content: fill(c) });
    }
    for (const c of tpl.notes) ops.push({ op: 'createCard', id: ids[n], parent: { kind: 'root' }, index: null, content: fill(c) });
    if (ops.length) methods['board.apply']({ board: snap.header.id, op: { op: 'batch', ops }, label: 'New board from template' });
    return api.snapshot(snap.header.id);
  };
  methods['board.upgrade'] = (p) => ({ report: { from: 2, to: 1, files: 1 }, snapshot: api.snapshot(p.board) });
  methods['settings.export'] = (p) => {
    download(String(p.path), JSON.stringify(p.bundle, null, 2), 'application/json');
    return p.bundle;
  };
  methods['settings.import'] = () => ({
    format: 'luau-settings',
    version: 1,
    settings: api.ls('settings', {}),
    keybindings: api.ls('keybindings', []),
    templates: [],
  });
}
