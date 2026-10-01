// Lightweight card metadata parser (TS twin of luau-core/src/markdown).
// Used for optimistic UI and the browser mock; the Rust parser is authoritative.

import type { Face, FaceItem, Footer, Heading, LinkRef, TaskStats } from '$lib/backend/types';

const TAG_RE = /(^|[\s([,;])#([\p{L}\p{N}_/-]+)/gmu;
const MENTION_RE = /(^|[\s([,;])@([\p{L}\p{N}][\p{L}\p{N}_.-]*)/gmu;
const LINK_RE = /(!?)\[\[([^[\]\n]+?)\]\]/g;
const DATE_RE = /\[(\d{4}-\d{2}-\d{2})(?:[ T](\d{1,2}:\d{2}))?\]/g;
const CARD_ID_RE = /^c[a-z0-9]{6}$/;
const TASK_RE = /^(\s*)([-*+]|\d+[.)])\s+\[([ xX])\]\s*(.*)$/;
const LIST_RE = /^(\s*)([-*+]|\d+[.)])\s+(.*)$/;

export interface ParsedMeta {
  title: string;
  hasTitleLine: boolean;
  tags: string[];
  links: LinkRef[];
  mentions: string[];
  dates: string[];
  footer: Footer;
  tasks: TaskStats;
  face: Face;
  headings: Heading[];
  wordCount: number;
  hasCode: boolean;
}

export function isCardId(s: string): boolean {
  return CARD_ID_RE.test(s);
}

export function titleLine(first: string): string | null {
  const t = first.trimStart();
  if (t === '#') return '';
  if (t.startsWith('# ')) return t.slice(2).trim().replace(/#+$/, '').trim();
  return null;
}

export function emptyFooter(): Footer {
  return { startLine: null, fields: [], priority: null, due: null, start: null, assignees: [], labels: [] };
}

const FIELD_RE = /^([A-Za-z][\w -]{0,31}):\s*(.*)$/;

export function parseFooter(lines: string[]): Footer {
  let i = lines.length;
  let sawField = false;
  while (i > 0) {
    const line = lines[i - 1].trimEnd();
    if (line.trim() === '') {
      i--;
      continue;
    }
    if (line.trim() === '---') {
      const precededOk = i < 2 || lines[i - 2].trim() === '';
      if (sawField && precededOk) return buildFooter(lines, i - 1);
      return emptyFooter();
    }
    if (FIELD_RE.test(line)) {
      sawField = true;
      i--;
      continue;
    }
    return emptyFooter();
  }
  return emptyFooter();
}

function splitList(v: string): string[] {
  return v
    .split(',')
    .map((s) => s.trim().replace(/^[@#]/, '').trim())
    .filter(Boolean);
}

function buildFooter(lines: string[], sep: number): Footer {
  const f = emptyFooter();
  f.startLine = sep;
  for (const line of lines.slice(sep + 1)) {
    const m = FIELD_RE.exec(line);
    if (!m) continue;
    const key = m[1].trim().toLowerCase();
    const v = m[2].trim();
    if (key === 'priority') f.priority = v ? v.toLowerCase() : null;
    else if (key === 'due') f.due = v || null;
    else if (key === 'start') f.start = v || null;
    else if (key === 'assignees' || key === 'assignee' || key === 'owners') f.assignees = splitList(v);
    else if (key === 'labels' || key === 'label') f.labels = splitList(v);
    f.fields.push([key, v]);
  }
  return f;
}

/** Replace/insert/remove the property footer. */
export function setFooterFields(content: string, fields: [string, string][]): string {
  const lines = content.replace(/\r\n/g, '\n').split('\n');
  const cur = parseFooter(lines);
  const end = cur.startLine ?? lines.length;
  let body = lines.slice(0, end).join('\n').replace(/[\n ]+$/, '');
  const nonEmpty = fields.filter(([, v]) => v.trim() !== '');
  if (!nonEmpty.length) return body + '\n';
  body += '\n\n---\n';
  for (const [k, v] of nonEmpty) body += `${k}: ${v.trim()}\n`;
  return body;
}

export function setTitle(content: string, title: string): string {
  const t = title.replace(/[\r\n]+/g, ' ').trim();
  const nl = content.indexOf('\n');
  const first = nl < 0 ? content : content.slice(0, nl);
  const rest = nl < 0 ? '' : content.slice(nl + 1);
  if (titleLine(first) !== null) return `# ${t}\n${rest}`;
  return `# ${t}\n\n${content}`;
}

export function toggleTaskLine(content: string, line: number): string | null {
  const lines = content.split('\n');
  const l = lines[line];
  if (l === undefined) return null;
  const m = /^(\s*(?:[-*+]|\d+[.)])\s+\[)([ xX])(\].*)$/.exec(l);
  if (!m) return null;
  lines[line] = m[1] + (m[2] === ' ' ? 'x' : ' ') + m[3];
  return lines.join('\n');
}

/** Code ranges (fenced blocks + inline code) to skip when scanning tokens. */
function codeMask(src: string): [number, number][] {
  const out: [number, number][] = [];
  const fence = /^(\s*)(```|~~~)[^\n]*\n[\s\S]*?(^\1\2[^\n]*$|(?![\s\S]))/gm;
  let m: RegExpExecArray | null;
  while ((m = fence.exec(src))) out.push([m.index, m.index + m[0].length]);
  const inline = /`[^`\n]+`/g;
  while ((m = inline.exec(src))) out.push([m.index, m.index + m[0].length]);
  const link = /\]\([^)\n]*\)/g;
  while ((m = link.exec(src))) out.push([m.index, m.index + m[0].length]);
  const math = /\$[^$\n]+\$/g;
  while ((m = math.exec(src))) out.push([m.index, m.index + m[0].length]);
  return out;
}

const inMask = (pos: number, mask: [number, number][]) => mask.some(([a, b]) => pos >= a && pos < b);

function pushUnique(arr: string[], v: string) {
  if (!arr.some((x) => x.toLowerCase() === v.toLowerCase())) arr.push(v);
}

function stripInline(s: string): string {
  return s
    .replace(/!\[[^\]]*\]\([^)]*\)/g, '')
    .replace(/\[([^\]]*)\]\([^)]*\)/g, '$1')
    .replace(/[*_~`]+/g, '')
    .trim();
}

export function parseMeta(content: string): ParsedMeta {
  const src = content.replace(/^\uFEFF/, '').replace(/\r\n/g, '\n');
  const lines = src.split('\n');
  const footer = parseFooter(lines);
  const bodyLines = lines.slice(0, footer.startLine ?? lines.length);
  const body = bodyLines.join('\n');
  const tl = titleLine(lines[0] ?? '');
  const out: ParsedMeta = {
    title: tl ?? '',
    hasTitleLine: tl !== null,
    tags: [],
    links: [],
    mentions: [],
    dates: [],
    footer,
    tasks: { total: 0, done: 0 },
    face: { kind: 'none' },
    headings: [],
    wordCount: 0,
    hasCode: /^\s*(```|~~~)/m.test(body),
  };
  const mask = codeMask(body);
  const lineStarts = [0];
  for (let i = 0; i < body.length; i++) if (body[i] === '\n') lineStarts.push(i + 1);
  const lineOf = (pos: number) => {
    let lo = 0;
    let hi = lineStarts.length - 1;
    while (lo < hi) {
      const mid = (lo + hi + 1) >> 1;
      if (lineStarts[mid] <= pos) lo = mid;
      else hi = mid - 1;
    }
    return lo;
  };
  let m: RegExpExecArray | null;
  LINK_RE.lastIndex = 0;
  while ((m = LINK_RE.exec(body))) {
    if (inMask(m.index, mask)) continue;
    const inner = m[2].trim();
    const [targetPart, alias] = inner.split('|');
    const [target, heading] = targetPart.split('#');
    const t = target.trim();
    if (!t) continue;
    out.links.push({
      id: isCardId(t) ? t : null,
      target: t,
      heading: heading?.trim() ?? null,
      alias: alias?.trim() ?? null,
      embed: m[1] === '!',
      line: lineOf(m.index),
    });
    mask.push([m.index, m.index + m[0].length]);
  }
  TAG_RE.lastIndex = 0;
  while ((m = TAG_RE.exec(body))) {
    const hashPos = m.index + m[1].length;
    if (inMask(hashPos, mask)) continue;
    const tag = m[2].replace(/[/-]+$/, '');
    if (!tag || tag.startsWith('/') || /^\d+$/.test(tag)) continue;
    pushUnique(out.tags, tag);
  }
  MENTION_RE.lastIndex = 0;
  while ((m = MENTION_RE.exec(body))) {
    if (inMask(m.index + m[1].length, mask)) continue;
    const name = m[2].replace(/[.-]+$/, '');
    if (name) pushUnique(out.mentions, name);
  }
  DATE_RE.lastIndex = 0;
  while ((m = DATE_RE.exec(body))) {
    if (inMask(m.index, mask)) continue;
    const before = body[m.index - 1];
    const after = body[m.index + m[0].length];
    if (before === '[' || after === '(' || after === '[' || after === ':' || after === ']') continue;
    if (!out.dates.includes(m[1]) && !Number.isNaN(Date.parse(m[1]))) out.dates.push(m[1]);
  }
  for (const a of footer.assignees) pushUnique(out.mentions, a);

  // Structure scan: headings, tasks, first list/table.
  let inFence = false;
  let face: Face | null = null;
  let listItems: FaceItem[] | null = null;
  let listTotal = 0;
  let listOrdered = false;
  let listIndent = -1;
  const paragraphs: string[] = [];
  let para = '';
  const flushPara = () => {
    if (para.trim()) paragraphs.push(para.trim());
    para = '';
  };
  const finishList = () => {
    if (listItems && listItems.length && !face) {
      face = listItems.some((i) => i.task !== null)
        ? { kind: 'checklist', items: listItems, total: listTotal }
        : { kind: 'list', items: listItems, total: listTotal, ordered: listOrdered };
    }
    listItems = null;
    listIndent = -1;
  };
  for (let i = 0; i < bodyLines.length; i++) {
    const line = bodyLines[i];
    if (/^\s*(```|~~~)/.test(line)) {
      inFence = !inFence;
      flushPara();
      continue;
    }
    if (inFence) continue;
    const h = /^(#{1,6})\s+(.*)$/.exec(line);
    if (h) {
      flushPara();
      finishList();
      out.headings.push({ level: h[1].length, text: h[2].trim(), line: i });
      if (!out.title && h[1].length === 1 && !out.hasTitleLine) out.title = h[2].trim();
      continue;
    }
    const task = TASK_RE.exec(line);
    const li = task ? null : LIST_RE.exec(line);
    if (task || li) {
      flushPara();
      const indent = (task ?? li)![1].length;
      if (task) {
        out.tasks.total++;
        if (task[3] !== ' ') out.tasks.done++;
      }
      if (!face && (listItems === null || indent <= listIndent || listIndent < 0)) {
        if (listItems === null) {
          listItems = [];
          listTotal = 0;
          listIndent = indent;
          listOrdered = /\d/.test((task ?? li)![2]);
        }
        if (indent === listIndent) {
          listTotal++;
          if (listItems.length < 8)
            listItems.push({
              text: stripInline(task ? task[4] : li![3]),
              task: task ? task[3] !== ' ' : null,
              line: i,
            });
        }
      }
      continue;
    }
    if (/^\s*\|.*\|\s*$/.test(line) && !face && listItems === null) {
      flushPara();
      const rows: string[][] = [];
      let j = i;
      while (j < bodyLines.length && /^\s*\|.*\|\s*$/.test(bodyLines[j])) {
        const cells = bodyLines[j].trim().slice(1, -1).split('|').map((c) => stripInline(c.trim()));
        if (!cells.every((c) => /^:?-{2,}:?$/.test(c))) rows.push(cells);
        j++;
      }
      if (rows.length >= 1) face = { kind: 'table', header: rows[0], rows: rows.slice(1, 6), total: rows.length - 1 };
      i = j - 1;
      continue;
    }
    if (line.trim() === '') {
      flushPara();
      if (listItems !== null) finishList();
      continue;
    }
    if (listItems !== null && /^\s+/.test(line)) continue;
    finishList();
    if (i === 0 && out.hasTitleLine) continue;
    para += ' ' + stripInline(line);
  }
  flushPara();
  finishList();
  if (!face) {
    const text = paragraphs.join(' ');
    const sentences = text.split(/(?<=[.!?])\s+/).filter((s) => s.length > 2);
    const summary = sentences.slice(0, 2).join(' ');
    face = summary ? { kind: 'summary', text: summary.length > 220 ? summary.slice(0, 219) + '…' : summary } : { kind: 'none' };
  }
  out.face = face;
  out.wordCount = body.split(/\s+/).filter(Boolean).length;
  return out;
}

/** Stable pastel hue for a tag (matches the palette used by chips). */
export function tagHue(tag: string): number {
  let h = 2166136261;
  for (const ch of tag.toLowerCase()) h = Math.imul(h ^ ch.codePointAt(0)!, 16777619);
  return Math.abs(h) % 360;
}
