// History panel commands (auto-registered: `*.commands.ts`), its setting and card action.

import {
  History,
  Trash2,
  Archive,
  Eraser,
  Flame,
  RotateCcw,
  RefreshCw,
  Columns2,
} from "@lucide/svelte";
import type { Command } from "$lib/commands/registry.svelte";
import { contribute } from "$lib/contributions/registry.svelte";
import { settings } from "$lib/settings/store.svelte";
import { boards } from "$lib/state/boards.svelte";
import { toast } from "$lib/state/toasts.svelte";
import { activeCard, activeBoard } from "$lib/app/helpers";
import { pickOne, BACK } from "$lib/quickinput/qi.svelte";
import { t, relTime } from "$lib/i18n/index.svelte";
import type { TrashEntry } from "$lib/backend/types";
import { hist, showCardHistory, showHistoryView } from "./historyState.svelte";
import {
  cleanupTrash,
  emptyTrash,
  knownBoards,
  listTrash,
  restoreFromTrash,
} from "./actions";

function showCard(args?: { boardId?: string; id?: string }) {
  const ref =
    args?.boardId && args?.id
      ? { boardId: args.boardId, id: args.id }
      : (() => {
          const c = activeCard();
          return c ? { boardId: c.board.id, id: c.id } : null;
        })();
  if (!ref) {
    toast.info(t("history.actions.noCard"));
    return;
  }
  showCardHistory(
    ref.boardId,
    ref.id,
    boards.get(ref.boardId)?.node(ref.id)?.title ?? "",
  );
}

/** The active board, or ask which one. */
async function pickBoard(): Promise<string | null> {
  const b = activeBoard();
  if (b) return b.id;
  const list = knownBoards();
  if (list.length <= 1) return list[0]?.id ?? null;
  const id = await pickOne(
    list.map((x) => ({ label: x.name, value: x.id })),
    { placeholder: t("history.trash.pickBoard") },
  );
  return typeof id === "string" ? id : null;
}

async function restorePick() {
  const board = await pickBoard();
  if (!board) return;
  let list: TrashEntry[] = [];
  try {
    list = await listTrash(board);
  } catch {
    /* empty */
  }
  if (!list.length) {
    toast.info(t("history.trash.empty"));
    return;
  }
  const picked = await pickOne(
    list.map((e) => ({
      label: e.title || t("common.untitled"),
      description: t("history.trash.deleted", { when: relTime(e.deletedAt) }),
      value: e,
    })),
    { placeholder: t("history.trash.pickEntry") },
  );
  if (picked && picked !== BACK)
    await restoreFromTrash(board, [picked as TrashEntry]);
}

export const commands: Command[] = [
  {
    id: "history.showCard",
    title: "commands.history.showCard",
    category: "history",
    icon: History,
    run: showCard,
  },
  {
    id: "history.showTimeline",
    title: "commands.history.showTimeline",
    category: "history",
    icon: History,
    run: () => ((hist.card = null), showHistoryView("timeline")),
  },
  {
    id: "history.showTrash",
    title: "commands.history.showTrash",
    category: "history",
    icon: Trash2,
    run: () => showHistoryView("trash", activeBoard()?.id),
  },
  {
    id: "history.showArchive",
    title: "commands.history.showArchive",
    category: "history",
    icon: Archive,
    run: () => showHistoryView("archive", activeBoard()?.id),
  },
  {
    id: "trash.cleanup",
    title: "commands.history.cleanup",
    category: "history",
    icon: Eraser,
    run: () => cleanupTrash(),
  },
  {
    id: "trash.empty",
    title: "commands.history.emptyTrash",
    category: "history",
    icon: Flame,
    run: async () => {
      const b = await pickBoard();
      if (b) await emptyTrash(b);
    },
  },
  {
    id: "trash.restore",
    title: "commands.history.restoreFromTrash",
    category: "history",
    icon: RotateCcw,
    run: restorePick,
  },
  {
    id: "history.refresh",
    title: "commands.history.refresh",
    category: "history",
    icon: RefreshCw,
    run: () => hist.refresh++,
  },
  {
    id: "history.toggleDiffLayout",
    title: "commands.history.toggleDiffLayout",
    category: "history",
    icon: Columns2,
    run: () =>
      settings.set(
        "history.diffLayout",
        settings.get<string>("history.diffLayout") === "split"
          ? "inline"
          : "split",
      ),
  },
];

export function init() {
  settings.register([
    {
      key: "history.diffLayout",
      type: "enum",
      default: "inline",
      options: ["inline", "split"],
      category: "history",
    },
  ]);
  contribute("cardActions", {
    id: "history.showCard",
    label: () => t("history.actions.showCard"),
    icon: History,
    run: (c) => showCard({ boardId: c.boardId, id: c.id }),
  });
}
