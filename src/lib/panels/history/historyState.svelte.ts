// Shared state of the History panel (timeline filters, card focus, open diff).

import type { JournalEntry } from "$lib/backend/types";
import { ui, persistUi } from "$lib/state/ui.svelte";
import type { KindGroup, RangePreset } from "./model";

export type HistoryView = "timeline" | "trash" | "archive";

export interface CardFocus {
  boardId: string;
  id: string;
  title: string;
}

export interface DiffRequest {
  entry: JournalEntry;
  /** Other version entries of the same card (newest first) for prev/next stepping. */
  siblings: JournalEntry[];
}

export const hist = $state({
  view: "timeline" as HistoryView,
  /** '' = all boards. Also the board shown by the Trash / Archive views when set. */
  board: "",
  card: null as CardFocus | null,
  range: "week" as RangePreset,
  sources: [] as string[],
  groups: [] as KindGroup[],
  text: "",
  limit: 200,
  /** Bumped to reload the current view. */
  refresh: 0,
  diff: null as DiffRequest | null,
});

function reveal() {
  ui.left.section = "history";
  ui.left.visible = true;
  persistUi();
}

export function showHistoryView(view: HistoryView, board?: string) {
  hist.view = view;
  if (board !== undefined) hist.board = board;
  reveal();
}

/** Focus the timeline on one card ("Show history for this card"). */
export function showCardHistory(boardId: string, id: string, title: string) {
  hist.card = { boardId, id, title };
  hist.board = boardId;
  hist.range = "all";
  hist.groups = [];
  hist.sources = [];
  hist.text = "";
  hist.view = "timeline";
  hist.refresh++;
  reveal();
}

export function clearCardFocus() {
  hist.card = null;
  hist.range = "week";
  hist.refresh++;
}

export function openDiff(entry: JournalEntry, siblings: JournalEntry[] = []) {
  hist.diff = { entry, siblings };
}

export function closeDiff() {
  hist.diff = null;
}
