// Two-way sync between the search box text and the filter UI. The filter UI
// edits a parsed `Query` and writes it back with `rewrite`, which keeps an
// explicit `case:` token (so "case:no" survives when the default is on).

import { toText, tokenize, type Query } from './query';

const CASE_ON = ['yes', 'true', 'sensitive', '1'];

/** `true` / `false` when the text has an explicit `case:` token, else `null`. */
export function explicitCase(text: string): boolean | null {
  let v: boolean | null = null;
  for (const tok of tokenize(text)) {
    const m = /^case:(.+)$/i.exec(tok);
    if (m) v = CASE_ON.includes(m[1].replace(/"/g, '').toLowerCase());
  }
  return v;
}

/** Case sensitivity in effect: the explicit token wins over the default. */
export function effectiveCase(text: string, dflt: boolean): boolean {
  return explicitCase(text) ?? dflt;
}

/** Replace (or drop with `null`) the explicit `case:` token, keeping the rest verbatim. */
export function withCase(text: string, v: boolean | null): string {
  const toks = tokenize(text).filter((tok) => !/^case:/i.test(tok));
  if (v !== null) toks.push(v ? 'case:yes' : 'case:no');
  return toks.join(' ');
}

/** Serialize a query edited by the filter UI back into the box text. */
export function rewrite(text: string, q: Query): string {
  return withCase(toText({ ...q, caseSensitive: false }), explicitCase(text));
}

/** Text to send to the backend: applies the default case when not explicit. */
export function effectiveText(text: string, dflt: boolean): string {
  return explicitCase(text) === null && dflt ? withCase(text, true) : text;
}
