// Line + word diff (Myers O(ND)) for the history diff view, markdown-aware.
// Pure, no deps, no Svelte: unit-tested in diff.test.ts.

export type DiffKind = "eq" | "add" | "del";

export interface DiffLine {
  kind: DiffKind;
  text: string;
  /** 1-based line number in the old text (eq/del). */
  a?: number;
  /** 1-based line number in the new text (eq/add). */
  b?: number;
}

export interface Hunk {
  /** 1-based start lines and lengths, like `@@ -a,al +b,bl @@`. */
  aStart: number;
  aLen: number;
  bStart: number;
  bLen: number;
  lines: DiffLine[];
}

export interface SideRow<T extends DiffLine = DiffLine> {
  left: T | null;
  right: T | null;
}

export interface DiffStats {
  added: number;
  removed: number;
}

/** Split text into lines; a trailing newline does not produce an empty last line. */
export function splitLines(s: string): string[] {
  if (!s) return [];
  const t = s.replace(/\r\n?/g, "\n");
  const lines = t.split("\n");
  if (lines.length && lines[lines.length - 1] === "") lines.pop();
  return lines;
}

/** Above this edit distance we give up and show a full replace (keeps the UI snappy). */
const MAX_D = 1200;

/**
 * Line diff of `a` → `b`. Common prefix/suffix are trimmed first, then Myers'
 * greedy algorithm finds a shortest edit script for the middle.
 */
export function diffLines(aText: string, bText: string): DiffLine[] {
  const a = splitLines(aText);
  const b = splitLines(bText);
  let pre = 0;
  while (pre < a.length && pre < b.length && a[pre] === b[pre]) pre++;
  let suf = 0;
  while (
    suf < a.length - pre &&
    suf < b.length - pre &&
    a[a.length - 1 - suf] === b[b.length - 1 - suf]
  )
    suf++;
  const am = a.slice(pre, a.length - suf);
  const bm = b.slice(pre, b.length - suf);

  const out: DiffLine[] = [];
  for (let i = 0; i < pre; i++)
    out.push({ kind: "eq", text: a[i], a: i + 1, b: i + 1 });
  for (const op of myers(am, bm)) {
    if (op.kind === "eq")
      out.push({
        kind: "eq",
        text: am[op.i],
        a: pre + op.i + 1,
        b: pre + op.j + 1,
      });
    else if (op.kind === "del")
      out.push({ kind: "del", text: am[op.i], a: pre + op.i + 1 });
    else out.push({ kind: "add", text: bm[op.j], b: pre + op.j + 1 });
  }
  for (let k = 0; k < suf; k++) {
    const i = a.length - suf + k;
    const j = b.length - suf + k;
    out.push({ kind: "eq", text: a[i], a: i + 1, b: j + 1 });
  }
  return out;
}

type Op = { kind: DiffKind; i: number; j: number };

