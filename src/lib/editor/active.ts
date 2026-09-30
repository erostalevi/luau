// Registry of the focused CodeMirror editor so commands can target it.

import type { EditorView } from '@codemirror/view';
import { ctx } from '$lib/commands/context.svelte';

export interface ActiveEditor {
  view: EditorView;
  boardId: string;
  cardId: string;
}

let current: ActiveEditor | null = null;
let last: ActiveEditor | null = null;

export function setActiveEditor(e: ActiveEditor | null) {
  current = e;
  if (e) last = e;
  ctx.editorOpen = !!last;
}

export function clearActiveEditor(view: EditorView) {
  if (current?.view === view) current = null;
  if (last?.view === view) last = null;
  ctx.editorOpen = !!last;
}

/** Focused editor, or the last one that had focus (for palette commands). */
export function activeEditor(): ActiveEditor | null {
  return current ?? last;
}

export function scrollEditorToLine(line: number) {
  const e = activeEditor();
  if (!e) return;
  const doc = e.view.state.doc;
  const l = doc.line(Math.min(doc.lines, line + 1));
  e.view.dispatch({ selection: { anchor: l.from }, scrollIntoView: true });
  e.view.focus();
}
