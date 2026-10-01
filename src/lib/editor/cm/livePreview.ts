// Live preview (Obsidian-style): Markdown syntax is hidden and rich widgets are
// shown everywhere except where the cursor is. Implemented as a StateField so
// block widgets (tables, math, diagrams, properties, embeds) are allowed.

import { EditorState, StateField, StateEffect, type Range, type Extension, Facet } from '@codemirror/state';
import { Decoration, EditorView, type DecorationSet } from '@codemirror/view';
import { ensureSyntaxTree, syntaxTree } from '@codemirror/language';
import type { SyntaxNode } from '@lezer/common';
import { parseFooter } from '$lib/markdown/meta';
import { luauContext, type CodeResult } from './context';
import {
  BulletWidget,
  CalloutLabelWidget,
  CardLinkWidget,
  CheckboxWidget,
  CodeHeaderWidget,
  CodeOutputWidget,
  DateWidget,
  EmbedWidget,
  FileChipWidget,
  HrWidget,
  ImageWidget,
  LinkPreviewWidget,
  MathWidget,
  MediaWidget,
  MermaidWidget,
  PdfWidget,
  PropertiesWidget,
  TableWidget,
  outputsChanged,
} from './widgets';

/** Set to false to show raw Markdown (source mode). */
export const livePreviewEnabled = Facet.define<boolean, boolean>({ combine: (v) => (v.length ? v[v.length - 1] : true) });

export const setFocused = StateEffect.define<boolean>();
export const refreshPreview = StateEffect.define<null>();

const focusField = StateField.define<boolean>({
  create: () => false,
  update(v, tr) {
    for (const e of tr.effects) if (e.is(setFocused)) v = e.value;
    return v;
  },
});

/** Code outputs keyed by `${lang}\u0000${code}`. */
export const setOutput = StateEffect.define<{ key: string; result: CodeResult | 'running' }>();
export const outputsField = StateField.define<Map<string, CodeResult | 'running'>>({
  create: () => new Map(),
  update(v, tr) {
    let next = v;
    for (const e of tr.effects)
      if (e.is(setOutput)) {
        if (next === v) next = new Map(v);
        next.set(e.value.key, e.value.result);
      }
    return next;
  },
});

export const outputKey = (lang: string, code: string) => `${lang}\u0000${code}`;

const RUNNABLE = new Set(['python', 'py', 'python3']);
const IMG_EXT = /\.(png|jpe?g|gif|webp|svg|avif|bmp|heic)$/i;
const VIDEO_EXT = /\.(mp4|mov|webm|m4v|mkv)$/i;
const AUDIO_EXT = /\.(mp3|wav|ogg|m4a|flac)$/i;
const isRemote = (u: string) => /^[a-z][a-z0-9+.-]*:/i.test(u);

const hide = Decoration.replace({});
const lineDeco = (cls: string) => Decoration.line({ class: cls });

