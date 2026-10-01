// History / trash / archive actions shared by the panel views and commands.
// Every mutation goes through `board.apply` (journaled + undoable) except the
// permanent trash deletions, which are not undoable by nature and always ask.

import { rpc } from "$lib/backend/rpc";
import type { TrashEntry } from "$lib/backend/types";
import {
  apply,
  boards,
  openBoard,
  undo,
  type BoardModel,
} from "$lib/state/boards.svelte";
import { registry } from "$lib/state/registry.svelte";
import { confirm } from "$lib/state/dialogs.svelte";
import { toast } from "$lib/state/toasts.svelte";
import { settings } from "$lib/settings/store.svelte";
import { t, i18n } from "$lib/i18n/index.svelte";
import { hist } from "./historyState.svelte";

export function boardName(id: string): string | null {
  return (
    registry.data.boards.find((b) => b.id === id)?.name ??
    boards.get(id)?.header.name ??
    null
  );
}

/** Title of a card on an open board (used when the journal has no titles). */
export function titleOf(boardId: string, id: string): string | null {
  return boards.get(boardId)?.node(id)?.title ?? null;
}

/** Boards the history/trash views can show (known and present on disk). */
export function knownBoards(): { id: string; name: string }[] {
  return registry.data.boards
    .filter((b) => !b.missing)
    .map((b) => ({ id: b.id, name: b.name }));
}

/** Date + time in the UI locale ('' for invalid timestamps). */
export function fmtDateTime(ts: string | number): string {
  const d = new Date(ts);
  return Number.isNaN(d.getTime())
    ? ""
    : d.toLocaleString(i18n.locale, {
        dateStyle: "medium",
        timeStyle: "short",
      });
}

export function trashTtl(): number {
  const v = Number(settings.get<number>("trash.ttlDays"));
  return Number.isFinite(v) && v >= 1 ? Math.floor(v) : 7;
}

const HASH_RE = /^[0-9a-f]{8,128}$/i;

/** Text of a stored version; `null` if it was pruned or the hash is malformed. */
export async function loadBlob(
  board: string,
  hash: string | null | undefined,
): Promise<string | null> {
  if (!hash) return "";
  if (!HASH_RE.test(hash)) return null;
  try {
    return await rpc<string>("history.blob", { board, hash });
  } catch {
    return null;
  }
}

async function writable(boardId: string): Promise<BoardModel | null> {
  try {
    const b = await openBoard({ id: boardId });
    if (b.header.readOnly) {
      toast.warn(t("history.diff.readOnly"));
      return null;
    }
    return b;
  } catch (e) {
    toast.error(t("errors.opFailed", { message: (e as Error).message }));
    return null;
  }
}

/** Replace a card's text with an older version, as one undoable operation. */
export async function restoreVersion(
  boardId: string,
  cardId: string,
  content: string,
  ts: string,
): Promise<boolean> {
  const b = await writable(boardId);
  if (!b) return false;
  const n = b.node(cardId);
  if (!n) {
    toast.warn(t("history.diff.missing"));
    return false;
  }
  const ok = await confirm({
    title: t("history.restore.confirmTitle"),
    message: t("history.restore.confirmMessage", { date: fmtDateTime(ts) }),
    confirmLabel: t("history.diff.restoreAfter"),
    cancelFocused: false,
  });
  if (!ok) return false;
  const r = await apply(
    boardId,
    { op: "writeCard", id: cardId, content },
    t("history.restore.op", { title: n.title || t("common.untitled") }),
  );
  if (!r) return false;
  toast.success(t("history.restore.done"), {
    action: { label: t("common.undo"), run: () => void undo(boardId) },
  });
  hist.refresh++;
  return true;
}

export async function listTrash(boardId: string): Promise<TrashEntry[]> {
  return rpc<TrashEntry[]>("trash.list", { board: boardId });
}

/** Restore trash entries (undoable board op). */
export async function restoreFromTrash(
  boardId: string,
  entries: TrashEntry[],
): Promise<boolean> {
  if (!entries.length) return false;
  const b = await writable(boardId);
  if (!b) return false;
  const r = await apply(
    boardId,
    { op: "restore", entries: entries.map((e) => e.id) },
    t("history.trash.restoreOp"),
  );
  if (!r) return false;
  const title = entries[0].title || t("common.untitled");
  toast.success(t("history.trash.restored", { title }), {
    action: { label: t("common.undo"), run: () => void undo(boardId) },
  });
  hist.refresh++;
  return true;
}

export async function deleteForever(
  boardId: string,
  entry: TrashEntry,
): Promise<boolean> {
  const ok = await confirm({
    title: t("history.trash.confirmDeleteTitle", {
      title: entry.title || t("common.untitled"),
    }),
    message: t("history.trash.confirmDeleteMessage"),
    confirmLabel: t("history.trash.deleteForever"),
    danger: true,
  });
  if (!ok) return false;
  try {
    const n = await rpc<number>("trash.delete", {
      board: boardId,
      ids: [entry.id],
    });
    toast.info(t("history.trash.deletedForever", { count: n }));
    hist.refresh++;
    return n > 0;
  } catch (e) {
    toast.error(t("errors.opFailed", { message: (e as Error).message }));
    return false;
  }
}

export async function emptyTrash(boardId: string): Promise<boolean> {
  let count = 0;
  try {
    count = (await listTrash(boardId)).length;
  } catch {
    /* the confirmation still makes sense without a count */
  }
  if (!count) {
    toast.info(t("history.trash.empty"));
    return false;
  }
  const ok = await confirm({
    title: t("history.trash.confirmEmptyTitle", {
      board: boardName(boardId) ?? "",
    }),
    message: t("history.trash.confirmEmptyMessage", { count }),
    confirmLabel: t("history.trash.emptyTrash"),
    danger: true,
  });
  if (!ok) return false;
  try {
    const n = await rpc<number>("trash.purge", { board: boardId, all: true });
    toast.info(t("history.trash.deletedForever", { count: n }));
    hist.refresh++;
    return true;
  } catch (e) {
    toast.error(t("errors.opFailed", { message: (e as Error).message }));
    return false;
  }
}

/** Apply the retention window now (on one board, or every known board). */
export async function cleanupTrash(
  boardIds: string[] = knownBoards().map((b) => b.id),
): Promise<number> {
  let total = 0;
  for (const board of boardIds) {
    try {
      total += await rpc<number>("trash.purge", { board, all: false });
    } catch {
      /* a board that is unavailable right now is skipped */
    }
  }
  toast.info(
    total
      ? t("history.trash.cleaned", { count: total })
      : t("history.trash.nothingExpired"),
  );
  hist.refresh++;
  return total;
}

export async function unarchive(
  boardId: string,
  cards: string[],
  lanes: string[],
): Promise<boolean> {
  if (!cards.length && !lanes.length) return false;
  const b = await writable(boardId);
  if (!b) return false;
  const r = await apply(
    boardId,
    {
      op: "setArchived",
      nodes: cards.map((id) => [id, false] as [string, boolean]),
      lanes: lanes.map((id) => [id, false] as [string, boolean]),
    },
    t("ops.unarchive"),
  );
  if (!r) return false;
  toast.success(
    t("history.archive.unarchived", { count: cards.length + lanes.length }),
    { action: { label: t("common.undo"), run: () => void undo(boardId) } },
  );
  return true;
}
