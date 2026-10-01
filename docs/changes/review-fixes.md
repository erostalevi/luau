# Review fixes (2026-10-01 macOS build review)

Objective: fix every finding of the 2026-10-01 review, most severe first, plus the
rename to Luau. Each section lists files, logic, decisions, risks and tests.

## A0 — Merge integrations and wire Slack delivery for scheduled summaries

- Files: `crates/luau-core/src/integrations/service.rs` (`register_slack_sender`, called from
  `start_watcher`), `src-tauri/src/ai_rpc.rs` (`ai.slackConnected` now also requires a Slack account).
- Logic: the AI scheduler calls `ai::send_to_slack`; the registered sender picks the first Slack
  account, checks the push toggle (`integrations.allowPush`) and posts from a fresh thread with its
  own current-thread runtime (the caller may already be inside a runtime).
- Decision: scheduled delivery is configured explicitly by the user, so it skips the interactive
  confirmation dialog but still honours the global push toggle. If push is off, the schedule records
  `push_disabled` as the delivery result.
- Tests: `cargo test --workspace` (186), `pnpm check`, vitest (142), i18n en/es/pt.

## A1 — Rename Lull → Luau, "Hawaii sunset" default palette, new icon

- Files: every tracked file mentioning lull/Lull/LULL/MyKanban (≈140), `crates/lull-core` → `crates/luau-core`,
  `Cargo.lock`, `src/styles/tokens.css`, `src/lib/settings/schema.ts`, `src/lib/theme/theme.svelte.ts`,
  `src/lib/components/ColorPicker.svelte`, `src/lib/state/colorDialog.svelte.ts`, `src/lib/views/StartPage.svelte`,
  `assets/icon.png`, `src-tauri/icons/*`.
- Renames: crate `luau-core` / `luau_core`, app crate `luau` / `luau_lib`, bundle id `app.luau.desktop`, URI scheme
  `luau://`, board marker folder `.luau/`, interchange format `luau-interchange`, settings bundle `luau-settings`,
  dev hook `window.__luau`, mock storage `luau.mock.*`, demo Jira project key `LUAU`.
- Palette: light = warm sand backgrounds, coral primary (`#ef8a7c` default, derived shades at runtime), peach
  secondary (`#fdeadc`), plum-tinted ink and shadows; dark = "Hawaii dusk" deep plum with coral/lavender highlights.
  New `--sunset` gradient token (mango → coral → hibiscus → lavender) used on the start-page greeting. Swatches now
  start with the sunset colours.
- Icon: rendered with a CoreGraphics script (sunset sky, sun, sea with reflections) → `tauri icon` set
  (mobile icon folders removed).
- Decisions: no migration from `.lull/` (no real boards exist yet). The repo folder itself is still named
  `mykanban` on disk; rename it when publishing (see G1).
- Risk: the updater key path in `release.yml` is now `~/.tauri/luau-updater.key`; rename the local key file:
  `mv ~/.tauri/lull-updater.key ~/.tauri/luau-updater.key` (and `.pub`).
- Tests: cargo test (186), clippy, svelte-check, vitest (142), i18n; visual check light/dark in the web build.

## B1 — Deadlock after ~60 s of continuous typing (critical)

- File: `crates/luau-core/src/app/mod.rs`, test in `app/tests.rs` (`long_edit_session_flush_does_not_deadlock`).
- Cause: `Core::apply` holds the board mutex and calls `note_edit`; when the pending edit session was older than
  60 s, `note_edit` → `write_edit` called `self.board(..).lock()` on the same non-reentrant `parking_lot::Mutex`.
- Fix: `note_edit` now receives the board root from the caller and journals through the new lock-free
  `write_edit_at(root, …)`. `write_edit` (used by flush paths that do not hold the lock) resolves the root and
  delegates. Other `journal_entry` callers were checked: they drop the board lock first.
