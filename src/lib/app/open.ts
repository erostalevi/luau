// Open a card wherever it belongs: files-board docs get a document tab,
// kanban cards open in the card editor (modal or sidebar) over their board.

import { openBoard, boards } from '$lib/state/boards.svelte';
import { openBoardTab, openDocTab, activeTab } from '$lib/state/workspace.svelte';
import { openEditor } from '$lib/state/ui.svelte';
import { uiPushRecent } from '$lib/state/persist.svelte';
import { select } from '$lib/state/selection.svelte';
import { rpc } from '$lib/backend/rpc';

export async function openCard(boardId: string, cardId: string, opts: { focusBoard?: boolean } = {}) {
  const m = boards.get(boardId) ?? (await openBoard({ id: boardId }));
  uiPushRecent('recentCards', { boardId, cardId }, (x) => x.boardId === boardId && x.cardId === cardId);
  if (m.kind === 'files') {
    await openDocTab(boardId, cardId);
    return;
  }
  const tab = activeTab();
  if (opts.focusBoard !== false && tab?.boardId !== boardId) await openBoardTab(boardId);
  select(boardId, cardId);
  openEditor(boardId, cardId);
}

/** Resolve a card id to its board (links can point anywhere). */
export async function openCardById(cardId: string) {
  for (const [id, b] of boards) if (b.node(cardId)) return openCard(id, cardId);
  const loc = await rpc<{ board: string } | null>('search.locate', { id: cardId });
  if (loc) return openCard(loc.board, cardId);
}
