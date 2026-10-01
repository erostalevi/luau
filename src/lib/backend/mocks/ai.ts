// Browser mock for local AI, activity summaries, schedules, code cells and link previews.
// Summaries are built from the current board snapshot (lane-name stage inference),
// not from a real journal; good enough for UI work in `pnpm dev:web`.

import type { MockApi } from '../mock';
import type { BoardSnapshot } from '../types';
import { RpcError } from '../rpc';

type Methods = Record<string, (p: Record<string, any>) => unknown>;

const DONE = /\b(done|complete[d]?|finished|closed|resolved|shipped|released|hecho|terminado|listo|completado|cerrado|feito|concluido)\b/i;
const DOING = /\b(doing|in progress|wip|working|review|testing|qa|en curso|en progreso|haciendo|fazendo|em andamento)\b/i;

const rid = (p: string) => p + Math.random().toString(36).slice(2, 10);

export function register(methods: Methods, api: MockApi) {
  const load = <T>(k: string, d: T): T => api.ls(k, d) as T;
  const save = (k: string, v: unknown) => api.lsSet(k, v);

  function facts(p: Record<string, any>) {
    const ids: string[] = p.boards?.length ? p.boards : [...api.boards.keys()];
    const boards = ids
      .filter((id) => api.boards.has(id))
      .map((id) => {
        const s: BoardSnapshot = api.snapshot(id);
        const title = (nid: string) => s.nodes.find((n) => n.id === nid)?.title ?? nid;
        const completed: { id: string; title: string; lane: string }[] = [];
        const started: typeof completed = [];
        const pending: typeof completed = [];
        for (const l of s.lanes.filter((x) => !x.archived)) {
          const list = DONE.test(l.name) ? completed : DOING.test(l.name) ? started : pending;
          for (const c of l.order) list.push({ id: c, title: title(c), lane: l.name });
        }
        return { id, name: s.header.name, created: [], completed, started, moved: [], edited: [], deleted: [], archived: [], external: [], pending, overdue: [] };
      });
    const sum = (k: 'completed' | 'started' | 'pending') => boards.reduce((a, b) => a + b[k].length, 0);
    return {
      period: { from: p.from, to: p.to },
      boards,
      totals: { boards: boards.length, created: 0, completed: sum('completed'), started: sum('started'), moved: 0, edited: 0, deleted: 0, archived: 0, external: 0, pending: sum('pending'), overdue: 0, events: 1 },
    };
  }

  function render(f: ReturnType<typeof facts>, detail: number, links: boolean) {
    const lim = [3, 5, 25, 60, 999][Math.min(4, Math.max(0, detail - 1))];
    const name = (i: { id: string; title: string }) => (links ? `[[${i.id}]]` : i.title);
    const out = ['# Activity summary', `_${new Date(f.period.from).toDateString()} – ${new Date(f.period.to).toDateString()}_`];
    for (const b of f.boards) {
      out.push('', `## ${b.name}`);
      for (const [label, list] of [['Completed', b.completed], ['Started', b.started]] as const) {
        if (!list.length) continue;
        out.push('', `### ${label} (${list.length})`, ...list.slice(0, lim).map((i) => `- ${name(i)}`));
        if (list.length > lim) out.push(`- …and ${list.length - lim} more`);
      }
    }
    if (!f.boards.some((b) => b.completed.length || b.started.length)) out.push('', 'Nothing happened in this period.');
    return out.join('\n');
  }

  methods['ai.status'] = () => ({ provider: 'none', available: false, endpoint: 'http://localhost:11434', model: null, models: [], remote: false, error: 'mock: no local AI in the browser' });
  methods['ai.models'] = () => [];
  methods['ai.pullModel'] = () => {
    throw new Error('mock: downloading models needs Ollama');
  };
  methods['ai.slackConnected'] = () => false;
  methods['activity.facts'] = (p) => facts(p);
  methods['activity.summarize'] = (p) => {
    const f = facts(p);
    const markdown = render(f, p.detail ?? 3, p.links !== false);
    const created = new Date().toISOString();
    let id: string | null = null;
    if (p.save !== false) {
      id = rid('s');
      const list = load<any[]>('ai.summaries', []);
      list.unshift({ id, created, title: p.title || `Activity summary · ${new Date(p.from).toLocaleDateString()}`, from: p.from, to: p.to, boards: p.boards ?? [], detail: p.detail ?? 3, prompt: p.prompt ?? '', engine: 'basic', model: null, scheduleId: p.scheduleId ?? null, markdown, delivery: [] });
      save('ai.summaries', list.slice(0, 200));
    }
    if (p.requestId) for (const stage of ['facts', 'done']) api.emit({ type: 'custom', name: 'summary.progress', payload: { requestId: p.requestId, stage } });
    return { id, title: 'Activity summary', markdown, engine: 'basic', model: null, fallbackReason: p.engine === 'basic' ? null : 'local AI not available', created, facts: f, delivery: [] };
  };
  methods['card.summarize'] = (p) => {
    const s: BoardSnapshot = api.snapshot(p.board);
    const n = s.nodes.find((x) => x.id === p.id);
    return { text: n?.title ?? '', engine: 'basic', model: null, cached: false };
  };
  methods['summaries.list'] = (p) =>
    load<any[]>('ai.summaries', [])
      .slice(0, p.limit ?? 50)
      .map(({ markdown, boards: _b, detail: _d, prompt: _p, ...m }) => ({ ...m, excerpt: String(markdown).slice(0, 160) }));
  methods['summaries.get'] = (p) => {
    const s = load<any[]>('ai.summaries', []).find((x) => x.id === p.id);
    if (!s) throw new Error('not found');
    return s;
  };
  methods['summaries.delete'] = (p) => save('ai.summaries', load<any[]>('ai.summaries', []).filter((x) => x.id !== p.id));
  methods['summaries.export'] = () => null;

  methods['schedules.list'] = () => load<any[]>('ai.schedules', []).map((s) => ({ ...s, nextRun: s.enabled ? new Date(Date.now() + 86_400_000).toISOString() : null }));
  methods['schedules.save'] = (p) => {
    const s = { ...p.schedule };
    if (!String(s.name ?? '').trim()) throw new Error('schedule needs a name');
    const list = load<any[]>('ai.schedules', []);
    if (!s.id) {
      s.id = rid('h');
      s.created = new Date().toISOString();
      list.push(s);
    } else {
      const i = list.findIndex((x) => x.id === s.id);
      if (i >= 0) list[i] = { ...list[i], ...s };
      else list.push(s);
    }
    save('ai.schedules', list);
    return s;
  };
  methods['schedules.delete'] = (p) => save('ai.schedules', load<any[]>('ai.schedules', []).filter((x) => x.id !== p.id));
  methods['schedules.runNow'] = (p) => {
    const s = load<any[]>('ai.schedules', []).find((x) => x.id === p.id);
    if (!s) throw new Error('not found');
    const to = new Date();
    const from = new Date(to.getTime() - 86_400_000);
    const r = methods['activity.summarize']({ from: from.toISOString(), to: to.toISOString(), boards: s.boards, detail: s.detail, engine: 'basic', title: s.name, scheduleId: s.id, links: false }) as Record<string, unknown>;
    save('ai.schedules', load<any[]>('ai.schedules', []).map((x) => (x.id === s.id ? { ...x, lastRun: to.toISOString(), lastStatus: 'ok' } : x)));
    api.emit({ type: 'custom', name: 'schedules.ran', payload: { id: s.id, summaryId: r.id } });
    return { ...r, delivery: ['notification: unavailable'] };
  };

  methods['code.trusted'] = (p) => ({ trusted: load<string[]>('ai.trust', []).includes(p.board ?? '*') });
  methods['code.trust'] = (p) => {
    const set = new Set(load<string[]>('ai.trust', []));
    if (p.trusted === false) set.delete(p.board ?? '*');
    else set.add(p.board ?? '*');
    save('ai.trust', [...set]);
    return null;
  };
  methods['code.cached'] = () => null;
  methods['code.run'] = (p) => {
    if (!load<string[]>('ai.trust', []).includes(p.board ?? '*')) {
      throw new RpcError('conflict', 'needs_trust');
    }
    return { ok: true, stdout: `(mock) would run ${String(p.lang)} code (${String(p.code ?? '').length} chars)\n`, stderr: '', images: [], ms: 1, exitCode: 0, timedOut: false, truncated: false };
  };
  methods['web.preview'] = (p) => {
    let host: string;
    try {
      host = new URL(String(p.url)).hostname;
    } catch {
      return null;
    }
    return { url: p.url, title: host, description: 'Link preview (mock)', image: null, site: host, favicon: null, insecure: String(p.url).startsWith('http:'), fetched: new Date().toISOString() };
  };
}
