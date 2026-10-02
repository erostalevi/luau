// Board quick-filter: a tiny query language over card metadata.
//   words · #tag · tag:x · @person · is:open|done|archived|group|jira
//   has:image|attachment|tasks|links|due|start|tags · priority:high
//   due:overdue|today|<date|<=date|>date|>=date|date · started: (same forms)

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

/** Keys the quick filter understands (for autocomplete). */
export const FILTER_KEYS = ['is', 'has', 'priority', 'due', 'started', 'start', 'tag'] as const;

/** `today`, `overdue`, or a comparison against an ISO date prefix. */
function dateTest(v: string, today: string): (d: string | null) => boolean {
  if (v === 'overdue') return (d) => !!d && d.slice(0, 10) < today;
  if (v === 'today') return (d) => !!d && d.slice(0, 10) === today;
  const m = /^(<=|>=|<|>)?(.*)$/.exec(v)!;
  const [op, x] = [m[1] ?? '', m[2]];
  return (d) => {
    if (!d) return false;
    const s = d.slice(0, x.length || 10);
    if (op === '<') return s < x;
    if (op === '<=') return s <= x;
    if (op === '>') return s > x;
    if (op === '>=') return s >= x;
    return d.startsWith(x);
  };
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
        v === 'open'
          ? n.tasks.total > n.tasks.done
          : v === 'done'
            ? (n.tasks.total > 0 && n.tasks.done === n.tasks.total) || r?.statusCategory === 'done'
            : v === 'archived'
              ? n.archived
              : v === 'group'
                ? n.isGroup
                : v === 'remote' || v === 'jira'
                  ? !!r
                  : true;
    } else if (s.startsWith('due:')) {
      const test = dateTest(s.slice(4), today);
      p = (n) => test(n.footer.due ?? n.dates[0] ?? null);
    } else if (s.startsWith('started:') || s.startsWith('start:')) {
      const test = dateTest(s.slice(s.indexOf(':') + 1), today);
      p = (n) => test(n.footer.start);
    } else if (s.startsWith('tag:') || s.startsWith('label:')) {
      const tag = s.slice(s.indexOf(':') + 1).replace(/^#/, '');
      p = (n) => [...n.tags, ...n.footer.labels].some((x) => x.toLowerCase() === tag || x.toLowerCase().startsWith(tag + '/'));
    } else if (s.startsWith('has:')) {
      const v = s.slice(4);
      p = (n) =>
        v === 'image' || v === 'images'
          ? n.attachments.some((a) => a.kind === 'image') || n.cover !== null
          : v === 'attachment' || v === 'attachments' || v === 'file'
            ? n.attachments.length > 0
            : v === 'tasks' || v === 'checklist'
              ? n.tasks.total > 0
              : v === 'links' || v === 'link'
                ? n.links.length > 0
                : v === 'due'
                  ? !!(n.footer.due ?? n.dates[0])
                  : v === 'start'
                    ? !!n.footer.start
                    : v === 'tags' || v === 'tag'
                      ? n.tags.length + n.footer.labels.length > 0
                      : true;
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