function myers(a: string[], b: string[]): Op[] {
  const n = a.length;
  const m = b.length;
  if (n === 0) return b.map((_, j) => ({ kind: "add" as const, i: 0, j }));
  if (m === 0) return a.map((_, i) => ({ kind: "del" as const, i, j: 0 }));
  const max = Math.min(n + m, MAX_D);
  const off = max + 1;
  let v = new Int32Array(2 * max + 3);
  const trace: Int32Array[] = [];
  let found = false;
  for (let d = 0; d <= max && !found; d++) {
    trace.push(v.slice());
    const nv = v.slice();
    for (let k = -d; k <= d; k += 2) {
      let x: number;
      if (k === -d || (k !== d && v[off + k - 1] < v[off + k + 1]))
        x = v[off + k + 1];
      else x = v[off + k - 1] + 1;
      let y = x - k;
      while (x < n && y < m && a[x] === b[y]) {
        x++;
        y++;
      }
      nv[off + k] = x;
      if (x >= n && y >= m) {
        found = true;
        break;
      }
    }
    v = nv;
  }
  if (!found) {
    // Too different: delete everything, then add everything.
    return [
      ...a.map((_, i) => ({ kind: "del" as const, i, j: 0 })),
      ...b.map((_, j) => ({ kind: "add" as const, i: 0, j })),
    ];
  }
  trace.push(v);
  // Backtrack.
  const ops: Op[] = [];
  let x = n;
  let y = m;
  for (let d = trace.length - 2; d >= 0; d--) {
    const pv = trace[d];
    const k = x - y;
    let prevK: number;
    if (k === -d || (k !== d && pv[off + k - 1] < pv[off + k + 1]))
      prevK = k + 1;
    else prevK = k - 1;
    const prevX = d === 0 ? 0 : pv[off + prevK];
    const prevY = d === 0 ? 0 : prevX - prevK;
    while (x > prevX && y > prevY) {
      x--;
      y--;
      ops.push({ kind: "eq", i: x, j: y });
    }
    if (d === 0) break;
    if (x === prevX) {
      y--;
      ops.push({ kind: "add", i: x, j: y });
    } else {
      x--;
      ops.push({ kind: "del", i: x, j: y });
    }
  }
  while (x > 0 && y > 0) {
    x--;
    y--;
    ops.push({ kind: "eq", i: x, j: y });
  }
  ops.reverse();
  // Show deletions before additions inside each changed block (reads naturally).
  return normalizeBlocks(ops);
}

function normalizeBlocks(ops: Op[]): Op[] {
  const out: Op[] = [];
  let dels: Op[] = [];
  let adds: Op[] = [];
  const flush = () => {
    out.push(...dels, ...adds);
    dels = [];
    adds = [];
  };
  for (const op of ops) {
    if (op.kind === "eq") {
      flush();
      out.push(op);
    } else if (op.kind === "del") dels.push(op);
    else adds.push(op);
  }
  flush();
  return out;
}

export function diffStats(lines: DiffLine[]): DiffStats {
  let added = 0;
  let removed = 0;
  for (const l of lines) {
    if (l.kind === "add") added++;
    else if (l.kind === "del") removed++;
  }
  return { added, removed };
}

/** Group a diff into hunks with `context` unchanged lines around each change. */
export function toHunks(lines: DiffLine[], context = 3): Hunk[] {
  const changed = lines
    .map((l, i) => (l.kind === "eq" ? -1 : i))
    .filter((i) => i >= 0);
  if (!changed.length) return [];
  const ranges: [number, number][] = [];
  for (const i of changed) {
    const s = Math.max(0, i - context);
    const e = Math.min(lines.length - 1, i + context);
    const last = ranges[ranges.length - 1];
    if (last && s <= last[1] + 1) last[1] = Math.max(last[1], e);
    else ranges.push([s, e]);
  }
  return ranges.map(([s, e]) => {
    const hl = lines.slice(s, e + 1);
    // Line numbers before the hunk start.
    let aBefore = 0;
    let bBefore = 0;
    for (let i = 0; i < s; i++) {
      if (lines[i].kind !== "add") aBefore++;
      if (lines[i].kind !== "del") bBefore++;
    }
    const aLen = hl.filter((l) => l.kind !== "add").length;
    const bLen = hl.filter((l) => l.kind !== "del").length;
    return {
      aStart: aLen ? aBefore + 1 : aBefore,
      aLen,
      bStart: bLen ? bBefore + 1 : bBefore,
      bLen,
      lines: hl,
    };
  });
}

/** Pair deletions with additions for a two-pane (split) view. */
export function sideBySide<T extends DiffLine>(lines: T[]): SideRow<T>[] {
  const rows: SideRow<T>[] = [];
  let i = 0;
  while (i < lines.length) {
    const l = lines[i];
    if (l.kind === "eq") {
      rows.push({ left: l, right: l });
      i++;
      continue;
    }
    const dels: T[] = [];
    const adds: T[] = [];
    while (i < lines.length && lines[i].kind === "del") dels.push(lines[i++]);
    while (i < lines.length && lines[i].kind === "add") adds.push(lines[i++]);
    const n = Math.max(dels.length, adds.length);
    for (let k = 0; k < n; k++)
      rows.push({ left: dels[k] ?? null, right: adds[k] ?? null });
  }
  return rows;
}

