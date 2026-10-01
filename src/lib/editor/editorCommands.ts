import {
  Bold,
  Italic,
  Strikethrough,
  Code,
  Highlighter,
  Heading1,
  Heading2,
  Heading3,
  List,
  ListOrdered,
  ListChecks,
  Quote,
  Table,
  Link,
  Link2,
  Image,
  Minus,
  CalendarDays,
  Eye,
  CaseUpper,
  CaseLower,
  CaseSensitive,
  ArrowDownAZ,
  ArrowUpZA,
  Copy,
  ArrowUp,
  ArrowDown,
  Search,
  Sigma,
  Workflow,
  MessageSquareQuote,
  Play,
  ClipboardPaste,
  Rows3,
  Columns3,
  WrapText,
} from '@lucide/svelte';
import type { EditorView } from '@codemirror/view';
import type { Command } from '$lib/commands/registry.svelte';
import { activeEditor } from './active';
import type * as Md from './cm/commands';
import { settings } from '$lib/settings/store.svelte';
import { inputBox, pickOne, quickPick, steps, BACK } from '$lib/quickinput/qi.svelte';
import { rpc } from '$lib/backend/rpc';
import type { SearchHit } from '$lib/backend/types';
import { t } from '$lib/i18n/index.svelte';

const WHEN = 'editorOpen';

// CodeMirror helpers load on first use (keeps CodeMirror out of the startup
// bundle; they are already loaded whenever an editor is open).
let mdMod: typeof Md | null = null;
const loadMd = async () => (mdMod ??= await import('./cm/commands'));

function withView(pick: (m: typeof Md) => (v: EditorView) => unknown): () => void {
  return () => {
    const e = activeEditor();
    if (!e) return;
    const run = (m: typeof Md) => {
      pick(m)(e.view);
      e.view.focus();
    };
    if (mdMod) run(mdMod);
    else void loadMd().then(run);
  };
}

async function pickCard(title: string): Promise<{ id: string; title: string } | undefined> {
  const r = await quickPick<{ id: string; title: string }>([], {
    title,
    placeholder: t('editor.searchCards'),
    selfFiltered: true,
    onValue: async (q) => {
      const hits = await rpc<SearchHit[]>('search.query', { q: q.trim() ? `${q} in:title` : '', opts: { limit: 30 } }).catch(() => []);
      return hits.map((h) => ({
        label: h.title || t('common.untitled'),
        description: [h.boardName, h.laneName].filter(Boolean).join(' · '),
        value: { id: h.id, title: h.title },
      }));
    },
  });
  return r && typeof r === 'object' && !Array.isArray(r) && 'id' in r ? (r as { id: string; title: string }) : undefined;
}

const CODE_LANGS = [
  '',
  'python',
  'javascript',
  'typescript',
  'rust',
  'go',
  'sql',
  'bash',
  'json',
  'yaml',
  'html',
  'css',
  'java',
  'kotlin',
  'swift',
  'c',
  'cpp',
  'csharp',
  'php',
  'ruby',
  'mermaid',
];

