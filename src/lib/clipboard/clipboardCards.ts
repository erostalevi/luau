// "Create card from clipboard" — pure rules (no UI, no RPC): clipboard text
// clean-up, the "as is" card, where new cards go, the batch op and the
// Markdown inserted into an open document. AI drafts are validated by the
// core (`luau_core::ai::cards`); `acceptAiCards` re-checks shape and caps
// because the RPC result is still untrusted input for the UI.

import type { Op, Parent } from '$lib/backend/types';

/** Clipboard text accepted at all (same cap as the core). */
export const MAX_CLIPBOARD_BYTES = 256 * 1024;
/** Cards created from one paste (same cap as the core). */
export const MAX_CARDS = 12;
const MAX_CARD_CHARS = 64 * 1024;

/** LF line ends, no control characters except tab / newline, no trailing blanks. */
export function cleanText(s: string): string {
  return (
    s
      .replace(/\r\n?/g, '\n')
      // eslint-disable-next-line no-control-regex
      .replace(/[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/g, '')
      .replace(/\s+$/, '')
  );
}

export function tooLong(s: string): boolean {
  return new TextEncoder().encode(s).length > MAX_CLIPBOARD_BYTES;
}

const HEADING = /^#{1,6}\s+\S/;
/** A leading list / task marker, dropped from the title line. */
const MARKER = /^\s*(?:[-*+•]|\d{1,3}[.)])\s+(?:\[[ xX]\]\s+)?/;

function splitFirst(text: string): { first: string; rest: string } | null {
  const lines = cleanText(text).split('\n');
  const i = lines.findIndex((l) => l.trim() !== '');
  if (i < 0) return null;
  const rest = lines
    .slice(i + 1)
    .join('\n')
    .replace(/^\n+/, '');
  return { first: lines[i].trim(), rest };
}

function withHeading(text: string, level: '#' | '##'): string | null {
  const p = splitFirst(text);
  if (!p) return null;
  let title: string;
  if (HEADING.test(p.first)) title = level === '#' ? p.first : p.first.replace(/^#{1,6}/, '##');
  else title = `${level} ${p.first.replace(MARKER, '').trim() || p.first}`;
  return p.rest ? `${title}\n\n${p.rest}\n` : `${title}\n`;
}

/** "As is": the first non-empty line becomes `# Title` (unless it already is a heading); the rest stays verbatim. */
export function asIsCard(text: string): string | null {
  return withHeading(text, '#');
}

/** "As is" for an open document: the same, as a `##` section. */
export function asIsSection(text: string): string | null {
  return withHeading(text, '##');
}

/** Block inserted at the cursor: blank lines around it unless at the start / end of the document. */
export function blockForInsertion(sections: string[], before: string, after: string): string {
  const body = sections
    .map((s) => s.trim())
    .filter(Boolean)
    .join('\n\n');
  if (!body) return '';
  const lead = before === '' || before.endsWith('\n\n') ? '' : before.endsWith('\n') ? '\n' : '\n\n';
  const trail = after === '' ? '\n' : after.startsWith('\n\n') ? '' : after.startsWith('\n') ? '\n' : '\n\n';
  return lead + body + trail;
}

// --- insertion target -------------------------------------------------------

export interface BoardLike {
  id: string;
  kind: string;
  lanes: { id: string; archived: boolean }[];
  node(id: string): { parent: Parent } | undefined;
  childrenOf(p: Parent): string[];
}

export interface TargetInput {
  board: BoardLike | null;
  /** The document editor of this board's active doc tab (only for files boards). */
  docEditor: boolean;
  /** Selected / focused card on this board (or the card open in the editor). */
  focus: string | null;
  /** Focused lane on this board. */
  lane: string | null;
}

export type InsertTarget = { kind: 'doc' } | { kind: 'cards'; parent: Parent; before: string | null };

/**
 * Where the new card(s) go:
 * - files board with its document editor active → Markdown at the cursor;
 * - kanban → the lane of the selected card (right after it, at lane level),
 *   else the focused lane, else the first open lane (end of it);
 * - files board without an editor → after the selected document, else at the end.
 * `null` when there is nowhere to put a card (kanban without open lanes).
 */
export function insertionTarget(i: TargetInput): InsertTarget | null {
  const b = i.board;
  if (!b) return null;
  if (b.kind === 'files' && i.docEditor) return { kind: 'doc' };
  const after = (id: string): InsertTarget | null => {
    let cur = id;
    let n = b.node(cur);
    // Kanban: climb from a sub-card to its lane-level card.
    while (b.kind !== 'files' && n && n.parent.kind === 'card') {
      cur = n.parent.id;
      n = b.node(cur);
    }
    if (!n) return null;
    if (b.kind !== 'files' && n.parent.kind !== 'lane') return null;
    const sibs = b.childrenOf(n.parent);
    return { kind: 'cards', parent: n.parent, before: sibs[sibs.indexOf(cur) + 1] ?? null };
  };
  if (i.focus) {
    const t = after(i.focus);
    if (t) return t;
  }
  if (b.kind === 'files') return { kind: 'cards', parent: { kind: 'root' }, before: null };
  const lane = (i.lane && b.lanes.find((l) => l.id === i.lane && !l.archived)) || b.lanes.find((l) => !l.archived);
  return lane ? { kind: 'cards', parent: { kind: 'lane', id: lane.id }, before: null } : null;
}

/** One `batch` op creating the cards in order at the target (one undo step). */
export function createCardsOp(ids: string[], contents: string[], parent: Parent, siblings: string[], before: string | null): Op {
  const start = before ? siblings.indexOf(before) : -1;
  const ops: Op[] = contents.map((content, k) => ({
    op: 'createCard',
    id: ids[k],
    parent,
    index: start >= 0 ? start + k : null,
    content,
  }));
  return ops.length === 1 ? ops[0] : { op: 'batch', ops };
}

// --- AI result ----------------------------------------------------------------

export interface AcceptedCard {
  title: string;
  markdown: string;
  section: string;
}

/** Re-check the RPC answer: shape, caps, and a `#` title line on every card. */
export function acceptAiCards(raw: unknown): AcceptedCard[] {
  const list = (raw as { cards?: unknown } | null)?.cards;
  if (!Array.isArray(list)) return [];
  const out: AcceptedCard[] = [];
  for (const c of list) {
    if (out.length >= MAX_CARDS) break;
    const { title, markdown, section } = (c ?? {}) as Record<string, unknown>;
    if (typeof title !== 'string' || typeof markdown !== 'string' || typeof section !== 'string') continue;
    const md = cleanText(markdown).slice(0, MAX_CARD_CHARS);
    const sec = cleanText(section).slice(0, MAX_CARD_CHARS);
    const name = title.replace(/\s+/g, ' ').trim().slice(0, 200);
    if (!name || !/^# \S/.test(md) || !/^## \S/.test(sec)) continue;
    out.push({ title: name, markdown: md + '\n', section: sec + '\n' });
  }
  return out;
}

/** Short preview for the confirmation dialog. */
export function previewText(cards: AcceptedCard[], max = 8): string {
  const lines = cards.slice(0, max).map((c) => `• ${c.title}`);
  if (cards.length > max) lines.push(`… +${cards.length - max}`);
  return lines.join('\n');
}
