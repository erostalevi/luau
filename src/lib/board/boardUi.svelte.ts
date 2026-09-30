// Transient per-window board interaction state (quick add, inline rename…).

import type { Parent } from '$lib/backend/types';

export interface QuickAddTarget {
  boardId: string;
  parent: Parent;
  /** Insert before this sibling (null = end). */
  before: string | null;
  /** Unique key of the list rendering the input. */
  key: string;
}

export const boardUi = $state({
  quickAdd: null as QuickAddTarget | null,
  renamingCard: null as string | null,
  renamingLane: null as string | null,
  newLane: null as { boardId: string; index: number | null } | null,
  /** Groups collapsed in the board view (app-local). */
  collapsed: {} as Record<string, boolean>,
  /** Cards to flash briefly (after creation / navigation). */
  flash: null as string | null,
});

export function listKey(boardId: string, p: Parent): string {
  return `${boardId}:${p.kind}:${'id' in p ? p.id : ''}`;
}

export function flash(id: string) {
  boardUi.flash = id;
  setTimeout(() => {
    if (boardUi.flash === id) boardUi.flash = null;
  }, 900);
}
