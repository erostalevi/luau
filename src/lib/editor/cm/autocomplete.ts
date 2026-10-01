// Autocomplete sources: slash menu, #tags, @mentions, [[card links]].
// Natural dates: typing `]` after `[tomorrow` converts to `[2026-10-01]`.

import { autocompletion, type Completion, type CompletionContext, type CompletionResult } from '@codemirror/autocomplete';
import { EditorView } from '@codemirror/view';
import { Prec, type Extension } from '@codemirror/state';
import { parseNaturalDate } from '$lib/util/naturalDate';
import { tableMarkdown } from './commands';

export interface CompletionData {
  tags(): Promise<string[]>;
  people(): Promise<string[]>;
  searchCards(q: string): Promise<{ id: string; title: string; board: string }[]>;
  /** Create a new card titled `title` next to the edited card; returns its id. */
  createCard(title: string): Promise<string | null>;
  headings(id: string): Promise<string[]>;
  insertFile(view: EditorView, from: number, to: number): void;
  t(key: string): string;
}

interface SlashItem {
  id: string;
  label: string;
  detail?: string;
  keywords: string;
  insert: string | ((view: EditorView, from: number, to: number) => void);
  cursor?: number;
  block?: boolean;
}

function slashItems(d: CompletionData): SlashItem[] {
  const t = d.t;
  return [
    { id: 'h1', label: t('slash.h1'), keywords: 'heading title h1', insert: '# ', block: true },
    { id: 'h2', label: t('slash.h2'), keywords: 'heading h2 subtitle', insert: '## ', block: true },
    { id: 'h3', label: t('slash.h3'), keywords: 'heading h3', insert: '### ', block: true },
    { id: 'bullet', label: t('slash.bullet'), keywords: 'list bullet ul', insert: '- ', block: true },
    { id: 'number', label: t('slash.numbered'), keywords: 'list numbered ordered ol', insert: '1. ', block: true },
    { id: 'todo', label: t('slash.checklist'), keywords: 'todo task checklist checkbox', insert: '- [ ] ', block: true },
    { id: 'quote', label: t('slash.quote'), keywords: 'quote blockquote', insert: '> ', block: true },
    { id: 'callout', label: t('slash.callout'), keywords: 'callout note info tip warning', insert: '> [!note] ', block: true },
    { id: 'code', label: t('slash.code'), keywords: 'code snippet fence', insert: '```\n\n```', cursor: 4, block: true },
    { id: 'python', label: t('slash.python'), detail: t('slash.pythonDetail'), keywords: 'python run notebook jupyter cell', insert: '```python\n\n```', cursor: 10, block: true },
    { id: 'table', label: t('slash.table'), keywords: 'table grid', insert: tableMarkdown(3, 2), cursor: 2, block: true },
    { id: 'divider', label: t('slash.divider'), keywords: 'divider hr rule line separator', insert: '---\n', block: true },
    { id: 'math', label: t('slash.math'), keywords: 'math latex katex equation formula', insert: '$$\n\n$$', cursor: 3, block: true },
    { id: 'mermaid', label: t('slash.mermaid'), keywords: 'mermaid diagram chart flow graph', insert: '```mermaid\ngraph LR\n  A --> B\n```', cursor: 11, block: true },
    { id: 'image', label: t('slash.file'), detail: t('slash.fileDetail'), keywords: 'image picture file attachment pdf upload', insert: (v, f, to) => d.insertFile(v, f, to) },
    { id: 'link', label: t('slash.cardLink'), keywords: 'link card reference wiki', insert: '[[', block: false },
    { id: 'embed', label: t('slash.embed'), keywords: 'embed transclude card include', insert: '![[', block: false },
    { id: 'date', label: t('slash.today'), keywords: 'date today now', insert: `[${new Date().toISOString().slice(0, 10)}] ` },
    { id: 'toc', label: t('slash.properties'), keywords: 'properties priority due assignee labels metadata', insert: '\n---\npriority: \n', cursor: 15, block: true },
  ];
}

function slashSource(d: CompletionData) {
  const items = slashItems(d);
  return (ctx: CompletionContext): CompletionResult | null => {
    const m = ctx.matchBefore(/(?:^|\s)\/[\w-]*$/);
    if (!m) return null;
    const slashAt = m.text.lastIndexOf('/') + m.from;
    const q = ctx.state.sliceDoc(slashAt + 1, ctx.pos).toLowerCase();
    const options: Completion[] = items
      .filter((it) => !q || it.label.toLowerCase().includes(q) || it.keywords.includes(q))
      .map((it) => ({
        label: it.label,
        detail: it.detail,
        type: 'slash',
        apply: (view, _c, from, to) => {
          if (typeof it.insert === 'function') {
            it.insert(view, from, to);
            return;
          }
          const line = view.state.doc.lineAt(from);
          const atStart = view.state.sliceDoc(line.from, from).trim() === '';
          let text = it.insert;
          let pre = '';
          if (it.block && !atStart) pre = '\n';
          text = pre + text;
          view.dispatch({
            changes: { from, to, insert: text },
            selection: { anchor: from + pre.length + (it.cursor ?? it.insert.length) },
            userEvent: 'input.complete',
          });
        },
      }));
    return { from: slashAt, options, filter: false };
  };
}