/** Unified diff text (for "Copy diff"). */
export function unifiedText(lines: DiffLine[], context = 3): string {
  return toHunks(lines, context)
    .map((h) => {
      const head = `@@ -${h.aStart},${h.aLen} +${h.bStart},${h.bLen} @@`;
      const body = h.lines.map(
        (l) => (l.kind === "add" ? "+" : l.kind === "del" ? "-" : " ") + l.text,
      );
      return [head, ...body].join("\n");
    })
    .join("\n");
}

// --- word level ---------------------------------------------------------------

export interface Seg {
  kind: DiffKind;
  text: string;
}

/**
 * Markdown-aware tokenizer: wiki links, inline links, inline code, URLs, tags,
 * mentions and emphasis markers stay atomic so a highlight never splits them.
 */
const TOKEN_RE =
  /\[\[[^\]\n]*\]\]|!?\[[^\]\n]*\]\([^)\s]*\)|`[^`\n]*`|https?:\/\/[^\s)]+|[#@][\p{L}\p{N}_\-/]+|\*\*|__|~~|[\p{L}\p{N}_'’]+|\s+|./gu;

export function tokenize(line: string): string[] {
  return line.match(TOKEN_RE) ?? [];
}

/** Myers over arbitrary token lists (used for word diffs). */
export function diffTokens(a: string[], b: string[]): Seg[] {
  return myers(a, b).map((op) => ({
    kind: op.kind,
    text: op.kind === "add" ? b[op.j] : a[op.i],
  }));
}

function merge(segs: Seg[]): Seg[] {
  const out: Seg[] = [];
  for (const s of segs) {
    const last = out[out.length - 1];
    if (last && last.kind === s.kind) last.text += s.text;
    else out.push({ ...s });
  }
  return out;
}

export interface WordDiff {
  /** Segments of the old line (`eq` + `del`). */
  left: Seg[];
  /** Segments of the new line (`eq` + `add`). */
  right: Seg[];
  /** Share of characters kept, 0..1. */
  similarity: number;
}

/** Word diff of one changed line pair. A lone space between two changes is folded into the change. */
export function diffWords(a: string, b: string): WordDiff {
  const ops = diffTokens(tokenize(a), tokenize(b));
  for (let i = 1; i < ops.length - 1; i++) {
    const o = ops[i];
    if (
      o.kind === "eq" &&
      /^\s+$/.test(o.text) &&
      ops[i - 1].kind !== "eq" &&
      ops[i + 1].kind !== "eq"
    ) {
      ops.splice(
        i,
        1,
        { kind: "del", text: o.text },
        { kind: "add", text: o.text },
      );
      i++;
    }
  }
  const left = merge(ops.filter((o) => o.kind !== "add"));
  const right = merge(ops.filter((o) => o.kind !== "del"));
  const kept = ops
    .filter((o) => o.kind === "eq")
    .reduce((n, o) => n + o.text.length, 0);
  const total = Math.max(a.length, b.length);
  return { left, right, similarity: total ? kept / total : 1 };
}

// --- markdown awareness --------------------------------------------------------

export type MdKind =
  | "heading"
  | "task"
  | "taskDone"
  | "list"
  | "quote"
  | "fence"
  | "code"
  | "rule"
  | "table"
  | "footer"
  | "text"
  | "blank";

export interface MdInfo {
  kind: MdKind;
  /** Heading level (1–6). */
  level?: number;
}

/** Classify every line of a document (fenced code blocks and the `---` footer are tracked). */
export function classifyLines(lines: string[]): MdInfo[] {
  let fence: string | null = null;
  let footer = false;
  return lines.map((l): MdInfo => {
    const tr = l.trim();
    const f = /^(`{3,}|~{3,})/.exec(tr);
    if (fence) {
      if (f && tr.startsWith(fence)) fence = null;
      return { kind: f && !fence ? "fence" : "code" };
    }
    if (f) {
      fence = f[1];
      return { kind: "fence" };
    }
    if (!tr) return { kind: "blank" };
    if (/^(-{3,}|\*{3,}|_{3,})$/.test(tr)) {
      footer = true;
      return { kind: "rule" };
    }
    if (footer && /^[\w.-]+:(\s|$)/.test(tr)) return { kind: "footer" };
    const h = /^(#{1,6})\s/.exec(tr);
    if (h) return { kind: "heading", level: h[1].length };
    if (/^[-*+]\s+\[[xX]\]\s/.test(tr)) return { kind: "taskDone" };
    if (/^[-*+]\s+\[ \]\s/.test(tr)) return { kind: "task" };
    if (/^([-*+]|\d+[.)])\s/.test(tr)) return { kind: "list" };
    if (tr.startsWith(">")) return { kind: "quote" };
    if (tr.startsWith("|")) return { kind: "table" };
    return { kind: "text" };
  });
}

