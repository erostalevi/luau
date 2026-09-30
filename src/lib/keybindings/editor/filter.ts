// Text filter for the keyboard shortcuts table.
//   "move card"         words matched against title, category, id, when
//   @source:user        default | user | preset | extension | none
//   @singlekey          bindings affected by the single-key toggle
//   @unbound            commands without a keybinding

import { isSingleKeyBinding, matchesKeys, type BindingRow } from './model';

export interface RowText {
  title: string;
  category: string;
}

export interface KbQuery {
  words: string[];
  sources: string[];
  singleKey: boolean;
  unbound: boolean;
}

export function parseKbQuery(q: string): KbQuery {
  const out: KbQuery = { words: [], sources: [], singleKey: false, unbound: false };
  for (const raw of q.trim().split(/\s+/).filter(Boolean)) {
    const w = raw.toLowerCase();
    if (w.startsWith('@source:')) out.sources.push(w.slice(8));
    else if (w === '@singlekey') out.singleKey = true;
    else if (w === '@unbound') out.unbound = true;
    else out.words.push(w);
  }
  return out;
}

export function filterRows(rows: BindingRow[], q: KbQuery, text: (r: BindingRow) => RowText, keys?: string | null): BindingRow[] {
  return rows.filter((r) => {
    if (keys && !matchesKeys(r.key, keys)) return false;
    if (q.sources.length && !q.sources.includes(r.source)) return false;
    if (q.singleKey && !isSingleKeyBinding(r)) return false;
    if (q.unbound && r.key) return false;
    if (!q.words.length) return true;
    const tx = text(r);
    const hay = `${tx.title} ${tx.category} ${r.command} ${r.when ?? ''} ${r.key}`.toLowerCase();
    return q.words.every((w) => hay.includes(w));
  });
}
