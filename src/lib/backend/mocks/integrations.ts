// Browser mock for integrations: a fake Jira Cloud site with project "LUAU",
// a Trello/Slack connect that always succeeds, mirrors, links and the WriteGate.

import type { MockApi } from '../mock';
import { RpcError } from '../rpc';
import type { RemoteInfo } from '../types';
import type {
  Account,
  ChangeRow,
  IdName,
  Mirror,
  PrepareRequest,
  Prepared,
  RemoteComment,
  RemoteIssue,
  StatusCategory,
  Transition,
} from '$lib/integrations/types';

type Methods = Record<string, (p: Record<string, any>) => unknown>;

const SITE = 'https://luau-demo.atlassian.net';
const STATUSES: { id: string; name: string; cat: StatusCategory }[] = [
  { id: '1', name: 'To Do', cat: 'todo' },
  { id: '3', name: 'In Progress', cat: 'inProgress' },
  { id: '4', name: 'In Review', cat: 'inProgress' },
  { id: '10', name: 'Done', cat: 'done' },
];
const PEOPLE = [
  { id: 'u-ana', name: 'Ana Pérez' },
  { id: 'u-luis', name: 'Luis Gómez' },
  { id: 'u-eros', name: 'Eros T.' },
];

function seedIssues(): RemoteIssue[] {
  const rows: [string, number, number | null, string, string][] = [
    ['Login fails with SSO on Safari', 1, 0, 'Bug', 'High'],
    ['Export board as PDF', 0, 2, 'Story', 'Medium'],
    ['Dark mode contrast for chips', 2, 1, 'Task', 'Low'],
    ['Offline banner copy', 3, 2, 'Task', 'Low'],
    ['Keyboard shortcut cheat sheet', 3, 0, 'Story', 'Medium'],
    ['Crash when dropping images on lanes', 1, 1, 'Bug', 'Highest'],
    ['Search filters by sprint', 0, null, 'Story', 'Medium'],
    ['Mirror board ordering by rank', 1, 2, 'Task', 'High'],
    ['Trello import: labels', 0, null, 'Task', 'Low'],
    ['Activity summary for yesterday', 2, 0, 'Story', 'Medium'],
    ['Settings search highlights', 3, 1, 'Task', 'Low'],
    ['Slack: post card to channel', 0, 2, 'Story', 'Medium'],
  ];
  // A second project so the panel shows more than one group.
  const web: [string, number, number | null, string, string][] = [
    ['Landing page hero copy', 0, 2, 'Task', 'Low'],
    ['Pricing table on mobile', 1, 2, 'Bug', 'High'],
  ];
  const all = [...rows.map((r, i) => ['LUAU', 101 + i, r] as const), ...web.map((r, i) => ['WEB', 7 + i, r] as const)];
  return all.map(([project, n, [summary, s, who, type, priority]], i) => {
    const st = STATUSES[s];
    const key = `${project}-${n}`;
    return {
      key,
      id: String(10001 + i),
      url: `${SITE}/browse/${key}`,
      summary,
      descriptionMd: `Context for **${summary.toLowerCase()}**.\n\n- [ ] Reproduce\n- [ ] Fix\n- [ ] Add a test`,
      status: st.name,
      statusId: st.id,
      statusCategory: st.cat,
      assignee: who === null ? null : PEOPLE[who],
      priority,
      labels: i % 3 === 0 ? ['web'] : [],
      type,
      sprint: 'Sprint 12',
      updated: new Date(Date.now() - i * 3_600_000).toISOString(),
      subtasks: [],
      attachments: [],
      project,
    };
  });
}

