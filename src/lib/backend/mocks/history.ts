// Browser mock for the History panel: a real trash (restore / delete forever /
// purge by TTL), text versions for edits (diff + restore) and a cross-board query.
// Wraps the base `board.apply` so the core mock stays untouched.

import type { MockApi } from "../mock";
import type { JournalEntry, LaneDto, Parent, TrashEntry } from "../types";

interface Node {
  id: string;
  parent: Parent;
  children: string[];
  content: string;
  archived: boolean;
  cover: unknown;
  mtime: number;
}
interface Board {
  id: string;
  lanes: LaneDto[];
  rootOrder: string[];
  nodes: Map<string, Node>;
  version: number;
  journal: JournalEntry[];
}
interface Stored {
  entry: TrashEntry;
  nodes: Node[];
  lane?: LaneDto;
}

const DAY = 86_400_000;
const titleOf = (content: string) =>
  /^#\s+(.+)$/m.exec(content)?.[1]?.trim() ??
  content.split("\n")[0]?.trim() ??
  "";

export function register(
  methods: Record<string, (p: Record<string, any>) => unknown>,
  api: MockApi,
) {
  const trash = new Map<string, Stored[]>();
  const blobs = new Map<string, string>();
  let n = 0;
  const hashOf = (text: string) => {
    const h = (++n).toString(16).padStart(8, "0") + "a0b1c2d3";
    blobs.set(h, text);
    return h;
  };
  const board = (id: string) => api.boards.get(id) as Board;
  const list = (b: Board, p: Parent): string[] =>
    p.kind === "lane"
      ? (b.lanes.find((l) => l.id === p.id)?.order ?? [])
      : p.kind === "card"
        ? (b.nodes.get(p.id)?.children ?? [])
        : b.rootOrder;
  const subtree = (b: Board, id: string): Node[] => {
    const node = b.nodes.get(id);
    return node ? [node, ...node.children.flatMap((c) => subtree(b, c))] : [];
  };
  const clone = (x: Node): Node => ({ ...x, children: [...x.children] });
  const tid = () => "t" + Math.random().toString(36).slice(2, 8).padEnd(6, "0");
  const ttl = () => {
    const v = Number(
      (api.ls("settings", {}) as Record<string, unknown>)["trash.ttlDays"],
    );
    return Number.isFinite(v) && v >= 1 ? v : 7;
  };
  const delta = (b: Board) => {
    const s = api.snapshot(b.id);
    return {
      boardId: b.id,
      version: b.version,
      header: s.header,
      lanes: s.lanes,
      rootOrder: s.rootOrder,
      nodes: s.nodes,
      removed: [],
    };
  };

  const baseApply = methods["board.apply"];
  methods["board.apply"] = (p) => {
    const b = board(p.board);
    const op = p.op as { op: string; [k: string]: any };
    const now = new Date().toISOString();
    const stored: Stored[] = [];
    let beforeText: string | null = null;
    let details: Record<string, unknown> | undefined;

    if (b && op.op === "trash") {
      for (const id of op.nodes as string[]) {
        const node = b.nodes.get(id);
        if (!node) continue;
        const sub = subtree(b, id).map(clone);
        const title = titleOf(node.content);
        stored.push({
          entry: {
            id: tid(),
            kind: "node",
            itemId: id,
            title,
            parent: node.parent,
            index: list(b, node.parent).indexOf(id),
            isGroup: node.children.length > 0,
            count: sub.length,
            deletedAt: now,
            boardId: b.id,
          },
          nodes: sub,
        });
      }
      for (const k of op.lanes as string[]) {
        const lane = b.lanes.find((l) => l.id === k);
        if (!lane) continue;
        const sub = lane.order.flatMap((c) => subtree(b, c)).map(clone);
        stored.push({
          entry: {
            id: tid(),
            kind: "lane",
            itemId: k,
            title: lane.name,
            parent: null,
            index: b.lanes.indexOf(lane),
            isGroup: false,
            count: lane.order.length,
            deletedAt: now,
            boardId: b.id,
          },
          nodes: sub,
          lane: { ...lane, order: [...lane.order] },
        });
      }
      details = {
        before: {
          items: stored
            .filter((s) => s.entry.kind === "node")
            .map((s) => ({ id: s.entry.itemId, title: s.entry.title })),
          lanes: stored.filter((s) => s.lane).map((s) => s.entry.title),
        },
      };
    } else if (b && op.op === "writeCard") {
      const node = b.nodes.get(op.id);
      beforeText = node?.content ?? null;
    } else if (b && op.op === "setArchived") {
      const items = (op.nodes as [string, boolean][]).map(([id, a]) => ({
        id,
        title: titleOf(b.nodes.get(id)?.content ?? ""),
        archived: a,
      }));
      const lanes = (op.lanes as [string, boolean][]).map(([id, a]) => ({
        lane: b.lanes.find((l) => l.id === id)?.name ?? "",
        archived: a,
      }));
      details = { after: { items, lanes } };
    }

    const res = baseApply(p) as {
      version: number;
      created: string[];
      trashed: string[];
    };
    if (!b) return res;
    const j = b.journal[0];

    if (op.op === "trash" && stored.length) {
      trash.set(b.id, [...stored, ...(trash.get(b.id) ?? [])]);
      res.trashed = stored.map((s) => s.entry.id);
    } else if (op.op === "restore") {
      const all = trash.get(b.id) ?? [];
      const picked = all.filter((s) =>
        (op.entries as string[]).includes(s.entry.id),
      );
      for (const s of picked) {
        for (const node of s.nodes) b.nodes.set(node.id, clone(node));
        if (s.lane)
          b.lanes.splice(Math.min(s.entry.index, b.lanes.length), 0, {
            ...s.lane,
            order: [...s.lane.order],
          });
        else if (s.entry.parent) {
          const target = list(b, s.entry.parent);
          target.splice(
            Math.min(Math.max(0, s.entry.index), target.length),
            0,
            s.entry.itemId,
          );
        }
      }
      trash.set(
        b.id,
        all.filter((s) => !picked.includes(s)),
      );
      details = {
        after: {
          items: picked.map((s) => ({
            id: s.entry.itemId,
            title: s.entry.title,
          })),
        },
      };
      b.version++;
      api.emit({ type: "boardDelta", delta: delta(b) } as never);
    } else if (op.op === "writeCard" && beforeText !== null) {
      const after = b.nodes.get(op.id)?.content ?? "";
      if (j) {
        j.kind = "edit";
        j.before = hashOf(beforeText);
        j.after = hashOf(after);
        details = {
          after: { items: [{ id: op.id, title: titleOf(after) }] },
          chars: after.length - beforeText.length,
        };
      }
    } else if (op.op === "createCard") {
      details = {
        after: { items: [{ id: op.id, title: titleOf(op.content ?? "") }] },
      };
    }
    if (j && details) {
      j.details = details;
      j.ids = [
        ...new Set([
          ...(j.ids ?? []),
          ...(
            (details.after as any)?.items ??
            (details.before as any)?.items ??
            []
          ).map((i: { id: string }) => i.id),
        ]),
      ];
    }
    return res;
  };

  methods["history.blob"] = (p) => {
    const v = blobs.get(String(p.hash));
    if (v === undefined) throw new Error("blob pruned");
    return v;
  };

  const query = (b: Board, f: Record<string, any> = {}) =>
    b.journal.filter(
      (e) =>
        (!f.from || e.ts >= f.from) &&
        (!f.to || e.ts <= f.to) &&
        (!f.ids?.length || e.ids.some((id) => f.ids.includes(id))),
    );
  methods["history.query"] = (p) =>
    query(board(p.board), p.filter).slice(0, p.filter?.limit ?? 200);
  methods["history.queryAll"] = (p) => {
    const ids: string[] = p.boards?.length ? p.boards : [...api.boards.keys()];
    return ids
      .flatMap((id) => (api.boards.has(id) ? query(board(id), p.filter) : []))
      .sort((a, b) => b.ts.localeCompare(a.ts))
      .slice(0, p.filter?.limit ?? 200);
  };

  methods["trash.list"] = (p) => (trash.get(p.board) ?? []).map((s) => s.entry);
  methods["trash.delete"] = (p) => {
    const ids: string[] = p.ids ?? [];
    const all = trash.get(p.board) ?? [];
    const keep = all.filter((s) => !ids.includes(s.entry.id));
    trash.set(p.board, keep);
    return all.length - keep.length;
  };
  methods["trash.purge"] = (p) => {
    const all = trash.get(p.board) ?? [];
    const cutoff = Date.now() - ttl() * DAY;
    const keep = p.all
      ? []
      : all.filter((s) => Date.parse(s.entry.deletedAt) > cutoff);
    trash.set(p.board, keep);
    return all.length - keep.length;
  };
}
