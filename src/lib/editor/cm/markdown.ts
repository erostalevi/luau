// Lezer Markdown extensions for Lull syntax:
//   [[c1a2b3c]] / [[c1a2b3c#Heading|alias]] / ![[c1a2b3c]]   CardLink / CardEmbed
//   #tag  @person  [2026-10-03]  ==highlight==  $inline math$  $$block math$$

import type { MarkdownConfig, InlineContext, BlockContext, Line } from '@lezer/markdown';
import { tags as t } from '@lezer/highlight';

const HASH = 35; // #
const AT = 64; // @
const LBRACK = 91; // [
const RBRACK = 93; // ]
const BANG = 33; // !
const EQ = 61; // =
const DOLLAR = 36; // $

const isWordChar = (c: number) => (c >= 48 && c <= 57) || (c >= 65 && c <= 90) || (c >= 97 && c <= 122) || c === 95 || c === 45 || c === 47 || c > 127;
const isSpaceOrStart = (c: number) => c === -1 || c === 32 || c === 9 || c === 10 || c === 40 || c === 44 || c === 59 || c === LBRACK;

function charAt(cx: InlineContext, pos: number): number {
  return pos < cx.offset + cx.text.length && pos >= cx.offset ? cx.char(pos) : -1;
}

export const lullMarkdown: MarkdownConfig = {
  defineNodes: [
    { name: 'CardLink', style: t.link },
    { name: 'CardEmbed', style: t.link },
    { name: 'CardLinkMark', style: t.processingInstruction },
    { name: 'Tag', style: t.labelName },
    { name: 'Mention', style: t.special(t.labelName) },
    { name: 'DateChip', style: t.special(t.string) },
    { name: 'Highlight', style: t.special(t.emphasis) },
    { name: 'HighlightMark', style: t.processingInstruction },
    { name: 'InlineMath', style: t.special(t.string) },
    { name: 'BlockMath', block: true, style: t.special(t.string) },
  ],
  parseInline: [
    {
      name: 'CardLink',
      before: 'Link',
      parse(cx, next, pos) {
        let start = pos;
        let embed = false;
        if (next === BANG && charAt(cx, pos + 1) === LBRACK && charAt(cx, pos + 2) === LBRACK) {
          embed = true;
          pos += 1;
        } else if (!(next === LBRACK && charAt(cx, pos + 1) === LBRACK)) return -1;
        const end = cx.text.indexOf(']]', pos + 2 - cx.offset);
        if (end < 0) return -1;
        const close = end + cx.offset;
        const inner = cx.slice(pos + 2, close);
        if (!inner.trim() || inner.includes('[') || inner.includes('\n')) return -1;
        return cx.addElement(
          cx.elt(embed ? 'CardEmbed' : 'CardLink', start, close + 2, [cx.elt('CardLinkMark', start, pos + 2), cx.elt('CardLinkMark', close, close + 2)]),
        );
        void start;
      },
    },
    {
      name: 'DateChip',
      before: 'Link',
      parse(cx, next, pos) {
        if (next !== LBRACK || charAt(cx, pos - 1) === LBRACK) return -1;
        const m = /^\[(\d{4}-\d{2}-\d{2})(?:[ T]\d{1,2}:\d{2})?\]/.exec(cx.slice(pos, Math.min(cx.end, pos + 24)));
        if (!m) return -1;
        const after = charAt(cx, pos + m[0].length);
        if (after === 40 || after === LBRACK || after === 58 || after === RBRACK) return -1;
        return cx.addElement(cx.elt('DateChip', pos, pos + m[0].length));
      },
    },
    {
      name: 'Tag',
      parse(cx, next, pos) {
        if (next !== HASH || !isSpaceOrStart(charAt(cx, pos - 1))) return -1;
        let end = pos + 1;
        while (end < cx.end && isWordChar(cx.char(end))) end++;
        while (end > pos + 1 && (cx.char(end - 1) === 47 || cx.char(end - 1) === 45)) end--;
        if (end === pos + 1) return -1;
        const body = cx.slice(pos + 1, end);
        if (/^\d+$/.test(body)) return -1;
        return cx.addElement(cx.elt('Tag', pos, end));
      },
    },
    {
      name: 'Mention',
      parse(cx, next, pos) {
        if (next !== AT || !isSpaceOrStart(charAt(cx, pos - 1))) return -1;
        let end = pos + 1;
        while (end < cx.end && (isWordChar(cx.char(end)) || cx.char(end) === 46)) end++;
        while (end > pos + 1 && (cx.char(end - 1) === 46 || cx.char(end - 1) === 45)) end--;
        if (end === pos + 1) return -1;
        return cx.addElement(cx.elt('Mention', pos, end));
      },
    },
    {
      name: 'Highlight',
      before: 'Emphasis',
      parse(cx, next, pos) {
        if (next !== EQ || charAt(cx, pos + 1) !== EQ) return -1;
        const end = cx.text.indexOf('==', pos + 2 - cx.offset);
        if (end < 0 || end + cx.offset === pos + 2) return -1;
        const close = end + cx.offset;
        return cx.addElement(cx.elt('Highlight', pos, close + 2, [cx.elt('HighlightMark', pos, pos + 2), cx.elt('HighlightMark', close, close + 2)]));
      },
    },
    {
      name: 'InlineMath',
      parse(cx, next, pos) {
        if (next !== DOLLAR || charAt(cx, pos + 1) === DOLLAR) return -1;
        const prev = charAt(cx, pos - 1);
        if (prev === 92) return -1; // escaped
        const rest = cx.slice(pos + 1, cx.end);
        const m = /^([^$\s](?:[^$]*[^$\s])?)\$(?!\d)/.exec(rest);
        if (!m) return -1;
        return cx.addElement(cx.elt('InlineMath', pos, pos + m[0].length + 1));
      },
    },
  ],
  parseBlock: [
    {
      name: 'BlockMath',
      before: 'FencedCode',
      parse(cx: BlockContext, line: Line) {
        if (!line.text.slice(line.pos).startsWith('$$')) return false;
        const start = cx.lineStart + line.pos;
        // Single-line $$…$$
        if (/^\$\$.+\$\$\s*$/.test(line.text.slice(line.pos))) {
          cx.nextLine();
          cx.addElement(cx.elt('BlockMath', start, cx.prevLineEnd()));
          return true;
        }
        while (cx.nextLine()) {
          if (/^\s*\$\$\s*$/.test(line.text)) {
            const end = cx.lineStart + line.text.length;
            cx.nextLine();
            cx.addElement(cx.elt('BlockMath', start, end));
            return true;
          }
        }
        cx.addElement(cx.elt('BlockMath', start, cx.prevLineEnd()));
        return true;
      },
      endLeaf: () => false,
    },
  ],
};
