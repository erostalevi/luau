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
        return {
          id,
          name: s.header.name,
          created: [],
          completed,
          started,
          moved: [],
          edited: [],
          deleted: [],
          archived: [],
          external: [],
          pending,
          overdue: [],
        };
      });
    const sum = (k: 'completed' | 'started' | 'pending') => boards.reduce((a, b) => a + b[k].length, 0);
    return {
      period: { from: p.from, to: p.to },
      boards,
      totals: {
        boards: boards.length,
        created: 0,
        completed: sum('completed'),
        started: sum('started'),
        moved: 0,
        edited: 0,
        deleted: 0,
        archived: 0,
        external: 0,
        pending: sum('pending'),
        overdue: 0,
        events: 1,
      },
    };
  }

  function render(f: ReturnType<typeof facts>, detail: number, links: boolean) {
    const lim = [3, 5, 25, 60, 999][Math.min(4, Math.max(0, detail - 1))];
    const name = (i: { id: string; title: string }) => (links ? `[[${i.id}]]` : i.title);
    const out = ['# Activity summary', `_${new Date(f.period.from).toDateString()} – ${new Date(f.period.to).toDateString()}_`];
    for (const b of f.boards) {
      out.push('', `## ${b.name}`);
      for (const [label, list] of [
        ['Completed', b.completed],
        ['Started', b.started],
      ] as const) {
        if (!list.length) continue;
        out.push('', `### ${label} (${list.length})`, ...list.slice(0, lim).map((i) => `- ${name(i)}`));
        if (list.length > lim) out.push(`- …and ${list.length - lim} more`);
      }
    }
    if (!f.boards.some((b) => b.completed.length || b.started.length)) out.push('', 'Nothing happened in this period.');
    return out.join('\n');
  }

  // Remote services: keys and consent live in localStorage (the app uses the
  // OS keychain and a native consent prompt).
  const REMOTE = ['anthropic', 'chatgpt', 'gemini', 'openrouter'];
  const MODELS: Record<string, string[]> = {
    anthropic: ['claude-opus-5-5', 'claude-sonnet-5-5', 'claude-haiku-4-5'],
    chatgpt: ['gpt-5', 'gpt-5-mini'],
    gemini: ['gemini-2.5-pro', 'gemini-2.5-flash'],
    openrouter: ['anthropic/claude-opus-5-5', 'openai/gpt-5', 'google/gemini-2.5-pro'],
  };
  const keys = () => load<Record<string, boolean>>('ai.keys', {});
  const consents = () => load<Record<string, boolean>>('ai.consent', {});
  const cfg = () => load<Record<string, any>>('settings', {});
  const remoteCheck = (p: unknown) => {
    if (!REMOTE.includes(String(p))) throw new RpcError('invalid', 'not a remote AI service');
    return String(p);
  };
  function status() {
    const provider = String(cfg()['ai.provider'] ?? 'auto');
    if (REMOTE.includes(provider)) {
      const error = !consents()[provider] ? 'consent needed before card text is sent to this service' : !keys()[provider] ? 'no API key' : undefined;
      return {
        provider,
        available: !error,
        endpoint: `https://${provider}.example`,
        model: cfg()['ai.remoteModel'] || MODELS[provider][0],
        models: [],
        remote: true,
        error,
      };
    }
    if (provider === 'off') return { provider: 'off', available: false, endpoint: '', model: null, models: [], remote: false };
    return {
      provider: 'none',
      available: false,
      endpoint: String(cfg()['ai.endpoint'] || 'http://localhost:11434'),
      model: null,
      models: [],
      remote: false,
      error: 'mock: no local AI in the browser',
      apple: { status: 'missing', contextSize: 0 },
    };
  }
  methods['ai.status'] = () => status();
  methods['ai.remoteState'] = () => REMOTE.map((provider) => ({ provider, hasKey: !!keys()[provider], consent: !!consents()[provider] }));
  methods['ai.setKey'] = (p) => {
    const pr = remoteCheck(p.provider);
    const k = String(p.key ?? '').trim();
    if (k.length < 16 || /\s/.test(k)) throw new RpcError('invalid', "that doesn't look like an API key");
    save('ai.keys', { ...keys(), [pr]: true });
    return { persisted: true };
  };
  methods['ai.deleteKey'] = (p) => {
    const pr = remoteCheck(p.provider);
    const k = keys();
    delete k[pr];
    save('ai.keys', k);
  };
  methods['ai.consent'] = (p) => {
    const pr = remoteCheck(p.provider);
    if (p.granted !== false && !window.confirm(`${p.service ?? pr}\n\n${p.message ?? ''}`)) throw new RpcError('cancelled', '');
    save('ai.consent', { ...consents(), [pr]: p.granted !== false });
  };
  methods['ai.remoteModels'] = (p) => {
    const pr = remoteCheck(p.provider);
    if (!keys()[pr]) throw new RpcError('invalid', 'no API key');
    return MODELS[pr];
  };
  let setupCancelled = false;
  methods['ai.setupCancel'] = () => {
    setupCancelled = true;
  };
  methods['ai.setupLocal'] = async (p) => {
    setupCancelled = false;
    const ev = (payload: Record<string, unknown>) => api.emit({ type: 'custom', name: 'ai.setup', payload } as never);
    const total = 180 * 1024 ** 2;
    for (const step of ['check', 'start']) {
      ev({ step });
      await new Promise((r) => setTimeout(r, 300));
    }
    for (let done = 0; done <= total; done += total / 6) {
      if (setupCancelled) throw new RpcError('other', 'cancelled');
      ev({ step: 'download', completed: done, total });
      await new Promise((r) => setTimeout(r, 250));
    }
    for (const step of ['verify', 'install', 'connect']) {
      ev({ step });
      await new Promise((r) => setTimeout(r, 300));
    }
    if (p.model) ev({ step: 'model', detail: p.model, completed: 1, total: 1 });
    ev({ step: 'done' });
    return { endpoint: 'http://localhost:11434', model: p.model ?? null, installed: true };
  };
  // Change with AI: a deterministic stand-in (streams, then diffs by words).
  methods['ai.transform'] = async (p) => {
    const s = status();
    if (!s.available) throw new RpcError('other', s.error ?? 'AI unavailable');
    const before = String(p.text ?? '');
    const instr = String(p.instruction ?? '').toLowerCase();
    let after: string;
    if (/short/.test(instr)) after = before.split(/(?<=[.!?])\s+/)[0] ?? before;
    else if (/checklist/.test(instr))
      after = before
        .split(/[.\n]+/)
        .filter((x) => x.trim())
        .map((x) => `- [ ] ${x.trim().replace(/^[-*]\s*(\[.\]\s*)?/, '')}`)
        .join('\n');
    else
      after = before
        .replace(/\bteh\b/g, 'the')
        .replace(/\s+([,.])/g, '$1')
        .replace(/^./, (c) => c.toUpperCase());
    for (const word of after.split(/(?<=\s)/)) {
      api.emit({ type: 'custom', name: 'ai.chunk', payload: { requestId: p.requestId, text: word } } as never);
      await new Promise((r) => setTimeout(r, 15));
    }
    const tok = (x: string) => x.split(/(\s+)/).filter(Boolean);
    const a = tok(before);
    const b = tok(after);
    // LCS word diff (fine for mock-sized texts).
    const dp = Array.from({ length: a.length + 1 }, () => new Array(b.length + 1).fill(0));
    for (let i = a.length - 1; i >= 0; i--)
      for (let j = b.length - 1; j >= 0; j--) dp[i][j] = a[i] === b[j] ? dp[i + 1][j + 1] + 1 : Math.max(dp[i + 1][j], dp[i][j + 1]);
    const diff: { op: string; text: string }[] = [];
    const push = (op: string, text: string) =>
      diff.length && diff[diff.length - 1].op === op ? (diff[diff.length - 1].text += text) : diff.push({ op, text });
    let i = 0;
    let j = 0;
    while (i < a.length && j < b.length) {
      if (a[i] === b[j]) {
        push('eq', a[i]);
        i++;
        j++;
      } else if (dp[i + 1][j] >= dp[i][j + 1]) push('del', a[i++]);
      else push('ins', b[j++]);
    }
    while (i < a.length) push('del', a[i++]);
    while (j < b.length) push('ins', b[j++]);
    return { text: after, diff, provider: s.provider, model: s.model };
  };
  methods['ai.test'] = async () => {
    const s = status();
    if (!s.available) throw new RpcError('other', s.error ?? 'AI unavailable');
    await new Promise((r) => setTimeout(r, 400));
    return { provider: s.provider, model: s.model, ms: 412, reply: 'OK' };
  };
  // Clipboard → cards: a deterministic stand-in for the local AI (one card per
  // list item, else one card), so the flow can be exercised in `pnpm dev:web`.
  methods['ai.cardsFromText'] = (p) => {
    const text = String(p.text ?? '').replace(/\r\n?/g, '\n');
    if (!text.trim()) throw new RpcError('invalid', 'the clipboard has no text');
    const items = text
      .split('\n')
      .map((l) => /^\s{0,3}(?:[-*+•]|\d{1,3}[.)])\s+(?:\[[ xX]\]\s+)?(.+)$/.exec(l)?.[1]?.trim())
      .filter((x): x is string => !!x);
    const first = text
      .trim()
      .split('\n')[0]
      .replace(/^#+\s*/, '')
      .trim();
    const titles = (items.length >= 2 ? items : [first]).slice(0, 12);
    const tag = (s: string) => /#([\p{L}\p{N}_-]+)/u.exec(s)?.[1];
    const cards = titles.map((raw) => {
      const title =
        raw
          .replace(/#[\p{L}\p{N}_-]+/gu, '')
          .replace(/\s+/g, ' ')
          .trim()
          .slice(0, 120) || 'Card';
      const tg = tag(raw);
      const body = items.length >= 2 ? '' : text.trim().split('\n').slice(1).join('\n').trim();
      const parts = (h: string) => [`${h} ${title}`, body, tg ? `#${tg}` : ''].filter(Boolean).join('\n\n') + '\n';
      return { title, markdown: parts('#'), section: parts('##') };
    });
    return { cards, truncated: false, provider: 'mock', model: 'mock' };
  };
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
      list.unshift({
        id,
        created,
        title: p.title || `Activity summary · ${new Date(p.from).toLocaleDateString()}`,
        from: p.from,
        to: p.to,
        boards: p.boards ?? [],
        detail: p.detail ?? 3,
        prompt: p.prompt ?? '',
        engine: 'basic',
        model: null,
        scheduleId: p.scheduleId ?? null,
        markdown,
        delivery: [],
      });
      save('ai.summaries', list.slice(0, 200));
    }
    if (p.requestId) for (const stage of ['facts', 'done']) api.emit({ type: 'custom', name: 'summary.progress', payload: { requestId: p.requestId, stage } });
    return {
      id,
      title: 'Activity summary',
      markdown,
      engine: 'basic',
      model: null,
      fallbackReason: p.engine === 'basic' ? null : 'local AI not available',
      created,
      facts: f,
      delivery: [],
    };
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
  methods['summaries.delete'] = (p) =>
    save(
      'ai.summaries',
      load<any[]>('ai.summaries', []).filter((x) => x.id !== p.id),
    );
  methods['summaries.export'] = () => null;

  methods['schedules.list'] = () =>
    load<any[]>('ai.schedules', []).map((s) => ({ ...s, nextRun: s.enabled ? new Date(Date.now() + 86_400_000).toISOString() : null }));
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
  methods['schedules.delete'] = (p) =>
    save(
      'ai.schedules',
      load<any[]>('ai.schedules', []).filter((x) => x.id !== p.id),
    );
  methods['schedules.runNow'] = (p) => {
    const s = load<any[]>('ai.schedules', []).find((x) => x.id === p.id);
    if (!s) throw new Error('not found');
    const to = new Date();
    const from = new Date(to.getTime() - 86_400_000);
    const r = methods['activity.summarize']({
      from: from.toISOString(),
      to: to.toISOString(),
      boards: s.boards,
      detail: s.detail,
      engine: 'basic',
      title: s.name,
      scheduleId: s.id,
      links: false,
    }) as Record<string, unknown>;
    save(
      'ai.schedules',
      load<any[]>('ai.schedules', []).map((x) => (x.id === s.id ? { ...x, lastRun: to.toISOString(), lastStatus: 'ok' } : x)),
    );
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
    return {
      ok: true,
      stdout: `(mock) would run ${String(p.lang)} code (${String(p.code ?? '').length} chars)\n`,
      stderr: '',
      images: [],
      ms: 1,
      exitCode: 0,
      timedOut: false,
      truncated: false,
    };
  };
  methods['web.preview'] = (p) => {
    let host: string;
    try {
      host = new URL(String(p.url)).hostname;
    } catch {
      return null;
    }
    return {
      url: p.url,
      title: host,
      description: 'Link preview (mock)',
      image: null,
      site: host,
      favicon: null,
      insecure: String(p.url).startsWith('http:'),
      fetched: new Date().toISOString(),
    };
  };
}
