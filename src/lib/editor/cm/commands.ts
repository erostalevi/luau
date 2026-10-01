// Markdown & text editing commands operating on an EditorView.

import { EditorSelection, type ChangeSpec, type SelectionRange } from '@codemirror/state';
import type { EditorView } from '@codemirror/view';

type Cmd = (view: EditorView) => boolean;

/** Wrap/unwrap each selection with `mark` (e.g. ** for bold). */
export function toggleWrap(mark: string, placeholder = ''): Cmd {
  return (view) => {
    const { state } = view;
    const tr = state.changeByRange((range) => {
      const text = state.sliceDoc(range.from, range.to);
      const before = state.sliceDoc(range.from - mark.length, range.from);
      const after = state.sliceDoc(range.to, range.to + mark.length);
      if (before === mark && after === mark) {
        return {
          changes: [
            { from: range.from - mark.length, to: range.from },
            { from: range.to, to: range.to + mark.length },
          ],
          range: EditorSelection.range(range.from - mark.length, range.to - mark.length),
        };
      }
      if (text.startsWith(mark) && text.endsWith(mark) && text.length >= mark.length * 2) {
        const inner = text.slice(mark.length, text.length - mark.length);
        return { changes: { from: range.from, to: range.to, insert: inner }, range: EditorSelection.range(range.from, range.from + inner.length) };
      }
      const body = text || placeholder;
      return {
        changes: { from: range.from, to: range.to, insert: mark + body + mark },
        range: EditorSelection.range(range.from + mark.length, range.from + mark.length + body.length),
      };
    });
    view.dispatch(state.update(tr, { scrollIntoView: true, userEvent: 'input.format' }));
    view.focus();
    return true;
  };
}

function selectedLines(view: EditorView): number[] {
  const out = new Set<number>();
  for (const r of view.state.selection.ranges) {
    const a = view.state.doc.lineAt(r.from).number;
    const b = view.state.doc.lineAt(r.to).number;
    for (let l = a; l <= b; l++) out.add(l);
  }
  return [...out].sort((x, y) => x - y);
}

/** Apply `fn` to each selected line's text. */
function mapLines(view: EditorView, fn: (text: string, index: number) => string, userEvent = 'input.format'): boolean {
  const { doc } = view.state;
  const changes: ChangeSpec[] = [];
  selectedLines(view).forEach((n, i) => {
    const line = doc.line(n);
    const next = fn(line.text, i);
    if (next !== line.text) changes.push({ from: line.from, to: line.to, insert: next });
  });
  if (changes.length) view.dispatch({ changes, userEvent });
  view.focus();
  return true;
}

const LIST_PREFIX = /^(\s*)(?:[-*+]\s+\[[ xX]\]\s+|[-*+]\s+|\d+[.)]\s+|>\s?|#{1,6}\s+)?/;

