// Up/Down that walk every line. The live preview replaces whole blocks
// (tables, math, diagrams, embeds, the property footer) with widgets and only
// reveals the raw text once the cursor is on it. CodeMirror's default vertical
// motion works in screen space, so it either hops over those widgets or lands
// in them and then jumps when the raw text expands under it. Here the target
// is picked in document space: inside a wrapped line we move one visual row
// at a time; at its edge we step to the neighbouring document line, let it
// reveal, and only then place the cursor at the goal x on its first (down) or
// last (up) visual row.

import { EditorSelection, type SelectionRange } from '@codemirror/state';
import { EditorView, type Command, type KeyBinding } from '@codemirror/view';

type Dir = 'up' | 'down';

/** the document line a vertical step leaves for, or null when the step stays inside the current line. */
function lineStep(lineNumber: number, lines: number, dir: Dir): number | 'docStart' | 'docEnd' {
  if (dir === 'down') return lineNumber < lines ? lineNumber + 1 : 'docEnd';
  return lineNumber > 1 ? lineNumber - 1 : 'docStart';
}

// CodeMirror keeps goalColumn relative to the content box; we work in client x.
const contentLeft = (view: EditorView) => view.contentDOM.getBoundingClientRect().left;

function goalX(view: EditorView, r: SelectionRange): number | null {
  if (r.goalColumn != null) return contentLeft(view) + r.goalColumn;
  const c = view.coordsAtPos(r.head, r.assoc || 1);
  return c ? c.left : null;
}

/** True when `r` sits on the first (up) or last (down) visual row of its document line. */
function atLineEdge(view: EditorView, r: SelectionRange, dir: Dir): boolean {
  const line = view.state.doc.lineAt(r.head);
  const cursor = EditorSelection.cursor(r.head, r.assoc);
  const bound = view.moveToLineBoundary(cursor, dir === 'down', true).head;
  return dir === 'down' ? bound >= line.to : bound <= line.from;
}

function moveRange(view: EditorView, r: SelectionRange, dir: Dir, extend: boolean): { range: SelectionRange; target?: number; x: number | null } {
  const x = goalX(view, r);
  const doc = view.state.doc;
  if (!atLineEdge(view, r, dir)) {
    const moved = view.moveVertically(r, dir === 'down');
    return { range: extend ? EditorSelection.range(r.anchor, moved.head, moved.goalColumn) : moved, x };
  }
  const step = lineStep(doc.lineAt(r.head).number, doc.lines, dir);
  if (step === 'docEnd' || step === 'docStart') {
    const pos = step === 'docEnd' ? doc.length : 0;
    return { range: extend ? EditorSelection.range(r.anchor, pos) : EditorSelection.cursor(pos), x };
  }
  const line = doc.line(step);
  const pos = dir === 'down' ? line.from : line.to;
  return { range: extend ? EditorSelection.range(r.anchor, pos) : EditorSelection.cursor(pos), target: step, x };
}

/** Second pass, after the target lines revealed: put each head at the goal x on the right visual row. */
function placeAtGoal(view: EditorView, r: SelectionRange, lineNo: number, dir: Dir, x: number | null, extend: boolean): SelectionRange {
  const line = view.state.doc.line(lineNo);
  let pos = dir === 'down' ? line.from : line.to;
  let assoc: -1 | 1 = dir === 'down' ? 1 : -1;
  if (x != null) {
    const c = view.coordsAtPos(pos, assoc);
    if (c) {
      const y = (c.top + c.bottom) / 2;
      const hit = view.posAtCoords({ x, y }, false);
      if (hit >= line.from && hit <= line.to) {
        pos = hit;
        // A position on a wrap point belongs to two rows; stick to the one we aimed at.
        const after = view.coordsAtPos(pos, 1);
        assoc = after && after.top <= y && after.bottom >= y ? 1 : -1;
      }
    }
  }
  const goal = x == null ? undefined : x - contentLeft(view);
  if (extend) return EditorSelection.range(r.anchor, pos, goal);
  return EditorSelection.cursor(pos, assoc, undefined, goal);
}

function vertical(dir: Dir, extend: boolean): Command {
  return (view) => {
    const { state } = view;
    const steps = state.selection.ranges.map((r) => {
      // Collapse a selection first, like the default commands do.
      if (!extend && !r.empty) {
        const pos = dir === 'down' ? r.to : r.from;
        return { range: EditorSelection.cursor(pos), x: null as number | null, target: undefined as number | undefined, collapsed: true };
      }
      return { ...moveRange(view, r, dir, extend), collapsed: false };
    });
    const first = EditorSelection.create(
      steps.map((s) => s.range),
      state.selection.mainIndex,
    );
    if (first.eq(state.selection) && !steps.some((s) => s.target)) return false;
    view.dispatch({ selection: first, scrollIntoView: true, userEvent: 'select' });
    if (!steps.some((s) => s.target)) return true;
    // The step above revealed the target lines; place the heads now that the layout is final.
    const placed = view.state.selection.ranges.map((r, i) => {
      const s = steps[i];
      return s?.target ? placeAtGoal(view, r, s.target, dir, s.x, extend) : r;
    });
    view.dispatch({
      selection: EditorSelection.create(placed, view.state.selection.mainIndex),
      scrollIntoView: true,
      userEvent: 'select',
    });
    return true;
  };
}

export const cursorUp = vertical('up', false);
export const cursorDown = vertical('down', false);
export const selectUp = vertical('up', true);
export const selectDown = vertical('down', true);

/** Bind ahead of the default keymap. Vim mode keeps its own j/k. */
export const verticalMotionKeymap: KeyBinding[] = [
  { key: 'ArrowUp', run: cursorUp, shift: selectUp },
  { key: 'ArrowDown', run: cursorDown, shift: selectDown },
];
