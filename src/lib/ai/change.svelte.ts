// "Change with AI…": apply an instruction to the selected text (or the whole
// card body when nothing is selected), stream the answer, then show a
// before/after preview. Accepting replaces the range in one undo step.

import { rpc, onCoreEvent } from '$lib/backend/rpc';
import { activeEditor } from '$lib/editor/active';
import { quickPick } from '$lib/quickinput/qi.svelte';
import { parseFooter } from '$lib/markdown/meta';
import { toast } from '$lib/state/toasts.svelte';
import { t } from '$lib/i18n/index.svelte';
import type { EditorView } from '@codemirror/view';

export interface DiffSeg {
  op: 'eq' | 'del' | 'ins';
  text: string;
}

interface TransformResult {
  text: string;
  diff: DiffSeg[];
  provider: string;
  model: string;
}

export const PRESETS = ['fix', 'shorter', 'clearer', 'formal', 'casual', 'checklist', 'bullets', 'toEnglish', 'toSpanish', 'toPortuguese'] as const;

export const aiChange = $state<{
  open: boolean;
  instruction: string;
  before: string;
  after: string;
  diff: DiffSeg[];
  streaming: boolean;
  error: string | null;
  model: string;
  wholeCard: boolean;
}>({ open: false, instruction: '', before: '', after: '', diff: [], streaming: false, error: null, model: '', wholeCard: false });

let target: { view: EditorView; from: number; to: number; text: string } | null = null;
let seq = 0;

/** The range to change: the selection, else the card body without title line and footer. */
export function changeRange(doc: string, sel: { from: number; to: number }): { from: number; to: number; whole: boolean } {
  if (sel.to > sel.from) return { from: sel.from, to: sel.to, whole: false };
  const lines = doc.split('\n');
  let from = 0;
  if (/^#\s/.test(lines[0] ?? '')) {
    from = (lines[0]?.length ?? 0) + 1;
    while (from < doc.length && doc[from] === '\n') from++;
  }
  const footer = parseFooter(lines);
  let to = doc.length;
  if (footer.startLine !== null) to = lines.slice(0, footer.startLine).join('\n').length;
  while (to > from && /\s/.test(doc[to - 1])) to--;
  return { from: Math.min(from, to), to, whole: true };
}

async function run() {
  if (!target) return;
  const id = ++seq;
  const requestId = 'c' + Math.random().toString(36).slice(2, 12);
  aiChange.after = '';
  aiChange.diff = [];
  aiChange.error = null;
  aiChange.streaming = true;
  const off = onCoreEvent((e) => {
    if (e.type !== 'custom' || e.name !== 'ai.chunk' || id !== seq) return;
    const p = e.payload as { requestId: string; text: string };
    if (p.requestId === requestId) aiChange.after += p.text;
  });
  try {
    const r = await rpc<TransformResult>('ai.transform', { text: target.text, instruction: aiChange.instruction, requestId });
    if (id !== seq) return;
    aiChange.after = r.text;
    aiChange.diff = r.diff;
    aiChange.model = r.model;
  } catch (e) {
    if (id === seq) aiChange.error = (e as Error).message;
  } finally {
    off();
    if (id === seq) aiChange.streaming = false;
  }
}

/** Ask for an instruction (or take `instruction`) and start. */
export async function changeWithAi(instruction?: string) {
  const ed = activeEditor();
  if (!ed) return;
  const view = ed.view;
  const doc = view.state.doc.toString();
  const range = changeRange(doc, view.state.selection.main);
  const text = doc.slice(range.from, range.to);
  if (!text.trim()) {
    toast.info(t('aiChange.nothing'));
    return;
  }
  let what = instruction;
  if (!what) {
    const r = await quickPick<string>(
      PRESETS.map((p) => ({ label: t(`aiChange.presets.${p}`), value: t(`aiChange.prompts.${p}`) })),
      {
        title: range.whole ? t('aiChange.titleWhole') : t('aiChange.title'),
        placeholder: t('aiChange.placeholder'),
        allowCustom: (s) => (s.trim() ? { label: s.trim(), value: s.trim() } : null),
      },
    );
    if (typeof r !== 'string') return;
    what = r;
  }
  target = { view, from: range.from, to: range.to, text };
  Object.assign(aiChange, { open: true, instruction: what, before: text, wholeCard: range.whole, model: '' });
  void run();
}

export function retry() {
  void run();
}

export function closeChange() {
  seq++;
  aiChange.open = false;
  target = null;
  activeEditor()?.view.focus();
}

/** Apply the answer: replace the range, or insert it below. One undo step. */
export function acceptChange(mode: 'replace' | 'below') {
  const tg = target;
  if (!tg || !aiChange.after || aiChange.streaming) return;
  const { view } = tg;
  if (view.state.sliceDoc(tg.from, tg.to) !== tg.text) {
    toast.warn(t('aiChange.changed'));
    return;
  }
  // Keep the selection's own trailing newlines so the paragraph spacing survives.
  const after = aiChange.after.replace(/\n+$/, '') + (/\n*$/.exec(tg.text)?.[0] ?? '');
  const changes =
    mode === 'replace' ? { from: tg.from, to: tg.to, insert: after } : { from: tg.to, to: tg.to, insert: `${/\n$/.test(tg.text) ? '' : '\n\n'}${after}` };
  const end = changes.from + changes.insert.length;
  view.dispatch({ changes, selection: { anchor: mode === 'replace' ? tg.from : changes.from, head: end }, scrollIntoView: true, userEvent: 'input.ai' });
  closeChange();
}
