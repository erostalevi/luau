// Assembles the CodeMirror extension set for Luau editors.

import { EditorState, Compartment, type Extension } from '@codemirror/state';
import {
  EditorView,
  keymap,
  placeholder as cmPlaceholder,
  drawSelection,
  dropCursor,
  highlightSpecialChars,
  rectangularSelection,
  crosshairCursor,
  lineNumbers,
} from '@codemirror/view';
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands';
import { markdown, markdownLanguage, markdownKeymap } from '@codemirror/lang-markdown';
import { languages } from '@codemirror/language-data';
import { syntaxHighlighting, HighlightStyle, bracketMatching, indentOnInput } from '@codemirror/language';
import { closeBrackets, closeBracketsKeymap, completionKeymap } from '@codemirror/autocomplete';
import { searchKeymap, highlightSelectionMatches, search } from '@codemirror/search';
import { tags as t } from '@lezer/highlight';
import { luauMarkdown } from './markdown';
import { livePreview, livePreviewEnabled, setOutput, outputKey } from './livePreview';
import { luauContext, type LuauEditorContext } from './context';
import { luauCompletions, type CompletionData } from './autocomplete';
import { pasteAndDrop, type PasteOptions } from './paste';
import { runRequest } from './widgets';
import { verticalMotionKeymap } from './verticalMotion';

export const compartments = {
  preview: new Compartment(),
  vim: new Compartment(),
  spell: new Compartment(),
  lines: new Compartment(),
  readOnly: new Compartment(),
  autoPair: new Compartment(),
  tabSize: new Compartment(),
};

const highlight = HighlightStyle.define([
  { tag: t.heading, fontWeight: '650' },
  { tag: t.strong, fontWeight: '680' },
  { tag: t.emphasis, fontStyle: 'italic' },
  { tag: t.strikethrough, textDecoration: 'line-through', color: 'var(--ink-3)' },
  { tag: t.link, color: 'var(--primary-strong)' },
  { tag: t.url, color: 'var(--ink-3)' },
  { tag: t.monospace, class: 'cm-md-code' },
  { tag: t.processingInstruction, class: 'cm-md-mark' },
  { tag: t.contentSeparator, color: 'var(--ink-4)' },
  { tag: t.quote, color: 'var(--ink-2)' },
  { tag: t.list, color: 'var(--ink-3)' },
  // code
  { tag: [t.keyword, t.operatorKeyword, t.modifier], color: 'var(--primary-strong)' },
  { tag: [t.string, t.special(t.string)], color: '#4f9a6b' },
  { tag: [t.number, t.bool, t.null, t.atom], color: '#c07a3c' },
  { tag: [t.comment, t.lineComment, t.blockComment], color: 'var(--ink-4)', fontStyle: 'italic' },
  { tag: [t.function(t.variableName), t.function(t.propertyName)], color: '#3f7fc4' },
  { tag: [t.typeName, t.className], color: '#a45fc2' },
  { tag: [t.propertyName, t.attributeName], color: '#b5655d' },
  { tag: t.meta, color: 'var(--ink-3)' },
]);

const baseTheme = EditorView.theme({
  '&': { fontSize: 'inherit' },
  '.cm-scroller': { fontFamily: 'inherit' },
});

export interface SetupOptions {
  doc: string;
  ctx: LuauEditorContext;
  completion: CompletionData;
  paste: PasteOptions;
  placeholder: string;
  livePreview: boolean;
  vim: boolean;
  spellcheck: boolean;
  lineNumbers: boolean;
  readOnly: boolean;
  autoPair: boolean;
  tabSize: number;
  onChange: (doc: string) => void;
  onFocus?: (focused: boolean, view: EditorView) => void;
  onSelection?: (hasSelection: boolean) => void;
  extraKeys?: Parameters<typeof keymap.of>[0];
}

async function vimExt(on: boolean): Promise<Extension> {
  if (!on) return [];
  const { vim } = await import('@replit/codemirror-vim');
  return vim({ status: true });
}