function tagSource(d: CompletionData) {
  let cache: string[] | null = null;
  return async (ctx: CompletionContext): Promise<CompletionResult | null> => {
    const m = ctx.matchBefore(/(?:^|[\s(,;])#[\p{L}\p{N}_/-]*$/u);
    if (!m) return null;
    const hash = m.text.lastIndexOf('#') + m.from;
    const line = ctx.state.doc.lineAt(ctx.pos);
    // `# ` headings are not tags.
    if (hash === line.from && ctx.state.sliceDoc(hash + 1, hash + 2) === ' ') return null;
    cache ??= await d.tags();
    return {
      from: hash + 1,
      options: cache.map((tag) => ({ label: tag, type: 'tag' })),
      validFor: /^[\p{L}\p{N}_/-]*$/u,
    };
  };
}

function mentionSource(d: CompletionData) {
  let cache: string[] | null = null;
  return async (ctx: CompletionContext): Promise<CompletionResult | null> => {
    const m = ctx.matchBefore(/(?:^|[\s(,;])@[\p{L}\p{N}_.-]*$/u);
    if (!m) return null;
    const at = m.text.lastIndexOf('@') + m.from;
    cache ??= await d.people();
    return { from: at + 1, options: cache.map((p) => ({ label: p, type: 'mention' })), validFor: /^[\p{L}\p{N}_.-]*$/u };
  };
}

function cardLinkSource(d: CompletionData) {
  return async (ctx: CompletionContext): Promise<CompletionResult | null> => {
    const heading = ctx.matchBefore(/!?\[\[(c[a-z0-9]{6})#[^\]\n]*$/);
    if (heading) {
      const id = /\[\[(c[a-z0-9]{6})#/.exec(heading.text)![1];
      const hs = await d.headings(id);
      const from = heading.from + heading.text.indexOf('#') + 1;
      return {
        from,
        options: hs.map((h) => ({ label: h, type: 'heading', apply: `${h}]]` })),
        validFor: /^[^\]\n]*$/,
      };
    }
    const m = ctx.matchBefore(/!?\[\[[^\]\n]*$/);
    if (!m) return null;
    const open = m.text.indexOf('[[') + m.from + 2;
    const q = ctx.state.sliceDoc(open, ctx.pos);
    const hits = await d.searchCards(q);
    const after = ctx.state.sliceDoc(ctx.pos, ctx.pos + 2);
    const exact = hits.some((h) => h.title.trim().toLowerCase() === q.trim().toLowerCase());
    const create: Completion[] =
      q.trim() && !exact
        ? [
            {
              label: d.t('editor.createCardNamed').replace('{title}', q.trim()),
              type: 'create',
              boost: -99,
              apply: (view: EditorView, _c: Completion, from: number, to: number) => {
                void d.createCard(q.trim()).then((id) => {
                  if (!id) return;
                  const close = view.state.sliceDoc(to, to + 2) === ']]' ? '' : ']]';
                  view.dispatch({ changes: { from, to, insert: `${id}${close}` }, selection: { anchor: from + id.length + 2 }, userEvent: 'input.complete' });
                });
              },
            },
          ]
        : [];
    return {
      from: open,
      filter: false,
      options: [...hits.map((h) => ({
        label: h.title || d.t('common.untitled'),
        detail: h.board,
        type: 'card',
        apply: (view: EditorView, _c: Completion, from: number, to: number) => {
          const close = after === ']]' ? '' : ']]';
          view.dispatch({ changes: { from, to, insert: `${h.id}${close}` }, selection: { anchor: from + h.id.length + 2 }, userEvent: 'input.complete' });
        },
      })), ...create],
    };
  };
}

/** Convert `[natural date]` to `[YYYY-MM-DD]` when the bracket closes. */
const naturalDates = Prec.highest(EditorView.inputHandler.of((view, from, to, text) => {
  if (text !== ']') return false;
  const line = view.state.doc.lineAt(from);
  const before = view.state.sliceDoc(line.from, from);
  const m = /\[([^[\]]{2,32})$/.exec(before);
  if (!m || /^\[/.test(m[1])) return false;
  const next = view.state.sliceDoc(to, to + 1);
  if (next === '(' || before.endsWith('[[')) return false;
  const iso = parseNaturalDate(m[1]);
  if (!iso || iso === m[1]) return false;
  const start = from - m[1].length;
  // Swallow a bracket auto-inserted by closeBrackets.
  const end = next === ']' ? to + 1 : to;
  view.dispatch({ changes: { from: start, to: end, insert: `${iso}]` }, selection: { anchor: start + iso.length + 1 }, userEvent: 'input.date' });
  return true;
}));

export function luauCompletions(d: CompletionData): Extension {
  return [
    autocompletion({
      override: [slashSource(d), cardLinkSource(d), tagSource(d), mentionSource(d)],
      icons: false,
      closeOnBlur: true,
      activateOnTypingDelay: 40,
      optionClass: (c) => `cm-opt-${c.type ?? 'x'}`,
    }),
    naturalDates,
  ];
}