export const editorCommands: Command[] = [
  { id: 'editor.bold', title: 'commands.editor.bold', category: 'editor', icon: Bold, when: WHEN, run: withView((m) => m.toggleWrap('**')) },
  { id: 'editor.italic', title: 'commands.editor.italic', category: 'editor', icon: Italic, when: WHEN, run: withView((m) => m.toggleWrap('*')) },
  { id: 'editor.strike', title: 'commands.editor.strike', category: 'editor', icon: Strikethrough, when: WHEN, run: withView((m) => m.toggleWrap('~~')) },
  { id: 'editor.inlineCode', title: 'commands.editor.inlineCode', category: 'editor', icon: Code, when: WHEN, run: withView((m) => m.toggleWrap('`')) },
  { id: 'editor.highlight', title: 'commands.editor.highlight', category: 'editor', icon: Highlighter, when: WHEN, run: withView((m) => m.toggleWrap('==')) },
  { id: 'editor.heading1', title: 'commands.editor.heading1', category: 'editor', icon: Heading1, when: WHEN, run: withView((m) => m.setHeading(1)) },
  { id: 'editor.heading2', title: 'commands.editor.heading2', category: 'editor', icon: Heading2, when: WHEN, run: withView((m) => m.setHeading(2)) },
  { id: 'editor.heading3', title: 'commands.editor.heading3', category: 'editor', icon: Heading3, when: WHEN, run: withView((m) => m.setHeading(3)) },
  { id: 'editor.heading4', title: 'commands.editor.heading4', category: 'editor', when: WHEN, run: withView((m) => m.setHeading(4)) },
  { id: 'editor.heading5', title: 'commands.editor.heading5', category: 'editor', when: WHEN, run: withView((m) => m.setHeading(5)) },
  { id: 'editor.heading6', title: 'commands.editor.heading6', category: 'editor', when: WHEN, run: withView((m) => m.setHeading(6)) },
  { id: 'editor.paragraph', title: 'commands.editor.paragraph', category: 'editor', when: WHEN, run: withView((m) => m.setHeading(0)) },
  {
    id: 'editor.toggleBullet',
    title: 'commands.editor.toggleBullet',
    category: 'editor',
    icon: List,
    when: WHEN,
    run: withView((m) => m.toggleLinePrefix('bullet')),
  },
  {
    id: 'editor.toggleNumbered',
    title: 'commands.editor.toggleNumbered',
    category: 'editor',
    icon: ListOrdered,
    when: WHEN,
    run: withView((m) => m.toggleLinePrefix('ordered')),
  },
  {
    id: 'editor.toggleChecklist',
    title: 'commands.editor.toggleChecklist',
    category: 'editor',
    icon: ListChecks,
    when: WHEN,
    run: withView((m) => m.toggleLinePrefix('task')),
  },
  { id: 'editor.toggleTask', title: 'commands.editor.toggleTask', category: 'editor', icon: ListChecks, when: WHEN, run: withView((m) => m.toggleTaskDone) },
  {
    id: 'editor.toggleQuote',
    title: 'commands.editor.toggleQuote',
    category: 'editor',
    icon: Quote,
    when: WHEN,
    run: withView((m) => m.toggleLinePrefix('quote')),
  },
  {
    id: 'editor.insertCallout',
    title: 'commands.editor.insertCallout',
    category: 'editor',
    icon: MessageSquareQuote,
    when: WHEN,
    run: async () => {
      const kind = await pickOne(
        ['note', 'tip', 'info', 'warning', 'danger', 'question'].map((k) => ({ label: t(`callouts.${k}`), value: k })),
        { title: t('commands.editor.insertCallout') },
      );
      if (typeof kind === 'string') withView((m) => m.insertBlock(`> [!${kind}] `))();
    },
  },
  {
    id: 'editor.insertCodeBlock',
    title: 'commands.editor.insertCodeBlock',
    category: 'editor',
    icon: Code,
    when: WHEN,
    run: async () => {
      const lang = await pickOne(
        CODE_LANGS.map((l) => ({ label: l || t('editor.plainText'), value: l })),
        { title: t('commands.editor.insertCodeBlock'), allowCustom: (s) => ({ label: s, value: s }) },
      );
      if (typeof lang !== 'string') return;
      withView((m) => m.insertBlock('```' + lang + '\n\n```', 4 + lang.length))();
    },
  },
  {
    id: 'editor.insertPython',
    title: 'commands.editor.insertPython',
    category: 'code',
    icon: Play,
    when: WHEN,
    run: withView((m) => m.insertBlock('```python\n\n```', 10)),
  },
  {
    id: 'editor.insertTable',
    title: 'commands.editor.insertTable',
    category: 'editor',
    icon: Table,
    when: WHEN,
    run: async () => {
      const r = await steps<[string, boolean]>([
        () =>
          inputBox({
            title: t('commands.editor.insertTable'),
            prompt: t('editor.tableSizePrompt'),
            value: '3x3',
            step: 1,
            totalSteps: 2,
            validate: (v) => (/^\s*\d{1,2}\s*[x×*]\s*\d{1,3}\s*$/i.test(v) ? null : t('editor.tableSizeInvalid')),
          }),
        () =>
          pickOne(
            [
              { label: t('editor.withHeader'), value: true },
              { label: t('editor.withoutHeader'), value: false },
            ],
            { title: t('commands.editor.insertTable'), step: 2, totalSteps: 2 },
          ),
      ]);
      if (!r) return;
      const [cols, rows] = r[0].split(/[x×*]/i).map((n) => Number(n.trim()));
      withView((m) => m.insertBlock(m.tableMarkdown(cols, rows, r[1]), 2))();
    },
  },
  { id: 'editor.addTableRow', title: 'commands.editor.addTableRow', category: 'editor', icon: Rows3, when: WHEN, run: withView((m) => m.addTableRow) },
  {
    id: 'editor.addTableColumn',
    title: 'commands.editor.addTableColumn',
    category: 'editor',
    icon: Columns3,
    when: WHEN,
    run: withView((m) => m.addTableColumn),
  },
  { id: 'editor.formatTable', title: 'commands.editor.formatTable', category: 'editor', icon: Table, when: WHEN, run: withView((m) => m.formatTable) },
  {
    id: 'editor.insertLink',
    title: 'commands.editor.insertLink',
    category: 'editor',
    icon: Link,
    when: WHEN,
    run: async () => {
      const kind = await pickOne(
        [
          { label: t('editor.linkToCard'), value: 'card', icon: Link2 },
          { label: t('editor.linkToUrl'), value: 'url', icon: Link },
        ],
        { title: t('commands.editor.insertLink'), step: 1, totalSteps: 2 },
      );
      if (kind === 'card') {
        const c = await pickCard(t('editor.linkToCard'));
        if (c) withView((m) => m.insertCardLink(c.id))();
        return;
      }
      if (kind !== 'url') return;
      const e = activeEditor();
      const sel = e ? e.view.state.sliceDoc(e.view.state.selection.main.from, e.view.state.selection.main.to) : '';
      const r = await steps<[string, string]>([
        () =>
          inputBox({
            title: t('editor.linkToUrl'),
            placeholder: 'https://',
            step: 2,
            totalSteps: 3,
            validate: (v) => (/^(https?:\/\/|mailto:)\S+$/i.test(v.trim()) ? null : t('editor.invalidUrl')),
          }),
        ([url]) => inputBox({ title: t('editor.linkText'), value: sel || url, step: 3, totalSteps: 3 }),
      ]);
      if (r) withView((m) => m.insertLink(r[0].trim(), r[1]))();
    },
  },
  {
    id: 'editor.insertCardLink',
    title: 'commands.editor.insertCardLink',
    category: 'editor',
    icon: Link2,
    when: WHEN,
    run: async () => {
      const c = await pickCard(t('commands.editor.insertCardLink'));
      if (c) withView((m) => m.insertCardLink(c.id))();
    },
  },
  {
    id: 'editor.insertEmbed',
    title: 'commands.editor.insertEmbed',
    category: 'editor',
    icon: Link2,
    when: WHEN,
    run: async () => {
      const c = await pickCard(t('commands.editor.insertEmbed'));
      if (c) withView((m) => m.insertBlock(`![[${c.id}]]`))();
    },
  },
  {
    id: 'editor.insertImage',
    title: 'commands.editor.insertImage',
    category: 'editor',
    icon: Image,
    when: WHEN,
    run: () => {
      const e = activeEditor();
      if (!e) return;
      const pos = e.view.state.selection.main.head;
      e.view.dispatch({ changes: { from: pos, insert: '/' }, selection: { anchor: pos + 1 } });
      void import('@codemirror/autocomplete').then((m) => m.startCompletion(e.view));
    },
  },
  {
    id: 'editor.insertDivider',
    title: 'commands.editor.insertDivider',
    category: 'editor',
    icon: Minus,
    when: WHEN,
    run: withView((m) => m.insertBlock('---')),
  },
  {
    id: 'editor.insertMath',
    title: 'commands.editor.insertMath',
    category: 'editor',
    icon: Sigma,
    when: WHEN,
    run: withView((m) => m.insertBlock('$$\n\n$$', 3)),
  },
  {
    id: 'editor.insertMermaid',
    title: 'commands.editor.insertMermaid',
    category: 'editor',
    icon: Workflow,
    when: WHEN,
    run: withView((m) => m.insertBlock('```mermaid\ngraph LR\n  A --> B\n```', 11)),
  },
  { id: 'editor.insertDate', title: 'commands.editor.insertDate', category: 'editor', icon: CalendarDays, when: WHEN, run: withView((m) => m.insertDate) },
  {
    id: 'editor.insertDateTime',
    title: 'commands.editor.insertDateTime',
    category: 'editor',
    icon: CalendarDays,
    when: WHEN,
    run: withView((m) => m.insertDateTime),
  },
  { id: 'editor.togglePreview', title: 'commands.editor.togglePreview', category: 'editor', icon: Eye, run: () => settings.toggle('editor.livePreview') },
  { id: 'editor.toUpper', title: 'commands.editor.toUpper', category: 'editor', icon: CaseUpper, when: WHEN, run: withView((m) => m.toUpper) },
  { id: 'editor.toLower', title: 'commands.editor.toLower', category: 'editor', icon: CaseLower, when: WHEN, run: withView((m) => m.toLower) },
  { id: 'editor.toTitle', title: 'commands.editor.toTitle', category: 'editor', icon: CaseSensitive, when: WHEN, run: withView((m) => m.toTitle) },
  { id: 'editor.toSentence', title: 'commands.editor.toSentence', category: 'editor', icon: CaseSensitive, when: WHEN, run: withView((m) => m.toSentence) },
  { id: 'editor.toggleCase', title: 'commands.editor.toggleCase', category: 'editor', icon: CaseSensitive, when: WHEN, run: withView((m) => m.toggleCase) },
  { id: 'editor.sortAsc', title: 'commands.editor.sortAsc', category: 'editor', icon: ArrowDownAZ, when: WHEN, run: withView((m) => m.sortLinesAsc) },
  { id: 'editor.sortDesc', title: 'commands.editor.sortDesc', category: 'editor', icon: ArrowUpZA, when: WHEN, run: withView((m) => m.sortLinesDesc) },
  { id: 'editor.dedupe', title: 'commands.editor.dedupe', category: 'editor', when: WHEN, run: withView((m) => m.dedupeLines) },
  { id: 'editor.joinLines', title: 'commands.editor.joinLines', category: 'editor', icon: WrapText, when: WHEN, run: withView((m) => m.joinLines) },
  { id: 'editor.trimTrailing', title: 'commands.editor.trimTrailing', category: 'editor', when: WHEN, run: withView((m) => m.trimTrailing) },
  {
    id: 'editor.duplicateLine',
    title: 'commands.editor.duplicateLine',
    category: 'editor',
    icon: Copy,
    when: WHEN,
    run: () => void import('@codemirror/commands').then((m) => withView(() => (v) => m.copyLineDown(v))()),
  },
  {
    id: 'editor.moveLineUp',
    title: 'commands.editor.moveLineUp',
    category: 'editor',
    icon: ArrowUp,
    when: WHEN,
    run: () => void import('@codemirror/commands').then((m) => withView(() => (v) => m.moveLineUp(v))()),
  },
  {
    id: 'editor.moveLineDown',
    title: 'commands.editor.moveLineDown',
    category: 'editor',
    icon: ArrowDown,
    when: WHEN,
    run: () => void import('@codemirror/commands').then((m) => withView(() => (v) => m.moveLineDown(v))()),
  },
  {
    id: 'editor.find',
    title: 'commands.editor.find',
    category: 'editor',
    icon: Search,
    when: WHEN,
    run: () => void import('@codemirror/search').then((m) => withView(() => (v) => m.openSearchPanel(v))()),
  },
  {
    id: 'editor.pastePlain',
    title: 'commands.editor.pastePlain',
    category: 'editor',
    icon: ClipboardPaste,
    when: WHEN,
    run: () => {
      const e = activeEditor();
      if (e) void import('./cm/paste').then((m) => m.pastePlain(e.view));
    },
  },
];

export { BACK };
