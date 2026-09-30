// Board quick-filter: a tiny query language over card metadata.
//   words · #tag · @person · is:open|done|archived|group · due:overdue|today|<date · priority:high

import type { NodeDto, RemoteInfo } from '$lib/backend/types';

export interface CompiledFilter {
  test(n: NodeDto, remote?: RemoteInfo): boolean;
  empty: boolean;
}

function faceText(n: NodeDto): string {
  const f = n.face;
  if (f.kind === 'summary') return f.text;
  if (f.kind === 'checklist' || f.kind === 'list') return f.items.map((i) => i.text).join(' ');
  if (f.kind === 'table') return [...f.header, ...f.rows.flat()].join(' ');
  return '';
}

export function compileFilter(text: string): CompiledFilter {
  const tokens = text.trim().split(/\s+/).filter(Boolean);
  if (!tokens.length) return { empty: true, test: () => true };
  const today = new Date().toISOString().slice(0, 10);
  const preds = tokens.map((tok) => {
    const neg = tok.startsWith('-') && tok.length > 1;
    const s = (neg ? tok.slice(1) : tok).toLowerCase();
    let p: (n: NodeDto, r?: RemoteInfo) => boolean;
    if (s.startsWith('#')) {
      const tag = s.slice(1);
      p = (n) => [...n.tags, ...n.footer.labels].some((x) => x.toLowerCase() === tag || x.toLowerCase().startsWith(tag + '/'));
    } else if (s.startsWith('@')) {
      const who = s.slice(1);
      p = (n, r) => n.mentions.some((m) => m.toLowerCase().includes(who)) || !!r?.assignee?.name.toLowerCase().includes(who);
    } else if (s.startsWith('is:')) {
      const v = s.slice(3);
      p = (n, r) =>
        v === 'open' ? n.tasks.total > n.tasks.done
        : v === 'done' ? (n.tasks.total > 0 && n.tasks.done === n.tasks.total) || r?.statusCategory === 'done'
        : v === 'archived' ? n.archived
        : v === 'group' ? n.isGroup
        : v === 'remote' || v === 'jira' ? !!r
        : true;
    } else if (s.startsWith('due:')) {
      const v = s.slice(4);
      const due = (n: NodeDto) => n.footer.due ?? n.dates[0] ?? null;
      p = (n) => {
        const d = due(n);
        if (!d) return false;
        if (v === 'overdue') return d < today;
        if (v === 'today') return d.slice(0, 10) === today;
        if (v.startsWith('<')) return d < v.slice(1);
        if (v.startsWith('>')) return d > v.slice(1);
        return d.startsWith(v);
      };
    } else if (s.startsWith('priority:')) {
      const v = s.slice(9);
      p = (n, r) => (n.footer.priority ?? r?.priority ?? '').toLowerCase() === v;
    } else {
      p = (n, r) =>
        n.title.toLowerCase().includes(s) ||
        faceText(n).toLowerCase().includes(s) ||
        n.tags.some((x) => x.toLowerCase().includes(s)) ||
        !!r?.key.toLowerCase().includes(s);
    }
    return neg ? (n: NodeDto, r?: RemoteInfo) => !p(n, r) : p;
  });
  return { empty: false, test: (n, r) => preds.every((p) => p(n, r)) };
}
