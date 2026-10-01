# History panel, diff/restore, trash and archive views

## Objective

SPEC §8.3, §12.1, §12.2: a sidebar History panel with a readable timeline, version
diffs with restore, a per-board Trash (TTL, restore, delete forever, empty, cleanup)
and an Archive view (unarchive cards/lanes).

## Files

Feature (new):
- `src/lib/panels/history/HistoryPanel.svelte` — view switcher (Timeline / Trash / Archive), diff overlay.
- `Timeline.svelte` — timeline grouped by day; filters: board, range, text, kind groups, source/origin; card focus; context menu.
- `DiffView.svelte` — inline or side-by-side diff, "this change" vs "compared with now", prev/next version (Alt+↑/↓), copy as unified diff, restore.
- `TrashView.svelte`, `ArchiveView.svelte`.
- `model.ts` (+ `model.test.ts`) — classify, describe (human titles, never ids), group by day, filter, `daysLeft`.
- `diff.ts` (+ `diff.test.ts`) — Myers line diff, markdown-aware word diff, hunks, side-by-side pairing, collapse.
- `actions.ts`, `historyState.svelte.ts`, `icons.ts`, `history.commands.ts`.
- `src/lib/i18n/parts/history.{en,es,pt}.ts`, `src/lib/backend/mocks/history.ts`.
- `crates/lull-core/src/app/trash_ops.rs` — `Core::trash_delete` (+ test).
- `src-tauri/src/history_rpc.rs` — `trash.delete`.

Shared files (one-line additions): `crates/lull-core/src/app/mod.rs` (`mod trash_ops;`),
`src-tauri/src/lib.rs` (`mod history_rpc;`), `src-tauri/src/rpc.rs` (dispatch chain),
`src/lib/backend/types.ts` (`TrashEntry.archived?`, `cover?`, already serialized by Rust).

## Logic

- Timeline: `history.queryAll` (one board or all; card focus searches all boards because cards
  move across boards). Kind/source/text filters are client-side. Reloads 1.2 s after `boardDelta`/`externalChange`.
- Descriptions: `describeEntry` maps `kind + details.before/after` (titles, lanes captured by the
  core journal) to `history.desc.*` keys. Falls back to the open board's titles, never shows ids.
- Diff: blobs via `history.blob` (hash validated client-side, `null` when pruned). Word-level highlight
  only for paired lines with similarity >= 0.35; wiki links, links, code, URLs, tags and mentions are atomic tokens.
- Restore version = `board.apply { op: writeCard }` → journaled and undoable (toast with Undo), after a confirm.
- Trash: `trash.list`; restore = `board.apply { op: restore }` (undoable); delete forever = `trash.delete`
  (new, confirm, danger); empty = `trash.purge { all: true }` (confirm); cleanup = `trash.purge { all: false }`
  on every known board (`trash.cleanup` command). Days left = `ceil(deletedAt + ttl − now)`.
- TTL: existing setting `trash.ttlDays` (default 7, 1–365, Settings › Files) editable inline in the Trash view;
  the core also purges expired entries when a board opens. New setting `history.diffLayout` registered via `settings.register` in `init()`.
- Archive: archived lanes/cards from the open `BoardModel`; unarchive = `board.apply { op: setArchived }` (undoable).

## Commands

`history.showCard` (also a card action through `contribute('cardActions')`), `history.showTimeline`,
`history.showTrash`, `history.showArchive`, `trash.cleanup`, `trash.empty`, `trash.restore` (quick pick),
`history.refresh`, `history.toggleDiffLayout`.

## Decisions

- Permanent deletion is the only non-undoable action; it always asks and is journaled as kind `purge`
  with titles and ids only (auditable, no card bodies).
- `trash.delete` rejects malformed ids (path traversal guard) and skips unknown ones.
- Did not add a new TTL setting: `trash.ttlDays` already existed in the schema and core.

## Risks / pending

- History blobs pruned by retention show "version no longer available".
- Mock undo does not roll back the mock trash list (browser-only).
- An "activity summary" button appears only if another feature registers `activity.summarize`/`summary.*`.

## Tests

- `npx vitest run` (108 passed, incl. `diff.test.ts`, `model.test.ts`), `pnpm check` 0/0, `pnpm i18n en|es|pt` parity.
- `cargo test --workspace`, `cargo clippy --workspace --all-targets -D warnings`, `cargo fmt --all`.
- Manual in `pnpm dev:web`: edit → diff with word highlights; trash → restore; archive → unarchive.
