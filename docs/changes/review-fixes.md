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

## C2 — File RPCs accepted any path from the webview (high)

- Files: `src-tauri/src/grants.rs` (new), `src-tauri/src/dialogs.rs` (new), `src-tauri/src/rpc.rs`,
  `src-tauri/src/exports.rs`, `src-tauri/src/ai_rpc.rs`, `src-tauri/src/lib.rs`, `src-tauri/capabilities/default.json`,
  `src/lib/app/helpers.ts`, `src/lib/editor/Editor.svelte`.
- Removed unused, unrestricted RPCs: `json.read`, `json.write`, `text.write`, `bytes.write`.
- New backend dialogs `dialog.pickFolder`, `dialog.pickFiles`, `dialog.save` (tauri-plugin-dialog, parented to the
  calling window). Every path the user picks is recorded as a **grant** (folder = subtree, file = exact path).
  The JS dialog permissions `dialog:allow-open/save` were removed, so the webview cannot open pickers itself.
- Gated RPCs (`grants::require`): `logs.export`, `board.create`, `board.open {path}`, `attachment.add {path}`,
  `io.export`, `io.inspect`, `io.import` (source + destination), `board.createFromTemplate`, `settings.export`,
  `settings.import`, `summaries.export`. Allowed: granted paths, the app's data/config/logs dirs, and (for reads
  and board operations) folders of registered boards. Paths are normalized lexically; anything that still contains
  `..` or is relative is rejected. `board.reassignId` accepts an existing board folder (discovered duplicates).
- Not gated (low risk, documented): `path.exists` (existence only).
- Tests: `grants::tests::normalization_and_subtrees`; clippy; svelte-check. Manual native-dialog test pending in F1.
- Risk: a flow that passes a path not obtained from a picker now fails with `forbidden_path` (toast). The web
  build is unaffected (mock backend).

## C3 — Concurrent undo/redo could panic; failed undo lost its step (high)

- Files: `crates/luau-core/src/app/mod.rs` (`undo`, `redo`), `store/mod.rs` (`BoardStore::undo/redo`),
  `error.rs` (`Error::is_transient`), tests in `app/tests.rs`.
- Cause: the cross-board branch peeked and popped under two separate locks (`take_undo().unwrap()`,
  `unreachable!()`), so two concurrent ⌘Z could panic — fatal with `panic = "abort"`. Entries were popped before
  applying, so any failure lost the step.
- Fix: peek+pop under one lock, no `unwrap`/`unreachable`. On failure the entry is pushed back when the error is
  transient (I/O, read-only, other); permanent errors (not found, conflict, invalid — e.g. the trash entry was
  purged) drop the step so undo is never stuck on it.
- Tests: `concurrent_undo_redo_never_panics` (6 threads × 30 undo/redo), `failed_undo_keeps_its_step_when_transient`.

## C4 — Unlinked-attachment sweep moved attachments still in use (high)

- File: `crates/luau-core/src/app/files.rs` (`sweep_unlinked`), test in `app/tests.rs`.
- Cause: "referenced" only meant Markdown link/image targets in the owning card, missing raw HTML (`<img src>`,
  `<video>`), `luau://` URLs and references from other cards; unreadable (cloud-evicted) cards looked empty.
- Fix: when at least one attachment looks unlinked, every card is read once and an attachment is kept if its file
  name appears anywhere (plain, percent-encoded or with `%20`). If any card cannot be read, nothing is swept that
  run. Boards without candidates skip the extra reads.
- Tests: `sweep_keeps_attachments_referenced_by_html_luau_urls_or_other_cards` (HTML, encoded luau URL and
  cross-card link kept; truly unused file moved); the existing TTL test still passes.

## C5 — Card operations got slower as boards grew (high, performance)

- Files: `crates/luau-core/src/store/mod.rs` (`Changes::reindex_lanes`), `store/ops.rs`, `store/trash.rs`,
  `app/mod.rs` (`index_changes`), `fsutil.rs` (`flush_to_disk`), `Cargo.toml` (`libc`), test in `store/tests.rs`.
- Causes: (1) any create/move/trash inside a lane set `Changes.lanes`, which `index_changes` treated as "lane
  renamed" and re-read + re-indexed **every card** under the board lock (O(n) per op); (2) each `atomic_write`
  called `File::sync_all`, which on macOS is `F_FULLFSYNC` (full drive-cache flush), twice per write.
- Fix: new `reindex_lanes` flag set only by lane rename/flag updates, lane archive, lane restore, kind change
  and external reloads with lane diffs; moved/created cards are already in `Changes.nodes`. On Apple platforms
  `atomic_write` uses plain `fsync(2)` for the temp file and the directory (ordering before rename — the same
  trade-off SQLite makes by default); other platforms keep `sync_all`.
- Measured (release, out-of-repo harness, 2000 cards): create **48 ms → 0.46 ms per card** (97 s → 0.9 s),
  move 500 cards 209 → 137 ms, undo 264 → 188 ms; reopen 32 ms, search 2–3 ms.
- Risk: plain fsync on macOS does not force the drive cache on power loss; atomic rename still guarantees no
  partial files, at worst the last save is lost.
- Tests: `only_lane_metadata_changes_request_a_full_reindex`; full suite.
- Follow-up (found while verifying C5): search hits right after opening a board could show the board id instead
  of its name, because the board row in the search DB was only written by the background re-index.
  `open_board` now records the board name synchronously. Full suite run 10× without failures.

## C6 — Keyboard: layouts, AltGr, stuck recording, shortcuts hijacking the editor/buttons (high)

- Files: `src/lib/keybindings/keys.ts` (+ `keys.test.ts`), `src/lib/keybindings/defaults.ts`,
  `src/lib/commands/context.svelte.ts`, `src/lib/views/settings/KeybindingsView.svelte`.
- Letters now follow the active layout (`e.key`), so ⌘Z is Z on AZERTY/QWERTZ; non-Latin output (⌥ on macOS,
  Cyrillic…) falls back to the physical key. Digits/punctuation/named keys still use `e.code`.
- AltGr (Ctrl+Alt on Windows/Linux) producing a character is never treated as a shortcut (`@ # | { }` on es/de/pl).
- `kb.recording` is reset when the keybinding editor unmounts, and ending a key edit keeps "record keys" search
  state instead of always clearing it — shortcuts can no longer stay globally suspended.
- New context key `buttonFocus` (button/link/select focused outside a card): Enter/Space/Tab/Shift+Tab keep their
  native meaning instead of opening/peeking/indenting cards.
- Editor keys no longer hijacked: ⌘[ / ⌘] (indent) and ⌥⇧↑/↓ (copy line) only navigate cards outside the editor;
  new in-editor alternatives: macOS ⌃- / ⌃⇧-, Windows/Linux Alt+←/→. `mod+shift+l` (orientation) is not active in
  the editor (select all matches). Emacs preset `alt+x` and `ctrl+x ctrl+f` are disabled while typing (Cut and ≈).
- Tests: `keys.test.ts` (AZERTY, QWERTZ, ⌥N, Cyrillic, digits, AltGr, real Ctrl+Alt); vitest total passes.