- Tests: regression test back-dates a pending edit by 61 s and asserts the next `apply` returns within 10 s and
  journals an `edit` entry. Reproduced before the fix with the out-of-repo harness (edit #60 hung).

## B2 — Trash purge/restore trusted ids from `entry.json` (critical)

- File: `crates/luau-core/src/store/trash.rs` (+ `safety_tests`).
- Cause: `purge` and `restore` built paths from `entry.id` / `entry.item_id` read from `entry.json`, without
  validation; `join` with an absolute path replaces the base, so a crafted board deleted arbitrary folders on open.
  Unparseable `deletedAt` counted as expired.
- Fix: `read_entry` requires the folder name to be a valid trash id, the entry's `id` to equal the folder name and
  `itemId` to be a valid card/lane id (orphans excepted). `list` skips invalid entries and refuses a symlinked
  trash folder. An unparseable timestamp is now *not* expired (late deletion is reversible, early is not).
  `remove_path` already uses `symlink_metadata`, so links are removed, never followed.
- Tests: absolute id, `../` id, mismatching id, non-id folder, bad item id and garbage timestamp; the victim
  folder outside the board survives `purge(all=true)`.

## B3 + B4 — Editor lost edits on close and could save into the wrong card (critical)

- File: `src/lib/editor/Editor.svelte`.
- Causes: (B3) `onDestroy` nulled `view` before the async `flushAndSeal()` reached `save()`, which then bailed out;
  (B4) `save()` used the live `boardId`/`cardId` props, which already point at the next card while the old
  document is still in `view`.
- Fix: every `EditorView` is registered in a `WeakMap<View, Target>` with the card it was loaded for.
  `save`, `resolveTitleLinks` and `flushAndSeal` take the view explicitly and resolve the card from that map;
  `onDestroy` captures the view, flushes it, then destroys it. The blur handler and `setActiveEditor` use the
  view's own target too.
- Tests (manual, web build): type then Esc immediately → saved (was lost); type in card A then
  `card.openNext` immediately → text only in card A, editor shows card B. `pnpm check` clean.
- Recommended: a Playwright e2e test for both flows once e2e tooling exists.

## B5 — Loading rewrote a damaged `board.json` (critical)

- Files: `crates/luau-core/src/store/mod.rs` (`salvage_manifest`, `save_recovered_copy`, `CORRUPT_MANIFEST`),
  `discovery.rs` (`read_marker` salvages), `io/upgrade.rs` (`Core::repair_manifest` + test), `src-tauri/src/exports.rs`
  (`board.repair`), `src/lib/io/io.commands.ts` (`board.repair` command), `src/lib/board/BoardView.svelte` (banner),
  `src/lib/i18n/parts/io.{en,es,pt}.ts`.
- Before: a parse failure wrote a fresh manifest (name, kind, lane order, colors, view lost; random id if the id was
  unreadable) — even from read-only paths (watcher reload, inspect, rescan). `open_board` failed with "not a board".
- Now: loading never writes. The board opens **read-only** (`corrupt_manifest`) from a salvage: id/name/type read
  with regexes when possible (id otherwise derived from the path, stable across reloads), kind inferred from the
  layout, lanes from the lane folders on disk. One copy of the damaged file per distinct content is kept in
  `.luau/cache/recovered/` (also fixes the pile-up of recovered copies for any damaged JSON). A banner offers
  **Repair**, which (after confirmation) rewrites the manifest from the salvaged state and reopens the board.
- Tests: `damaged_manifest_opens_read_only_without_rewrite_and_repairs` (truncated file stays byte-identical,
  id/name/lanes salvaged, one recovered copy after two loads, repair makes it writable).

## C1 — Release build blocked by a Tauri version mismatch; DMG step

- Files: `package.json`, `pnpm-lock.yaml` (`@tauri-apps/plugin-opener` → ^2.7.0), `src-tauri/Cargo.toml` (Tauri
  crates pinned to the same major.minor as their npm packages), `scripts/check-tauri-versions.mjs` (new,
  `pnpm check:tauri`), `.github/workflows/ci.yml` (runs it), `scripts/build.mjs`.
- Cause: crates were declared as `"2"`, so a lockfile refresh pulled `tauri-plugin-opener` 2.7 while npm stayed on
  2.6; `tauri build` refuses mismatched major.minor.
- DMG: `bundle_dmg.sh` styles the DMG window through Finder AppleScript, which fails without automation
  permission (sandboxed shells, SSH). Verified: `CI=true` (plain DMG) builds a 9.8 MB DMG. `build.mjs` now retries
  with a plain DMG automatically, and builds without updater artifacts when `TAURI_SIGNING_PRIVATE_KEY` is unset
  (local builds no longer need the signing key).
- Tests: `pnpm tauri build` passes the version check without `--ignore-version-mismatches`; `pnpm check:tauri`
  passes; `node scripts/build.mjs --localtarget --dry-run`.
