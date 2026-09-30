// Search query syntax — a faithful TS mirror of crates/lull-core/src/search/query.rs
// so the filter UI can stay in two-way sync with the query text.
//
//   login "exact phrase" -draft tag:backend -tag:wip board:"Project Alpha"
//   lane:Doing is:open has:image due:<2026-10-10 updated:>=2026-09-01
//   links:c1a2b3c linkedfrom:c1a2b3c case:yes in:title
//
// Unknown `key:value` pairs are treated as plain text (like the backend).

export type Cmp = 'eq' | 'lt' | 'le' | 'gt' | 'ge';

export interface Term {
  value: string;
  negate: boolean;
  phrase: boolean;
}

export interface Filter {
  key: string;
  cmp: Cmp;
  value: string;
  negate: boolean;
}

export interface Query {
  terms: Term[];
  filters: Filter[];
  caseSensitive: boolean;
  titleOnly: boolean;
}

export const KEYS = [
  'tag', 'label', 'board', 'lane', 'is', 'has', 'status', 'priority', 'assignee', 'mention', 'due', 'updated',
  'created', 'links', 'linkedfrom', 'type', 'in', 'case', 'id', 'key',
] as const;

const KEY_SET = new Set<string>(KEYS);

export function emptyQuery(): Query {
  return { terms: [], filters: [], caseSensitive: false, titleOnly: false };
}

export function tokenize(s: string): string[] {
  const out: string[] = [];
  let cur = '';
  let inQuote = false;
  for (const c of s) {
    if (c === '"') {
      inQuote = !inQuote;
      cur += c;
    } else if (/\s/.test(c) && !inQuote) {
      if (cur) {
        out.push(cur);
        cur = '';
      }
    } else cur += c;
  }
  if (cur) out.push(cur);
  return out;
}

function unquote(s: string): [string, boolean] {
  if (s.length >= 2 && s.startsWith('"') && s.endsWith('"')) return [s.slice(1, -1), true];
  return [s.replace(/^"+|"+$/g, ''), false];
}

export function parse(input: string): Query {
  const q = emptyQuery();
  for (const tok of tokenize(input)) {
    const negate = tok.startsWith('-') && tok.length > 1;
    const body = negate ? tok.slice(1) : tok;
    const colon = body.indexOf(':');
    if (colon >= 0) {
      const k = body.slice(0, colon);
      const v = body.slice(colon + 1);
      const key = k.toLowerCase();
      if (KEY_SET.has(key) && v !== '' && !k.startsWith('"')) {
        let cmp: Cmp = 'eq';
        let rest = v;
        if (v.startsWith('<=')) [cmp, rest] = ['le', v.slice(2)];
        else if (v.startsWith('>=')) [cmp, rest] = ['ge', v.slice(2)];
        else if (v.startsWith('<')) [cmp, rest] = ['lt', v.slice(1)];
        else if (v.startsWith('>')) [cmp, rest] = ['gt', v.slice(1)];
        const [value] = unquote(rest);
        if (key === 'case') q.caseSensitive = ['yes', 'true', 'sensitive', '1'].includes(value);
        else if (key === 'in' && value === 'title') q.titleOnly = true;
        else q.filters.push({ key, cmp, value: value.replace(/^[@#]+/, ''), negate });
        continue;
      }
    }
    const [value, phrase] = unquote(body);
    if (value) q.terms.push({ value, negate, phrase });
  }
  return q;
}

const OPS: Record<Cmp, string> = { eq: '', lt: '<', le: '<=', gt: '>', ge: '>=' };

/** Serialize back to the canonical text form (same as `Query::to_text`). */
export function toText(q: Query): string {
  const parts: string[] = [];
  for (const t of q.terms) {
    const v = t.phrase || t.value.includes(' ') ? `"${t.value}"` : t.value;
    parts.push(t.negate ? `-${v}` : v);
  }
  for (const f of q.filters) {
    const v = f.value.includes(' ') ? `"${f.value}"` : f.value;
    parts.push(`${f.negate ? '-' : ''}${f.key}:${OPS[f.cmp]}${v}`);
  }
  if (q.caseSensitive) parts.push('case:yes');
  if (q.titleOnly) parts.push('in:title');
  return parts.join(' ');
}

export function isEmpty(q: Query): boolean {
  return q.terms.length === 0 && q.filters.length === 0;
}

// --- filter-UI helpers (pure; return new queries) ---------------------------

const clone = (q: Query): Query => ({ ...q, terms: [...q.terms], filters: [...q.filters] });

/** Positive, equality values for a key (e.g. all `tag:` chips). */
export function values(q: Query, key: string): string[] {
  return q.filters.filter((f) => f.key === key && !f.negate && f.cmp === 'eq').map((f) => f.value);
}

export function hasValue(q: Query, key: string, value: string): boolean {
  const v = value.toLowerCase();
  return q.filters.some((f) => f.key === key && !f.negate && f.cmp === 'eq' && f.value.toLowerCase() === v);
}

/** Replace all positive equality filters of `key` with `vals` (keeps order of others). */
export function setValues(q: Query, key: string, vals: string[]): Query {
  const n = clone(q);
  n.filters = n.filters.filter((f) => !(f.key === key && !f.negate && f.cmp === 'eq'));
  for (const value of vals) if (value.trim()) n.filters.push({ key, cmp: 'eq', value: value.trim().replace(/^[@#]+/, ''), negate: false });
  return n;
}

export function toggleValue(q: Query, key: string, value: string): Query {
  const cur = values(q, key);
  const on = hasValue(q, key, value);
  return setValues(q, key, on ? cur.filter((v) => v.toLowerCase() !== value.toLowerCase()) : [...cur, value]);
}

export function removeFilter(q: Query, index: number): Query {
  const n = clone(q);
  n.filters.splice(index, 1);
  return n;
}

export interface DateRange {
  /** A keyword the backend understands (`overdue`, `today`) instead of a range. */
  preset: string | null;
  from: string | null;
  to: string | null;
}

/** Read a date filter (`due` / `updated`) as a range. */
export function getRange(q: Query, key: string): DateRange {
  const r: DateRange = { preset: null, from: null, to: null };
  for (const f of q.filters) {
    if (f.key !== key || f.negate) continue;
    if (f.cmp === 'eq' && !/^\d{4}-\d{2}-\d{2}/.test(f.value)) r.preset = f.value.toLowerCase();
    else if (f.cmp === 'ge' || f.cmp === 'gt') r.from = f.value;
    else if (f.cmp === 'le' || f.cmp === 'lt') r.to = f.value;
    else if (f.cmp === 'eq') r.from = r.to = f.value;
  }
  return r;
}

export function setRange(q: Query, key: string, r: DateRange): Query {
  const n = clone(q);
  n.filters = n.filters.filter((f) => !(f.key === key && !f.negate));
  if (r.preset) n.filters.push({ key, cmp: 'eq', value: r.preset, negate: false });
  else if (r.from && r.to && r.from === r.to) n.filters.push({ key, cmp: 'eq', value: r.from, negate: false });
  else {
    if (r.from) n.filters.push({ key, cmp: 'ge', value: r.from, negate: false });
    if (r.to) n.filters.push({ key, cmp: 'le', value: r.to, negate: false });
  }
  return n;
}

/** Number of structured filters (chips) — terms and flags excluded. */
export function filterCount(q: Query): number {
  return q.filters.length;
}

/** Plain words/phrases to highlight client-side (e.g. in titles). */
export function highlightTerms(q: Query): string[] {
  return q.terms.filter((t) => !t.negate).map((t) => t.value);
}