export function register(methods: Methods, api: MockApi) {
  const issues = seedIssues();
  const comments = new Map<string, RemoteComment[]>([
    ['LUAU-101', [{ id: 'm1', author: PEOPLE[1], bodyMd: 'I can reproduce this on **Safari 18**.', created: new Date(Date.now() - 86_400_000).toISOString() }]],
  ]);
  let accounts: Account[] = api.ls('integrations.accounts', [
    {
      id: 'a-demo',
      provider: 'jiraCloud',
      label: 'luau-demo.atlassian.net',
      baseUrl: SITE,
      allowedHosts: ['luau-demo.atlassian.net'],
      insecureHttp: false,
      savedQueries: [{ name: 'Bugs', query: 'project = LUAU AND type = Bug' }],
      userName: 'Eros T.',
      persisted: true,
      created: new Date().toISOString(),
    },
  ]);
  const save = () => api.lsSet('integrations.accounts', accounts);
  const links = new Map<string, Record<string, RemoteInfo>>();
  const mirrors: Mirror[] = [];
  const mirrorIds = () => new Set(mirrors.map((m) => m.id));
  const notes = new Map<string, string>();
  const pending = new Map<string, () => unknown>();
  let internal = false;

  const settingsOf = () => (methods['settings.get']?.({}) ?? {}) as Record<string, unknown>;
  const allowed = (dir: 'push' | 'pull') => {
    const s = settingsOf();
    if (dir === 'push' && s['integrations.allowPush'] !== true) throw new RpcError('invalid', 'invalid operation: push_disabled');
    if (dir === 'pull' && s['integrations.allowPull'] === false) throw new RpcError('invalid', 'invalid operation: pull_disabled');
  };
  const custom = (name: string, payload: unknown) => api.emit({ type: 'custom', name, payload });
  const info = (i: RemoteIssue, mirror = false): RemoteInfo => ({
    provider: 'jira',
    account: 'a-demo',
    key: i.key,
    url: i.url,
    status: i.status,
    statusCategory: i.statusCategory,
    assignee: i.assignee ?? null,
    priority: i.priority,
    labels: i.labels,
    type: i.type,
    sprint: i.sprint,
    updated: i.updated,
    mirror,
  });
  const issue = (key: string) => {
    const i = issues.find((x) => x.key === key);
    if (!i) throw new RpcError('not_found', 'not found: remote_404');
    return i;
  };
  const compose = (i: RemoteIssue) => `# ${i.summary}\n\n${i.descriptionMd}\n\nJira: [${i.key}](${i.url})\n`;
  const emitLinks = (board: string) => custom('integrations.links', { boardId: board, links: links.get(board) ?? {} });
  const apply = (board: string, op: Record<string, unknown>) => {
    internal = true;
    try {
      return methods['board.apply']({ board, op, label: 'sync' });
    } finally {
      internal = false;
    }
  };

  // Mirrors are read-only boards.
  const wrap = (name: string, f: (r: any, p: Record<string, any>) => unknown) => {
    const orig = methods[name];
    methods[name] = (p) => f(orig(p), p);
  };
  wrap('board.open', (snap) => {
    if (mirrorIds().has(snap.header.id)) snap.header.readOnly = 'mirror:jira';
    return snap;
  });
  wrap('board.snapshot', (snap) => {
    if (mirrorIds().has(snap.header.id)) snap.header.readOnly = 'mirror:jira';
    return snap;
  });
  wrap('registry.get', (reg) => {
    for (const b of reg.boards) if (mirrorIds().has(b.id)) b.mirror = true;
    return reg;
  });
  const origApply = methods['board.apply'];
  methods['board.apply'] = (p) => {
    if (!internal && mirrorIds().has(p.board)) throw new RpcError('read_only', 'board is read-only: mirror');
    return origApply(p);
  };

  function search(query: string): RemoteIssue[] {
    const q = (query || '').toLowerCase();
    if (!q || q.includes('currentuser')) return issues.filter((i) => i.assignee?.id === 'u-eros' || (!q && i.statusCategory !== 'done'));
    if (q.includes('type = bug')) return issues.filter((i) => i.type === 'Bug');
    const text = q
      .replace(/project\s*=\s*\w+/g, '')
      .replace(/and|order by.*$/g, '')
      .trim();
    return issues.filter((i) => !text || `${i.key} ${i.summary}`.toLowerCase().includes(text));
  }

  function mirrorSync(id: string) {
    const m = mirrors.find((x) => x.id === id);
    if (!m) throw new RpcError('invalid', 'not_a_mirror');
    const snap = api.snapshot(id);
    const lanes = new Map(snap.lanes.map((l) => [l.name, l.id]));
    const map = (links.get(id) ?? {}) as Record<string, RemoteInfo>;
    const byKey = new Map(Object.entries(map).map(([card, r]) => [r.key, card]));
    for (const st of STATUSES) {
      if (!lanes.has(st.name)) {
        const lid = methods['board.newLaneId']({}) as string;
        apply(id, { op: 'createLane', id: lid, name: st.name, index: null });
        lanes.set(st.name, lid);
      }
    }
    const next: Record<string, RemoteInfo> = {};
    for (const i of issues) {
      let card = byKey.get(i.key);
      const lane = lanes.get(i.status)!;
      if (!card) {
        card = methods['board.newCardId']({}) as string;
        apply(id, { op: 'createCard', id: card, parent: { kind: 'lane', id: lane }, index: null, content: compose(i) });
      } else {
        apply(id, { op: 'writeCard', id: card, content: compose(i) });
        apply(id, { op: 'move', ids: [card], to: { kind: 'lane', id: lane }, before: null });
      }
      next[card] = info(i, true);
    }
    links.set(id, next);
    m.lastSync = new Date().toISOString();
    emitLinks(id);
    custom('integrations.mirrors', {});
  }

  function gate(
    direction: 'push' | 'pull',
    service: string,
    target: string,
    changes: ChangeRow[],
    run: () => unknown,
    preview: string | null = null,
  ): Prepared {
    allowed(direction);
    const token = 'w' + Math.random().toString(36).slice(2);
    const fields = [...new Set(changes.map((c) => c.field))];
    const empty = !changes.length && !preview;
    if (!empty) pending.set(token, run);
    return { token, direction, service, site: service === 'slack' ? 'slack.com' : 'luau-demo.atlassian.net', target, fields, changes, preview, empty };
  }

  const linkOf = (board?: string, card?: string) => (board && card ? links.get(board)?.[card] : undefined);

  methods['integrations.accounts'] = () => accounts;
  methods['integrations.connect'] = (p) => {
    if (!p.token) throw new RpcError('invalid', 'invalid operation: missing_token');
    const provider = p.provider as Account['provider'];
    const host =
      provider === 'trello'
        ? 'api.trello.com'
        : provider === 'slack'
          ? 'slack.com'
          : String(p.baseUrl)
              .replace(/^https?:\/\//, '')
              .replace(/\/.*$/, '');
    const a: Account = {
      id: 'a' + Math.random().toString(36).slice(2, 8),
      provider,
      label: provider === 'trello' ? 'Trello' : provider === 'slack' ? 'Luau workspace' : host,
      baseUrl: `https://${host}`,
      allowedHosts: [host],
      insecureHttp: !!p.insecureHttp,
      savedQueries: [],
      userName: 'Eros T.',
      persisted: true,
      created: new Date().toISOString(),
    };
    accounts = [...accounts, a];
    save();
    return a;
  };
  methods['integrations.test'] = (p) => accounts.find((a) => a.id === p.account);
  methods['integrations.remove'] = (p) => {
    accounts = accounts.filter((a) => a.id !== p.account);
    save();
    return true;
  };
  methods['integrations.update'] = (p) => {
    const a = accounts.find((x) => x.id === p.account);
    if (!a) throw new RpcError('not_found', 'account');
    Object.assign(a, p.patch);
    save();
    return a;
  };
  methods['integrations.mirrors'] = () => mirrors;
  methods['integrations.tick'] = () => true;

  methods['remote.search'] = (p) => {
    const q = String(p.query ?? '');
    if (p.mode === 'text' && q.trim()) {
      // Plain text: the real core builds `text ~ "…"`; the mock matches literally.
      const needle = q.trim().toLowerCase();
      return { issues: issues.filter((i) => `${i.key} ${i.summary} ${i.descriptionMd}`.toLowerCase().includes(needle)), next: null, total: null };
    }
    // Mimic Jira's 400 for broken JQL (unbalanced quotes / parentheses).
    const quotes = (q.match(/"/g) ?? []).length;
    const parens = (q.match(/\(/g) ?? []).length - (q.match(/\)/g) ?? []).length;
    if (quotes % 2 || parens)
      throw new RpcError('invalid', `invalid operation: remote_bad_request:Error in the JQL Query: ${quotes % 2 ? 'missing closing quote' : "Expecting ')'"}.`);
    return { issues: search(q), next: null, total: null };
  };
  methods['remote.issue'] = (p) => issue(p.key);
  methods['remote.transitions'] = (p): Transition[] => {
    const i = issue(p.key);
    return STATUSES.filter((s) => s.name !== i.status).map((s) => ({ id: s.id, name: s.name, to: s.name, toCategory: s.cat }));
  };
  methods['remote.comments'] = (p) => comments.get(p.key) ?? [];
  methods['remote.users'] = (p) => PEOPLE.filter((u) => u.name.toLowerCase().includes(String(p.q).toLowerCase()));
  methods['remote.projects'] = (): IdName[] => [
    { id: '100', name: 'Luau', key: 'LUAU' },
    { id: '101', name: 'Website', key: 'WEB' },
  ];
  methods['remote.issueTypes'] = (): IdName[] => ['Task', 'Story', 'Bug'].map((n, i) => ({ id: String(i + 1), name: n }));
  methods['remote.boards'] = (): IdName[] => [{ id: '7', name: 'Luau board', key: 'LUAU', detail: 'kanban' }];

  methods['remote.links'] = (p) => links.get(p.board) ?? {};
  methods['remote.link'] = (p) => {
    const i = issue(p.key);
    const existing = Object.entries(links.get(p.board) ?? {}).find(([, r]) => r.key === p.key);
    if (existing && !p.force) throw new RpcError('conflict', `conflict: already_linked:${existing[0]}`);
    const id = methods['board.newCardId']({}) as string;
    const snap = api.snapshot(p.board);
    const parent = p.parent;
    const list = parent.kind === 'lane' ? (snap.lanes.find((l) => l.id === parent.id)?.order ?? []) : parent.kind === 'root' ? snap.rootOrder : [];
    const index = p.before ? list.indexOf(p.before) : null;
    methods['board.apply']({
      board: p.board,
      op: { op: 'createCard', id, parent, index: index !== null && index >= 0 ? index : null, content: compose(i) },
      label: 'Add Jira issue',
    });
    links.set(p.board, { ...(links.get(p.board) ?? {}), [id]: info(i) });
    emitLinks(p.board);
    return id;
  };
  // Many issues → one `batch` op (one undo step), list order kept.
  methods['remote.linkMany'] = (p) => {
    const keys = [...new Set((p.keys as string[]).map((k) => k.trim()).filter(Boolean))];
    if (!keys.length) throw new RpcError('invalid', 'invalid operation: no_issues');
    if (keys.length > 100) throw new RpcError('invalid', 'invalid operation: too_many_issues:100');
    const map = links.get(p.board) ?? {};
    // Like the core: only links whose card is still on the board count (undo keeps the entries).
    const snap0 = api.snapshot(p.board);
    const live = new Set(snap0.nodes.map((n) => n.id));
    const byKey = new Map(
      Object.entries(map)
        .filter(([card]) => live.has(card))
        .map(([card, r]) => [r.key, card]),
    );
    const skipped = keys.filter((k) => byKey.has(k)).map((key) => ({ key, card: byKey.get(key)! }));
    const todo = keys.filter((k) => !byKey.has(k));
    const found = todo.map((k) => issues.find((x) => x.key === k)).filter((x): x is RemoteIssue => !!x);
    const missing = todo.filter((k) => !issues.some((x) => x.key === k));
    if (!found.length) return { created: [], skipped, missing };
    const snap = api.snapshot(p.board);
    const parent = p.parent;
    const list = parent.kind === 'lane' ? (snap.lanes.find((l) => l.id === parent.id)?.order ?? []) : parent.kind === 'root' ? snap.rootOrder : [];
    const at = p.before ? list.indexOf(p.before) : -1;
    const created: string[] = [];
    const ops = found.map((i, n) => {
      let id = methods['board.newCardId']({}) as string;
      while (created.includes(id)) id = methods['board.newCardId']({}) as string;
      created.push(id);
      return { op: 'createCard', id, parent, index: at >= 0 ? at + n : null, content: compose(i) };
    });
    methods['board.apply']({ board: p.board, op: { op: 'batch', ops }, label: `Add ${created.length} Jira issues` });
    const next = { ...map };
    created.forEach((id, n) => (next[id] = info(found[n])));
    links.set(p.board, next);
    emitLinks(p.board);
    return { created, skipped, missing };
  };
  methods['remote.copyFromMirror'] = (p) =>
    (p.ids as string[]).flatMap((id) => {
      const r = links.get(p.from)?.[id];
      return r ? [methods['remote.link']({ board: p.to, parent: p.parent, before: p.before, account: r.account, key: r.key })] : [];
    });
  methods['remote.refresh'] = (p) => {
    const map = links.get(p.board) ?? {};
    for (const [card, r] of Object.entries(map)) map[card] = info(issue(r.key), r.mirror);
    emitLinks(p.board);
    return Object.keys(map).length;
  };
  methods['remote.unlink'] = (p) => {
    const map = links.get(p.board) ?? {};
    delete map[p.card];
    emitLinks(p.board);
    return true;
  };
  methods['remote.notes.get'] = (p) => notes.get(`${p.board}:${p.card}`) ?? '';
  methods['remote.notes.set'] = (p) => (notes.set(`${p.board}:${p.card}`, p.text), true);

  methods['remote.mirror.create'] = (p) => {
    allowed('pull');
    const snap = methods['board.create']({ path: `mirrors/${p.name}`, name: p.name, kind: 'kanban', lanes: [] }) as { header: { id: string } };
    const id = snap.header.id;
    mirrors.push({
      id,
      name: p.name,
      path: `mirrors/${id}`,
      account: p.account,
      provider: 'jiraCloud',
      source: p.source,
      watch: !!p.watch,
      lastSync: null,
      lastError: null,
    });
    mirrorSync(id);
    api.emit({ type: 'registryChanged', registry: methods['registry.get']({}) as any });
    return id;
  };
  methods['remote.mirror.sync'] = (p) => (allowed('pull'), mirrorSync(p.board), true);
  methods['remote.mirror.watch'] = (p) => {
    const m = mirrors.find((x) => x.id === p.board);
    if (m) m.watch = !!p.watch;
    return true;
  };
  methods['remote.mirror.remove'] = (p) => {
    const i = mirrors.findIndex((x) => x.id === p.board);
    if (i >= 0) mirrors.splice(i, 1);
    api.boards.delete(p.board);
    api.emit({ type: 'registryChanged', registry: methods['registry.get']({}) as any });
    return true;
  };

  methods['remote.prepare'] = (p) => {
    const r = p.request as PrepareRequest;
    switch (r.kind) {
      case 'push': {
        const l = linkOf(r.board, r.card);
        if (!l) throw new RpcError('not_found', 'not found: not_linked');
        const i = issue(l.key);
        const content = String(methods['card.read']({ board: r.board, id: r.card }));
        const title = content.match(/^# (.*)$/m)?.[1] ?? '';
        const body = content
          .replace(/^# .*\n+/, '')
          .replace(new RegExp(`\\n*Jira: \\[${l.key}\\]\\(.*\\)\\s*$`), '')
          .trim();
        const changes = [
          ...(title !== i.summary ? [{ field: 'Summary', before: i.summary, after: title }] : []),
          ...(body !== i.descriptionMd.trim() ? [{ field: 'Description', before: i.descriptionMd.slice(0, 400), after: body.slice(0, 400) }] : []),
        ];
        return gate('push', 'jira', `${i.key} “${i.summary}”`, changes, () => {
          i.summary = title;
          i.descriptionMd = body;
          return { key: i.key };
        });
      }
      case 'pull': {
        if (mirrorIds().has(r.board)) {
          const m = mirrors.find((x) => x.id === r.board)!;
          return gate('pull', 'jira', m.name, [{ field: 'Issue', before: null, after: `${issues.length} issues` }], () => mirrorSync(r.board));
        }
        const l = linkOf(r.board, r.card ?? undefined);
        if (!l) throw new RpcError('not_found', 'not found: not_linked');
        const i = issue(l.key);
        const current = String(methods['card.read']({ board: r.board, id: r.card }));
        const next = compose(i);
        const changes = current === next ? [] : [{ field: 'Description', before: current.slice(0, 200), after: next.slice(0, 200) }];
        return gate('pull', 'jira', i.summary, changes, () =>
          methods['board.apply']({ board: r.board, op: { op: 'writeCard', id: r.card, content: next }, label: 'Pulled from Jira' }),
        );
      }
      case 'comment': {
        const key = linkOf(r.board, r.card)?.key ?? r.key!;
        return gate(
          'push',
          'jira',
          key,
          [{ field: 'Comment', before: null, after: r.body.slice(0, 120) }],
          () => {
            comments.set(key, [...(comments.get(key) ?? []), { id: 'm' + Date.now(), author: PEOPLE[2], bodyMd: r.body, created: new Date().toISOString() }]);
            return { key };
          },
          r.body,
        );
      }
      case 'transition': {
        const l = linkOf(r.board, r.card);
        const key = l?.key ?? r.key!;
        const i = issue(key);
        return gate('push', 'jira', key, [{ field: 'Status', before: i.status, after: r.to }], () => {
          const st = STATUSES.find((s) => s.id === r.id)!;
          Object.assign(i, { status: st.name, statusId: st.id, statusCategory: st.cat, updated: new Date().toISOString() });
          for (const [b, map] of links) {
            let touched = false;
            for (const [card, info0] of Object.entries(map))
              if (info0.key === key) {
                map[card] = info(i, info0.mirror);
                touched = true;
              }
            if (touched) mirrorIds().has(b) ? mirrorSync(b) : emitLinks(b);
          }
          return { key };
        });
      }
      case 'assign': {
        const key = linkOf(r.board, r.card)?.key ?? r.key!;
        const i = issue(key);
        return gate('push', 'jira', key, [{ field: 'Assignee', before: i.assignee?.name ?? null, after: r.user?.name ?? null }], () => {
          i.assignee = r.user;
          methods['remote.refresh']({ board: r.board });
          return { key };
        });
      }
      case 'create': {
        const content = String(methods['card.read']({ board: r.board, id: r.card }));
        const title = content.match(/^# (.*)$/m)?.[1] ?? content.split('\n')[0];
        return gate(
          'push',
          'jira',
          r.project,
          [
            { field: 'Project', after: r.project },
            { field: 'Type', after: r.issueType ?? 'Task' },
            { field: 'Summary', after: title },
          ],
          () => {
            const key = `LUAU-${101 + issues.length}`;
            const i: RemoteIssue = {
              ...issues[0],
              key,
              id: String(20000 + issues.length),
              url: `${SITE}/browse/${key}`,
              summary: title,
              descriptionMd: '',
              status: 'To Do',
              statusId: '1',
              statusCategory: 'todo',
              assignee: null,
              type: r.issueType ?? 'Task',
            };
            issues.push(i);
            methods['board.apply']({
              board: r.board,
              op: { op: 'writeCard', id: r.card, content: `${content.trimEnd()}\n\nJira: [${key}](${i.url})\n` },
              label: `Linked to ${key}`,
            });
            links.set(r.board, { ...(links.get(r.board) ?? {}), [r.card]: info(i) });
            emitLinks(r.board);
            return { key, url: i.url };
          },
        );
      }
      case 'slackPost':
        return gate('push', 'slack', `#${r.channelName}`, [{ field: 'Message', after: r.text.slice(0, 120) }], () => ({ ok: true }), r.text);
    }
  };
  methods['remote.commit'] = (p) => {
    const run = pending.get(p.token);
    if (!run) throw new RpcError('invalid', 'invalid operation: write_token_invalid');
    pending.delete(p.token);
    return run() ?? true;
  };
  methods['remote.cancel'] = (p) => (pending.delete(p.token), true);
  methods['slack.channels'] = (): IdName[] => [
    { id: 'C01GENERAL', name: 'general' },
    { id: 'C02PRODUCT', name: 'product' },
    { id: 'G03CORE', name: 'core-team', detail: 'private' },
  ];
}