function build(state: EditorState): DecorationSet {
  const enabled = state.facet(livePreviewEnabled);
  const ctx = state.facet(luauContext);
  const out: Range<Decoration>[] = [];
  const doc = state.doc;
  const focused = state.field(focusField);
  const ranges = state.selection.ranges;

  const touchesLines = (from: number, to: number) => {
    if (!focused) return false;
    const a = doc.lineAt(from).from;
    const b = doc.lineAt(to).to;
    return ranges.some((r) => r.from <= b && r.to >= a);
  };
  const touches = (from: number, to: number) => focused && ranges.some((r) => r.from <= to && r.to >= from);

  // Title line.
  const first = doc.line(1);
  if (/^#\s/.test(first.text) || first.text === '#') out.push(lineDeco('cm-title').range(first.from));

  if (!enabled) return Decoration.set(out, true);

  // Property footer (last `---` + key: value lines).
  const lines = doc.toString().split('\n');
  const footer = parseFooter(lines);
  const footerFrom = footer.startLine !== null ? doc.line(footer.startLine + 1).from : doc.length + 1;
  if (footer.startLine !== null) {
    if (!touchesLines(footerFrom, doc.length)) {
      out.push(Decoration.replace({ widget: new PropertiesWidget(footer.fields), block: true }).range(footerFrom, doc.length));
    } else {
      for (let l = footer.startLine + 1; l <= doc.lines; l++) out.push(lineDeco('cm-footer-raw').range(doc.line(l).from));
    }
  }

  const tree = ensureSyntaxTree(state, doc.length, 120) ?? syntaxTree(state);
  const outputs = state.field(outputsField);

  const childOf = (node: SyntaxNode, name: string) => {
    for (let c = node.firstChild; c; c = c.nextSibling) if (c.name === name) return c;
    return null;
  };

  tree.iterate({
    enter(ref) {
      const node = ref.node;
      const { from, to, name } = ref;
      if (from >= footerFrom) return false;
      switch (name) {
        case 'ATXHeading1':
        case 'ATXHeading2':
        case 'ATXHeading3':
        case 'ATXHeading4':
        case 'ATXHeading5':
        case 'ATXHeading6': {
          const level = Number(name.slice(-1));
          out.push(lineDeco(`cm-h cm-h${level}`).range(doc.lineAt(from).from));
          if (!touchesLines(from, to)) {
            const mark = childOf(node, 'HeaderMark');
            if (mark) {
              out.push(hide.range(mark.from, Math.min(mark.to + 1, to)));
              // Optional closing #'s (`## Title ##`).
              const last = node.lastChild;
              if (last && last.name === 'HeaderMark' && last.from > mark.from) out.push(hide.range(Math.max(from, last.from - 1), last.to));
            }
          }
          return;
        }
        case 'Emphasis':
        case 'StrongEmphasis':
        case 'Strikethrough':
        case 'InlineCode':
        case 'Highlight': {
          if (!touches(from, to)) {
            for (let c = node.firstChild; c; c = c.nextSibling)
              if (c.name === 'EmphasisMark' || c.name === 'StrikethroughMark' || c.name === 'CodeMark' || c.name === 'HighlightMark') out.push(hide.range(c.from, c.to));
          }
          if (name === 'Highlight') out.push(Decoration.mark({ class: 'cm-highlight' }).range(from, to));
          return;
        }
        case 'Image': {
          if (touches(from, to)) return false;
          const src = state.sliceDoc(from, to);
          const m = /^!\[([^\]]*)\]\((\S*?)(?:\s+"[^"]*")?\)$/s.exec(src);
          if (!m) return false;
          const [altRaw, url] = [m[1], m[2].replace(/^<|>$/g, '')];
          const [alt, w] = altRaw.split('|');
          const width = w && /^\d+$/.test(w.trim()) ? Number(w) : null;
          let widget;
          if (/\.pdf$/i.test(url)) widget = new PdfWidget(url, width);
          else if (VIDEO_EXT.test(url)) widget = new MediaWidget(url, 'video');
          else if (AUDIO_EXT.test(url)) widget = new MediaWidget(url, 'audio');
          else if (IMG_EXT.test(url) || isRemote(url)) widget = new ImageWidget(url, alt, width, from, to);
          else widget = new FileChipWidget(url, alt);
          out.push(Decoration.replace({ widget }).range(from, to));
          return false;
        }
        case 'Link': {
          if (touches(from, to)) return;
          const urlNode = childOf(node, 'URL');
          const marks: SyntaxNode[] = [];
          for (let c = node.firstChild; c; c = c.nextSibling) if (c.name === 'LinkMark') marks.push(c);
          if (!urlNode || marks.length < 2) return;
          const url = state.sliceDoc(urlNode.from, urlNode.to);
          const labelFrom = marks[0].to;
          const labelTo = marks[1].from;
          const label = state.sliceDoc(labelFrom, labelTo);
          const titleNode = childOf(node, 'LinkTitle');
          const title = titleNode ? state.sliceDoc(titleNode.from + 1, titleNode.to - 1) : '';
          if (!isRemote(url) && !url.startsWith('#')) {
            out.push(Decoration.replace({ widget: new FileChipWidget(decodeURIComponent(url), label) }).range(from, to));
            return false;
          }
          if (/^https?:/i.test(url) && title === 'preview') {
            out.push(Decoration.replace({ widget: new LinkPreviewWidget(url, label) }).range(from, to));
            return false;
          }
          out.push(hide.range(from, labelFrom));
          out.push(hide.range(labelTo, to));
          if (labelTo > labelFrom) out.push(Decoration.mark({ class: 'cm-link-text', attributes: { 'data-href': url } }).range(labelFrom, labelTo));
          return false;
        }
        case 'URL': {
          if (node.parent?.name === 'Link' || node.parent?.name === 'Image') return;
          const url = state.sliceDoc(from, to);
          out.push(Decoration.mark({ class: 'cm-url', attributes: { 'data-href': url } }).range(from, to));
          return;
        }
        case 'CardLink':
        case 'CardEmbed': {
          const raw = state.sliceDoc(from, to);
          const inner = raw.replace(/^!?\[\[|\]\]$/g, '');
          const [targetPart, alias] = inner.split('|');
          const [target, heading] = targetPart.split('#');
          const id = target.trim();
          const isId = /^c[a-z0-9]{6}$/.test(id);
          if (!isId) {
            out.push(Decoration.mark({ class: 'cm-card-link-raw unresolved' }).range(from, to));
            return false;
          }
          const line = doc.lineAt(from);
          if (name === 'CardEmbed' && line.text.trim() === raw && !touchesLines(from, to)) {
            out.push(Decoration.replace({ widget: new EmbedWidget(id, heading?.trim() || null), block: true }).range(line.from, line.to));
            return false;
          }
          if (touches(from, to)) {
            out.push(Decoration.mark({ class: 'cm-card-link-raw' }).range(from, to));
            return false;
          }
          const title = ctx?.titleOf(id) ?? id;
          out.push(Decoration.replace({ widget: new CardLinkWidget(id, heading?.trim() || null, alias?.trim() || null, title, ctx?.isMissing(id) ?? false) }).range(from, to));
          return false;
        }
        case 'Tag': {
          const tag = state.sliceDoc(from + 1, to);
          out.push(Decoration.mark({ class: 'cm-tag', attributes: { style: ctx?.tagStyle(tag) ?? '' } }).range(from, to));
          return;
        }
        case 'Mention':
          out.push(Decoration.mark({ class: 'cm-mention' }).range(from, to));
          return;
        case 'DateChip': {
          if (touches(from, to)) {
            out.push(Decoration.mark({ class: 'cm-date-raw' }).range(from, to));
            return;
          }
          out.push(Decoration.replace({ widget: new DateWidget(state.sliceDoc(from + 1, to - 1)) }).range(from, to));
          return;
        }
        case 'InlineMath': {
          if (touches(from, to)) return;
          out.push(Decoration.replace({ widget: new MathWidget(state.sliceDoc(from + 1, to - 1), false) }).range(from, to));
          return;
        }
        case 'BlockMath': {
          const a = doc.lineAt(from);
          const b = doc.lineAt(to);
          if (touchesLines(from, to)) {
            for (let l = a.number; l <= b.number; l++) out.push(lineDeco('cm-mathblock-raw').range(doc.line(l).from));
            return false;
          }
          const tex = state.sliceDoc(from, to).replace(/^\s*\$\$|\$\$\s*$/g, '').trim();
          out.push(Decoration.replace({ widget: new MathWidget(tex, true), block: true }).range(a.from, b.to));
          return false;
        }
        case 'TaskMarker': {
          const checked = /x/i.test(state.sliceDoc(from, to));
          const line = doc.lineAt(from);
          if (checked) out.push(lineDeco('cm-task-done').range(line.from));
          if (!touches(from, to)) out.push(Decoration.replace({ widget: new CheckboxWidget(checked, from) }).range(from, to));
          return;
        }
        case 'ListMark': {
          const item = node.parent;
          const list = item?.parent;
          const isTask = !!item && !!childOf(item, 'Task');
          const line = doc.lineAt(from);
          if (isTask) {
            if (!touchesLines(from, to)) out.push(hide.range(from, Math.min(to + 1, line.to)));
            return;
          }
          if (list?.name === 'BulletList' && !touchesLines(from, to)) {
            let depth = 0;
            for (let p = list.parent; p; p = p.parent) if (p.name === 'BulletList' || p.name === 'OrderedList') depth++;
            out.push(Decoration.replace({ widget: new BulletWidget(depth) }).range(from, to));
          }
          return;
        }
        case 'Blockquote': {
          const a = doc.lineAt(from);
          const b = doc.lineAt(to);
          const callout = /^\s*>\s*\[!(\w+)\]([+-]?)\s*(.*)$/.exec(a.text);
          const cls = callout ? `cm-callout k-${callout[1].toLowerCase()}` : 'cm-quote';
          for (let l = a.number; l <= b.number; l++) {
            const line = doc.line(l);
            out.push(lineDeco(`${cls}${l === a.number ? ' first' : ''}${l === b.number ? ' last' : ''}`).range(line.from));
            if (!touchesLines(line.from, line.to)) {
              const qm = /^\s*>\s?/.exec(line.text);
              if (qm) out.push(hide.range(line.from, line.from + qm[0].length));
            }
          }
          if (callout && !touchesLines(a.from, a.to)) {
            const start = a.text.indexOf('[!');
            const label = callout[3] || callout[1][0].toUpperCase() + callout[1].slice(1);
            out.push(Decoration.replace({ widget: new CalloutLabelWidget(callout[1].toLowerCase(), label) }).range(a.from + start, a.to));
          }
          return;
        }
        case 'HorizontalRule': {
          if (!touchesLines(from, to)) out.push(Decoration.replace({ widget: new HrWidget() }).range(from, to));
          return;
        }
        case 'FencedCode': {
          const a = doc.lineAt(from);
          const b = doc.lineAt(to);
          const info = childOf(node, 'CodeInfo');
          const lang = info ? state.sliceDoc(info.from, info.to).trim().toLowerCase() : '';
          const codeText = childOf(node, 'CodeText');
          const code = codeText ? state.sliceDoc(codeText.from, codeText.to) : '';
          const active = touchesLines(from, to);
          if (lang === 'mermaid' && !active) {
            out.push(Decoration.replace({ widget: new MermaidWidget(code), block: true }).range(a.from, b.to));
            return false;
          }
          const closed = b.number > a.number && /^\s*(```|~~~)/.test(b.text);
          for (let l = a.number; l <= b.number; l++) {
            const cls = ['cm-codeblock'];
            if (l === a.number) cls.push('cm-code-first');
            if (l === b.number) cls.push('cm-code-last');
            out.push(lineDeco(cls.join(' ')).range(doc.line(l).from));
          }
          if (!active) {
            out.push(Decoration.replace({ widget: new CodeHeaderWidget(lang, code, RUNNABLE.has(lang)) }).range(a.from, a.to));
            if (closed) out.push(Decoration.replace({}).range(b.from, b.to));
          }
          const res = outputs.get(outputKey(lang, code));
          if (res && RUNNABLE.has(lang)) {
            out.push(Decoration.widget({ widget: new CodeOutputWidget(lang, code, res), block: true, side: 1 }).range(b.to));
          }
          return false;
        }
        case 'Table': {
          const a = doc.lineAt(from);
          const b = doc.lineAt(to);
          if (touchesLines(from, to)) {
            for (let l = a.number; l <= b.number; l++) out.push(lineDeco('cm-table-raw').range(doc.line(l).from));
            return false;
          }
          out.push(Decoration.replace({ widget: new TableWidget(state.sliceDoc(a.from, b.to)), block: true }).range(a.from, b.to));
          return false;
        }
      }
    },
  });
  return Decoration.set(out, true);
}

const previewField = StateField.define<DecorationSet>({
  create: (state) => build(state),
  update(deco, tr) {
    const refresh = tr.effects.some((e) => e.is(setFocused) || e.is(refreshPreview) || e.is(setOutput) || e.is(outputsChanged));
    if (tr.docChanged || tr.selection || refresh || syntaxTree(tr.state) !== syntaxTree(tr.startState)) return build(tr.state);
    return deco.map(tr.changes);
  },
  provide: (f) => EditorView.decorations.from(f),
});

const focusTracker = EditorView.focusChangeEffect.of((_state, focused) => setFocused.of(focused));

/** Clicks on rendered links open them; Alt-click edits. */
const linkClicks = EditorView.domEventHandlers({
  mousedown(e, view) {
    const target = (e.target as HTMLElement).closest<HTMLElement>('[data-href]');
    if (!target || e.button !== 0 || e.altKey) return false;
    const href = target.dataset.href!;
    const ctx = view.state.facet(luauContext);
    if (!ctx) return false;
    if (target.classList.contains('cm-url') && !(e.metaKey || e.ctrlKey)) return false;
    e.preventDefault();
    if (/^https?:|^mailto:/i.test(href)) ctx.openExternal(href);
    else ctx.openFile(href);
    return true;
  },
});

export function livePreview(): Extension {
  return [focusField, outputsField, previewField, focusTracker, linkClicks];
}