export interface RichLine extends DiffLine {
  md: MdInfo;
  /** Word-level segments when the line is part of a similar del/add pair. */
  segs?: Seg[];
}

/** Below this similarity a changed pair is shown as a whole-line replace. */
export const WORD_DIFF_MIN_SIMILARITY = 0.35;

/**
 * Attach markdown info (from the side the line belongs to) and word-level
 * segments to paired deletions/additions inside each changed block.
 */
export function enrich(
  lines: DiffLine[],
  aText: string,
  bText: string,
): RichLine[] {
  const aInfo = classifyLines(splitLines(aText));
  const bInfo = classifyLines(splitLines(bText));
  const out: RichLine[] = lines.map((l) => ({
    ...l,
    md: (l.kind === "add" ? bInfo[(l.b ?? 1) - 1] : aInfo[(l.a ?? 1) - 1]) ?? {
      kind: "text",
    },
  }));
  let i = 0;
  while (i < out.length) {
    if (out[i].kind === "eq") {
      i++;
      continue;
    }
    const dels: RichLine[] = [];
    const adds: RichLine[] = [];
    while (i < out.length && out[i].kind === "del") dels.push(out[i++]);
    while (i < out.length && out[i].kind === "add") adds.push(out[i++]);
    const n = Math.min(dels.length, adds.length);
    for (let k = 0; k < n; k++) {
      const w = diffWords(dels[k].text, adds[k].text);
      if (w.similarity >= WORD_DIFF_MIN_SIMILARITY) {
        dels[k].segs = w.left;
        adds[k].segs = w.right;
      }
    }
  }
  return out;
}

/** A display row: a line, or a collapsed run of unchanged lines. */
export type Row<T> =
  | { type: "line"; line: T; key: number }
  | { type: "gap"; count: number; key: number };

/**
 * Keep `context` items around every changed item and collapse the rest into
 * gap rows. Works for inline lines and for side-by-side rows alike.
 */
export function collapse<T>(
  items: T[],
  changed: (x: T) => boolean,
  context = 3,
): Row<T>[] {
  const keep = new Array<boolean>(items.length).fill(false);
  let any = false;
  items.forEach((x, i) => {
    if (!changed(x)) return;
    any = true;
    for (
      let k = Math.max(0, i - context);
      k <= Math.min(items.length - 1, i + context);
      k++
    )
      keep[k] = true;
  });
  if (!any)
    return items.map((line, key) => ({ type: "line" as const, line, key }));
  const rows: Row<T>[] = [];
  let gap = 0;
  items.forEach((line, i) => {
    if (keep[i]) {
      if (gap) rows.push({ type: "gap", count: gap, key: i - 0.5 });
      gap = 0;
      rows.push({ type: "line", line, key: i });
    } else gap++;
  });
  if (gap) rows.push({ type: "gap", count: gap, key: items.length });
  return rows;
}