export function spellAttrs(on: boolean): Extension {
  return EditorView.contentAttributes.of({ spellcheck: on ? 'true' : 'false', autocorrect: on ? 'on' : 'off', autocapitalize: 'sentences' });
}

export function createState(o: SetupOptions): EditorState {
  const runner = EditorView.updateListener.of((u) => {
    for (const tr of u.transactions)
      for (const e of tr.effects)
        if (e.is(runRequest)) {
          const key = outputKey(e.value.lang, e.value.code);
          u.view.dispatch({ effects: setOutput.of({ key, result: 'running' }) });
          void o.ctx.runCode(e.value.lang, e.value.code).then((result) => u.view.dispatch({ effects: setOutput.of({ key, result }) }));
        }
  });
  return EditorState.create({
    doc: o.doc,
    extensions: [
      compartments.vim.of([]),
      luauContext.of(o.ctx),
      history(),
      drawSelection(),
      dropCursor(),
      highlightSpecialChars(),
      rectangularSelection(),
      crosshairCursor(),
      indentOnInput(),
      bracketMatching(),
      highlightSelectionMatches(),
      search({ top: true }),
      EditorView.lineWrapping,
      markdown({ base: markdownLanguage, codeLanguages: languages, extensions: [luauMarkdown], addKeymap: true }),
      syntaxHighlighting(highlight),
      compartments.preview.of(livePreviewEnabled.of(o.livePreview)),
      livePreview(),
      compartments.autoPair.of(o.autoPair ? closeBrackets() : []),
      compartments.spell.of(spellAttrs(o.spellcheck)),
      compartments.lines.of(o.lineNumbers ? lineNumbers() : []),
      compartments.readOnly.of([EditorState.readOnly.of(o.readOnly), EditorView.editable.of(!o.readOnly)]),
      compartments.tabSize.of(EditorState.tabSize.of(o.tabSize)),
      cmPlaceholder(o.placeholder),
      luauCompletions(o.completion),
      pasteAndDrop(o.paste),
      runner,
      baseTheme,
      keymap.of([
        ...(o.extraKeys ?? []),
        ...closeBracketsKeymap,
        ...completionKeymap,
        ...markdownKeymap,
        ...searchKeymap,
        ...historyKeymap,
        ...verticalMotionKeymap,
        ...defaultKeymap,
        indentWithTab,
      ]),
      EditorView.updateListener.of((u) => {
        if (u.docChanged) o.onChange(u.state.doc.toString());
        if (u.focusChanged) o.onFocus?.(u.view.hasFocus, u.view);
        if (u.selectionSet) o.onSelection?.(!u.state.selection.main.empty);
      }),
    ],
  });
}

export async function applyVim(view: EditorView, on: boolean) {
  view.dispatch({ effects: compartments.vim.reconfigure(await vimExt(on)) });
}

export function reconfigure(
  view: EditorView,
  patch: Partial<Pick<SetupOptions, 'livePreview' | 'spellcheck' | 'lineNumbers' | 'readOnly' | 'autoPair' | 'tabSize'>>,
) {
  const effects = [];
  if (patch.livePreview !== undefined) effects.push(compartments.preview.reconfigure(livePreviewEnabled.of(patch.livePreview)));
  if (patch.spellcheck !== undefined) effects.push(compartments.spell.reconfigure(spellAttrs(patch.spellcheck)));
  if (patch.lineNumbers !== undefined) effects.push(compartments.lines.reconfigure(patch.lineNumbers ? lineNumbers() : []));
  if (patch.readOnly !== undefined)
    effects.push(compartments.readOnly.reconfigure([EditorState.readOnly.of(patch.readOnly), EditorView.editable.of(!patch.readOnly)]));
  if (patch.autoPair !== undefined) effects.push(compartments.autoPair.reconfigure(patch.autoPair ? closeBrackets() : []));
  if (patch.tabSize !== undefined) effects.push(compartments.tabSize.reconfigure(EditorState.tabSize.of(patch.tabSize)));
  if (effects.length) view.dispatch({ effects });
}