export function setHeading(level: number): Cmd {
  return (view) =>
    mapLines(view, (text) => {
      const m = /^(\s*)(#{1,6})\s+(.*)$/.exec(text);
      if (m && m[2].length === level) return m[1] + m[3];
      const body = text.replace(/^(\s*)#{1,6}\s+/, '$1');
      return level ? `${'#'.repeat(level)} ${body.trimStart()}` : body;
    });
}

export function toggleLinePrefix(kind: 'bullet' | 'ordered' | 'task' | 'quote'): Cmd {
  return (view) => {
    const lines = selectedLines(view).map((n) => view.state.doc.line(n).text);
    const has = (t: string) =>
      kind === 'bullet'
        ? /^\s*[-*+]\s+(?!\[[ xX]\])/.test(t)
        : kind === 'ordered'
          ? /^\s*\d+[.)]\s+/.test(t)
          : kind === 'task'
            ? /^\s*[-*+]\s+\[[ xX]\]/.test(t)
            : /^\s*>/.test(t);
    const allHave = lines.every(has);
    return mapLines(view, (text, i) => {
      const indent = /^\s*/.exec(text)![0];
      const body = text.replace(LIST_PREFIX, '');
      if (allHave) return indent + body;
      const prefix = kind === 'bullet' ? '- ' : kind === 'ordered' ? `${i + 1}. ` : kind === 'task' ? '- [ ] ' : '> ';
      return indent + prefix + body;
    });
  };
}

export const toggleTaskDone: Cmd = (view) =>
  mapLines(
    view,
    (text) => {
      const m = /^(\s*[-*+]\s+\[)([ xX])(\].*)$/.exec(text);
      if (m) return m[1] + (m[2] === ' ' ? 'x' : ' ') + m[3];
      const indent = /^\s*/.exec(text)![0];
      return `${indent}- [ ] ${text.replace(LIST_PREFIX, '')}`;
    },
    'input.toggle',
  );

export function insertText(text: string, cursorOffset?: number): Cmd {
  return (view) => {
    const r = view.state.selection.main;
    view.dispatch({
      changes: { from: r.from, to: r.to, insert: text },
      selection: { anchor: r.from + (cursorOffset ?? text.length) },
      scrollIntoView: true,
      userEvent: 'input',
    });
    view.focus();
    return true;
  };
}

/** Insert a block on its own line(s). */
export function insertBlock(text: string, cursorOffset?: number): Cmd {
  return (view) => {
    const r = view.state.selection.main;
    const line = view.state.doc.lineAt(r.from);
    const needsNlBefore = line.text.trim() !== '' ? '\n\n' : '';
    const insert = needsNlBefore + text + '\n';
    const from = line.text.trim() !== '' ? line.to : line.from;
    const to = line.text.trim() !== '' ? line.to : line.to;
    view.dispatch({
      changes: { from, to, insert },
      selection: { anchor: from + needsNlBefore.length + (cursorOffset ?? text.length) },
      scrollIntoView: true,
      userEvent: 'input',
    });
    view.focus();
    return true;
  };
}

export function tableMarkdown(cols: number, rows: number, header = true): string {
  const cells = (fill: (i: number) => string) => `| ${Array.from({ length: cols }, (_, i) => fill(i)).join(' | ')} |`;
  const lines = [header ? cells((i) => `Column ${i + 1}`) : cells(() => '   '), cells(() => '---')];
  for (let r = 0; r < rows; r++) lines.push(cells(() => '   '));
  return lines.join('\n');
}

/** Find the table (contiguous `|` lines) around the cursor. */
function tableAt(view: EditorView): { from: number; to: number; first: number; last: number } | null {
  const doc = view.state.doc;
  const cur = doc.lineAt(view.state.selection.main.head).number;
  const isRow = (n: number) => n >= 1 && n <= doc.lines && /^\s*\|.*\|\s*$/.test(doc.line(n).text);
  if (!isRow(cur)) return null;
  let a = cur;
  let b = cur;
  while (isRow(a - 1)) a--;
  while (isRow(b + 1)) b++;
  return { from: doc.line(a).from, to: doc.line(b).to, first: a, last: b };
}

function splitRow(line: string): string[] {
  return line
    .trim()
    .replace(/^\||\|$/g, '')
    .split(/(?<!\\)\|/)
    .map((c) => c.trim());
}

export const formatTable: Cmd = (view) => {
  const tb = tableAt(view);
  if (!tb) return false;
  const doc = view.state.doc;
  const rows: string[][] = [];
  for (let n = tb.first; n <= tb.last; n++) rows.push(splitRow(doc.line(n).text));
  const cols = Math.max(...rows.map((r) => r.length));
  const width = Array.from({ length: cols }, (_, c) => Math.max(3, ...rows.map((r, i) => (i === 1 ? 3 : (r[c] ?? '').length))));
  const text = rows
    .map(
      (r, i) =>
        '| ' +
        Array.from({ length: cols }, (_, c) => {
          const v = r[c] ?? '';
          if (i === 1) {
            const left = v.startsWith(':');
            const right = v.endsWith(':');
            return (left ? ':' : '-') + '-'.repeat(width[c] - 2) + (right ? ':' : '-');
          }
          return v.padEnd(width[c]);
        }).join(' | ') +
        ' |',
    )
    .join('\n');
  view.dispatch({ changes: { from: tb.from, to: tb.to, insert: text }, userEvent: 'input.format' });
  return true;
};

export const addTableRow: Cmd = (view) => {
  const tb = tableAt(view);
  if (!tb) return false;
  const line = view.state.doc.lineAt(view.state.selection.main.head);
  const cols = splitRow(line.text).length;
  const row = `\n| ${Array.from({ length: cols }, () => '   ').join(' | ')} |`;
  view.dispatch({ changes: { from: line.to, insert: row }, selection: { anchor: line.to + 3 }, userEvent: 'input' });
  return true;
};

export const addTableColumn: Cmd = (view) => {
  const tb = tableAt(view);
  if (!tb) return false;
  const doc = view.state.doc;
  const changes: ChangeSpec[] = [];
  for (let n = tb.first; n <= tb.last; n++) {
    const l = doc.line(n);
    const end = l.text.lastIndexOf('|');
    changes.push({ from: l.from + end, insert: n === tb.first + 1 ? '| --- ' : '|     ' });
  }
  view.dispatch({ changes, userEvent: 'input' });
  return formatTable(view);
};

export function transformSelection(fn: (s: string) => string): Cmd {
  return (view) => {
    const { state } = view;
    const tr = state.changeByRange((range: SelectionRange) => {
      let r = range;
      if (r.empty) {
        const w = state.wordAt(r.head);
        if (!w) return { range: r };
        r = EditorSelection.range(w.from, w.to);
      }
      const out = fn(state.sliceDoc(r.from, r.to));
      return { changes: { from: r.from, to: r.to, insert: out }, range: EditorSelection.range(r.from, r.from + out.length) };
    });
    view.dispatch(state.update(tr, { userEvent: 'input.transform' }));
    view.focus();
    return true;
  };
}

export const toUpper = transformSelection((s) => s.toLocaleUpperCase());
export const toLower = transformSelection((s) => s.toLocaleLowerCase());
export const toTitle = transformSelection((s) => s.toLocaleLowerCase().replace(/(^|[\s\-_(])(\p{L})/gu, (_, a, b) => a + b.toLocaleUpperCase()));
export const toSentence = transformSelection((s) => s.toLocaleLowerCase().replace(/(^\s*|[.!?]\s+)(\p{L})/gu, (_, a, b) => a + b.toLocaleUpperCase()));
export const toggleCase = transformSelection((s) => (s === s.toLocaleUpperCase() ? s.toLocaleLowerCase() : s.toLocaleUpperCase()));

function replaceSelectedLines(view: EditorView, fn: (lines: string[]) => string[]): boolean {
  const doc = view.state.doc;
  const r = view.state.selection.main;
  let a = doc.lineAt(r.from).number;
  let b = doc.lineAt(r.to).number;
  if (a === b) {
    a = 1;
    b = doc.lines;
  }
  const from = doc.line(a).from;
  const to = doc.line(b).to;
  const lines = view.state.sliceDoc(from, to).split('\n');
  const next = fn(lines).join('\n');
  view.dispatch({ changes: { from, to, insert: next }, selection: { anchor: from, head: from + next.length }, userEvent: 'input' });
  view.focus();
  return true;
}

export const sortLinesAsc: Cmd = (v) =>
  replaceSelectedLines(v, (l) => [...l].sort((a, b) => a.localeCompare(b, undefined, { numeric: true, sensitivity: 'base' })));
export const sortLinesDesc: Cmd = (v) =>
  replaceSelectedLines(v, (l) => [...l].sort((a, b) => b.localeCompare(a, undefined, { numeric: true, sensitivity: 'base' })));
export const dedupeLines: Cmd = (v) => replaceSelectedLines(v, (l) => l.filter((x, i) => x.trim() === '' || l.indexOf(x) === i));
export const trimTrailing: Cmd = (v) => replaceSelectedLines(v, (l) => l.map((x) => x.replace(/[ \t]+$/, '')));
export const joinLines: Cmd = (view) => {
  const doc = view.state.doc;
  const r = view.state.selection.main;
  const a = doc.lineAt(r.from);
  const b = r.empty ? (a.number < doc.lines ? doc.line(a.number + 1) : a) : doc.lineAt(r.to);
  if (a.number === b.number) return false;
  const text = view.state
    .sliceDoc(a.from, b.to)
    .split('\n')
    .map((x, i) => (i ? x.trim() : x.trimEnd()))
    .join(' ');
  view.dispatch({ changes: { from: a.from, to: b.to, insert: text }, userEvent: 'input' });
  return true;
};

export const insertDate: Cmd = (view) => {
  const d = new Date();
  const iso = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
  return insertText(`[${iso}]`)(view);
};

export const insertDateTime: Cmd = (view) => {
  const d = new Date();
  const iso = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')} ${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;
  return insertText(`[${iso}]`)(view);
};

export function insertLink(url: string, text?: string): Cmd {
  return (view) => {
    const r = view.state.selection.main;
    const label = text ?? (view.state.sliceDoc(r.from, r.to) || url);
    const md = `[${label}](${url})`;
    view.dispatch({ changes: { from: r.from, to: r.to, insert: md }, selection: { anchor: r.from + md.length }, userEvent: 'input' });
    view.focus();
    return true;
  };
}

export function insertCardLink(id: string, heading?: string | null, embed = false): Cmd {
  return insertText(`${embed ? '!' : ''}[[${id}${heading ? `#${heading}` : ''}]]`);
}
