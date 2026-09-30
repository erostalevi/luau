// Pure helpers for rendering search results (snippets, highlights, grouping).

import type { SearchHit } from '$lib/backend/types';

export interface Segment {
  text: string;
  hit: boolean;
}

/** Split a backend snippet marked with \u0002…\u0003 into segments (no HTML). */
export function snippetSegments(s: string): Segment[] {
  const out: Segment[] = [];
  let hit = false;
  let cur = '';
  for (const c of s) {
    if (c === '\u0002' || c === '\u0003') {
      if (cur) out.push({ text: cur, hit });
      cur = '';
      hit = c === '\u0002';
    } else cur += c;
  }
  if (cur) out.push({ text: cur, hit });
  return out;
}

/** Highlight plain terms inside a string (titles have no backend markers). */
export function highlightSegments(s: string, terms: string[], caseSensitive = false): Segment[] {
  const ts = terms.filter((x) => x.length > 0);
  if (!ts.length || !s) return [{ text: s, hit: false }];
  const hay = caseSensitive ? s : s.toLowerCase();
  const needles = ts.map((x) => (caseSensitive ? x : x.toLowerCase()));
  const out: Segment[] = [];
  let i = 0;
  let plain = '';
  while (i < s.length) {
    let len = 0;
    for (const n of needles) if (n.length > len && hay.startsWith(n, i)) len = n.length;
    if (len) {
      if (plain) out.push({ text: plain, hit: false });
      plain = '';
      out.push({ text: s.slice(i, i + len), hit: true });
      i += len;
    } else {
      plain += s[i];
      i++;
    }
  }
  if (plain) out.push({ text: plain, hit: false });
  return out;
}

export interface HitGroup {
  key: string;
  label: string;
  hits: SearchHit[];
}

/** Group hits by board, keeping the rank order of first appearance. */
export function groupByBoard(hits: SearchHit[]): HitGroup[] {
  const map = new Map<string, HitGroup>();
  for (const h of hits) {
    let g = map.get(h.board);
    if (!g) map.set(h.board, (g = { key: h.board, label: h.boardName, hits: [] }));
    g.hits.push(h);
  }
  return [...map.values()];
}

/** Group hits by lane name (merged across boards); cards without a lane last. */
export function groupByLane(hits: SearchHit[], noLane: string): HitGroup[] {
  const map = new Map<string, HitGroup>();
  for (const h of hits) {
    const name = h.laneName?.trim() || '';
    const key = name.toLowerCase();
    let g = map.get(key);
    if (!g) map.set(key, (g = { key: key || '\u0000', label: name || noLane, hits: [] }));
    g.hits.push(h);
  }
  const list = [...map.values()];
  return [...list.filter((g) => g.key !== '\u0000'), ...list.filter((g) => g.key === '\u0000')];
}
