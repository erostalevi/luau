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

## C7 — Drag and drop: swallowed click, stuck drags, Escape side effect, tree "after" target (high)

- Files: `src/lib/board/dnd.svelte.ts`, `src/lib/commands/context.svelte.ts` (`dragging`),
  `src/lib/keybindings/resolver.svelte.ts`.
- The post-drag click guard is registered synchronously on pointerup (before the drop RPC) and removed on the next
  tick, so it only swallows the click the browser synthesizes for that pointerup — never a later real click.
- `pointercancel` and window `blur` (alt-tab, Mission Control, release outside the window) cancel the drag like
  Escape; no ghost or listeners are left behind.
- While dragging, the keybinding resolver is suspended (`ctx.dragging`), so Escape cancels the drag without also
  running `board.clearSelection`.
- Explorer tree "after" zone: when the next sibling row is itself being dragged, the next undragged sibling is used
  (dropping "before" a moving card failed and forced a reload).
- Tests: web build — drag a card across lanes, then click another card: the editor opens (was swallowed);
  `pnpm check`, vitest.

## C8 — Default bindings pointing at missing commands (high)

- Files: `src/lib/board/commands.ts` (`myName`, `card.assignSelf`), `src/lib/commands/builtin/boards.ts`
  (`board.filterMine`), `src/lib/settings/schema.ts` (`general.yourName`), i18n `en/es/pt` + `parts/prefs.*`,
  `src/lib/keybindings/defaults.test.ts` (new).
- The `remote.*` chords now resolve (commands added by the integrations merge). The Trello preset's
  `board.filterMine` (q) and `card.assignSelf` (space) were added: they use a new setting **Your @name**
  (asked once if empty, validated) — "Show my cards" opens the board filter with `@name`, "Assign to me" adds you
  to the card's `assignees` footer field (selection-aware, like "Assign…").
- Test: `defaults.test.ts` fails if any default or preset binding names a command that is not declared.

## D1 — Code-run trust was decided by the webview (medium, security)

- Files: `src-tauri/src/ai_rpc.rs` (`code.run`, `code.trust`), `crates/luau-core/src/ai/service.rs`
  (`code_set_trust` + tests), `src/lib/summaries/code.ts`, `src/lib/editor/Editor.svelte`.
- `code.trust` now requires a board id and, when granting, shows a **native** warning dialog (parented to the
  window, naming the board) that the page cannot fake or click; cancelling returns `cancelled`. Blanket `"*"`
  trust can no longer be granted (core rejects it).
- `code.run` parses `board`/`card` strictly (a malformed value is an error, never "no board"), requires `card`
  when a board is given, and checks that the code is actually present in that saved card — trusting board A
  no longer lets arbitrary code run by naming A. The editor saves before running a cell so the check sees it.
- Web build: the in-app confirm stays (mock backend).
- Tests: `trust_is_per_board_only`, updated `schedules_crud_and_trust`.

## D2 — Settings import could change security-sensitive settings; TTLs not clamped (medium)

- Files: `crates/luau-core/src/io/settings.rs` (`PROTECTED_KEYS`, `skipped`, test), `app/mod.rs` (TTL clamps),
  `src/lib/io/io.commands.ts`, `src/lib/i18n/parts/io.{en,es,pt}.ts`.
- A shared settings bundle can no longer set `editor.python` (would run that binary), `ai.endpoint`/`ai.provider`
  (card text sent elsewhere), `discovery.roots`, or the integrations push/pull/confirm/insecure guards. They are
  dropped by the core validator and reported in `skipped`; the UI keeps this computer's current values for them
  (instead of resetting to defaults) and shows how many were kept.
- `trash.ttlDays` and `files.unlinkedTtlDays` are clamped to 1–3650 days in the core, so `0` can no longer mean
  "purge/sweep everything immediately".
- Tests: `validates_bundles` extended (protected keys skipped, others imported).

## D3 — `luau://` protocol: internal-folder bypass, symlinks, memory/threads, thumbnails, Range (medium)

- Files: `crates/luau-core/src/app/files.rs` (`resolve_board_file`, `thumb_bucket`, tests),
  `src-tauri/src/protocol.rs` (`parse_range`, bounded bodies, CSP, tests), `src-tauri/src/lib.rs`.
- Paths are checked **after** normalization: no `.luau` or `.git` component at any depth (`%5C`/`.//` tricks no
  longer reach trash/history), and an existing path is canonicalized and must stay inside the board (symlinks
  pointing outside are refused).
- Responses: files ≤ 32 MiB are served whole; larger ones answer with an 8 MiB `206` chunk so media elements
  continue with ranges (no whole-file buffering). Range parsing supports `a-b`, `a-` and suffix `-n`, returns
  `416` with `Content-Range: bytes */len` when unsatisfiable and cannot overflow. Every response carries
  `Content-Security-Policy: default-src 'none'; … sandbox` and `nosniff` (SVG/HTML from boards are inert).
- Requests run on Tauri's bounded blocking pool instead of one OS thread each. Thumbnail widths are rounded up to
  nine buckets (64…2048), so the cache cannot grow with every distinct `?w=`.
- Tests: `internal_folders_and_escaping_links_are_refused`, `protocol::tests::ranges`.

## D4 — Board-type rules only checked at the top level; same-id copies replaced open boards (medium)

- Files: `crates/luau-core/src/app/mod.rs` (`check_kind_rules`, `open_board`), test in `app/tests.rs`,
  `src/lib/commands/builtin/boards.ts`, i18n `boards.duplicate*` (en/es/pt).
- `check_kind_rules` now validates `Op::Place` items and recurses into (nested) `Op::Batch`, so cards can no longer
  land at the root of a kanban board (and vanish on reload) or in lanes of a files board through those ops.
- `open_board` refuses a folder whose `board.json` has the id of a board that is already open from another folder
  (`duplicate_board_id`), instead of replacing the original store in memory. "Open folder" catches it and offers
  "Give this copy its own identity" (`board.reassignId`) and then opens the copy.
- Test: `kind_rules_cover_place_and_batches_and_copies_need_new_ids`.

## D5 — External edits could be lost (medium, data)

- Files: `crates/luau-core/src/store/mod.rs` (`FileSig`, `recently_touched`, `reload_external`,
  `forget_history_for`, `op_touches`), `app/watch.rs`, `app/mod.rs` (`write_card_checked`),
  `src-tauri/src/rpc.rs` (`card.write {base}`), `src/lib/editor/Editor.svelte`, test in `app/tests.rs`.
- Echo suppression: each path we write is recorded with the file's size+mtime right after the write. A watcher
  event for that file is our echo only if the file still matches (or is gone because we moved it); a real external
  save within the 2.5 s window is now picked up. Directory touches keep the parent/child rule.
- Watcher reloads (`reload_external`) drop undo/redo steps that involve the changed cards, so ⌘Z can no longer
  write an old version over an external edit. Internal reloads (import) keep history.
- `card.write` accepts `base` (the text the editor last loaded/saved). If the file on disk differs, the write is
  refused with `changed_on_disk`; the editor then shows the existing conflict banner (keep mine / take theirs).
  "Keep mine" writes without `base`. When the app itself rewrites the card to the same text the base is updated.
- Test: `external_edits_are_not_overwritten_or_undone_away`; suite 5× green.

## D6 — Basic card summaries were always empty (medium)

- File: `crates/luau-core/src/ai/service.rs` (`card_summarize` + test).
- Cause: the extractive summarizer used `node.meta.plain`, which the indexer drops from memory once a card is
  indexed; the empty result was then cached by content hash.
- Fix: plain text is derived from the card content (`markdown::parse`) on demand; the cache key is versioned
  (`v2|…`) so previously cached empty summaries are ignored.
- Test: `basic_card_summary_is_not_empty_after_indexing`.

## D7 — Editor/overlay focus issues (medium)

- Files: `src/lib/editor/Editor.svelte`, `src/lib/editor/CardEditorHost.svelte`, `src/lib/board/BoardView.svelte`,
  `src/lib/keybindings/resolver.svelte.ts`, `src/lib/commands/builtin/workspace.ts`, overlay roots
  (`Dialogs.svelte`, `ColorDialog.svelte`, `Cheatsheet.svelte`, `FirstRun.svelte` get `data-overlay`).
- False "changed outside" banner on card switch: the external-change effect ignores the first mtime it sees for a
  newly loaded card and skips while the view still belongs to the previous card.
- Esc in a confirm dialog no longer also closes the modal editor (host ignores Escape when it was already handled
  or an overlay/menu/quick input is open).
- Pinned sidebar editor can be closed: the "follow selection" effect no longer re-reads `open`, and a card the user
  closed stays closed until the selection moves to another card.
- Overlays own the keyboard: while a dialog, first-run, cheat sheet or color picker is open, global shortcuts are
  suspended (e.g. Backspace behind a confirm no longer deletes the selected card).
- `boardFocus` is cleared when a focused board unmounts (WebKit fires no focusout), so board single-key shortcuts
  cannot act from a doc tab.
- ⌘Z/⌘⇧Z with focus on a board tab act on that board even if the sidebar editor shows a card of another board.
- Tests: `pnpm check`, vitest. Manual check recommended in the desktop app (F1).

## D8 — Editor: dates converted in code/links, rich paste into code, plain paste, priority label (medium)

- Files: `src/lib/editor/cm/autocomplete.ts`, `src/lib/editor/cm/paste.ts`, `src/lib/editor/cm/widgets.ts`.
- Natural dates: `[label]` is converted when the **next** character is typed (or on Enter) instead of on `]`, so
  `[today](url)`, `[sat](…)` and reference links `[x][y]` stay links; never inside code (syntax tree, plus an
  unclosed backtick span on the line), never for images `![…]` or `[[card links]]`.
- Rich paste falls back to plain text inside code blocks and for HTML that is only `<pre>`/`<code>` or monospace
  markup (VS Code, terminals), so indentation is kept and `* _ #` are not escaped.
- "Paste as plain text" uses the native clipboard plugin in the desktop app (no WebKit paste bubble).
- Properties widget lowercases priority values (`priority: High` → "High" label, `p-high` style) and falls back to
  the raw value for unknown priorities instead of showing `priority.High`.
- Tests (web build): `[tomorrow]␠` → date chip; `[today](x)` stays a link; `` `arr[now]` `` unchanged.

## D9 — Shell: tab reorder, move to new window, dead links, opening attachments (medium)

- Files: `src/lib/state/workspace.svelte.ts` (+ `workspace.test.ts`), `src/lib/commands/builtin/workspace.ts`,
  `src/lib/app/bootstrap.ts`, `src/lib/editor/Editor.svelte`, `src/lib/commands/builtin/app.ts`, `src-tauri/src/rpc.rs`.
- Tab drag inside one pane landed one slot too far right (index computed before removing the tab) — fixed.
- "Move tab to new window" carries the whole tab (`?tab=<json>`: board, doc, settings, keybindings, saved search,
  summary); the new window validates ids/kinds and reopens it. The double `board.release` is gone. The backend
  only accepts URL-safe window queries (≤ 4 KiB).
- Links in rendered Markdown (AI summaries, embeds, previews) open in the system browser through one delegated
  click handler (http/https/mailto); the webview never navigates away.
- Opening attachments / "Open settings.json": new backend RPCs `file.open {board, rel}` (path resolved by the core
  inside the board — no `..`, no links out — then opened with the default app) and `config.open {settings|keybindings}`.
  No webview `opener:open-path` permission was added. A malformed `%` escape in a link is ignored instead of throwing.
- Tests: `workspace.test.ts`; manual check of opening files pending in F1.

## E1 — Rust low-priority items

- History: `history::prune` now runs in the background when a writable board opens (settings
  `history.retentionDays`, clamped 7–3650, and `history.maxMb`, 5–2000). A date-only `to` filter includes that
  whole day.
- Trash: emptying/expiring trash is journaled (`purge`, count only). Opening a read-only board (newer schema,
  damaged manifest, mirror) no longer purges its trash.
- Watcher: pending paths are de-duplicated and capped (10 000 per board), and a board is processed at the latest
  3 s after its first event, so a continuously written file can no longer starve it.
- Cross-board moves: the used token is removed after each move/undo/redo (the map no longer grows all session).
- Concurrency: `open_board` is serialized, so two windows opening the same folder load one store.
- Async hygiene: the code runner uses `tokio::fs` and probes interpreters on the blocking pool.
- Files: `atomic_write` updates symlinked files through the link and keeps the existing permissions; ZIP exports
  replace the old file with a single `rename` (no delete-then-rename window).
- Failed ops: when an op fails half-way, the store resyncs from disk (memory never diverges from the files).
- Files: `history.rs`, `app/mod.rs`, `app/watch.rs`, `io/archive.rs`, `store/ops.rs`, `fsutil.rs` (+ test),
  `ai/code.rs`, `ai/service.rs`.
- Not changed (documented): `card_summarize` still reads one card under the board lock inside an async fn — a
  bounded, sub-millisecond read.
- Tests: `atomic_write_keeps_links_and_permissions`; full suite 3× green.

## E2 — Accessibility: focus traps, Esc, submenus, "don't ask again"

- Files: `src/lib/components/focusTrap.ts` (new action), `Dialogs.svelte`, `ColorDialog.svelte`,
  `src/lib/views/Cheatsheet.svelte`, `src/lib/views/FirstRun.svelte`, `src/lib/quickinput/QuickInput.svelte`,
  `src/lib/components/ContextMenu.svelte`, i18n `common.quickInput`.
- `trapFocus`: focuses the surface (or a chosen element) on open, keeps Tab/Shift+Tab inside, restores focus to the
  previously focused element on close. Used by dialogs, color picker, cheat sheet, first-run and quick input.
- The cheat sheet now receives focus, so Esc closes it. First-run has an accessible name, focuses the primary
  button and Esc = Skip.
- Context menus: ArrowRight/Enter on a submenu item moves focus into the submenu (first item active); ArrowLeft
  returns to the parent menu.
- Dialogs: the "don't ask again" checkbox state is per dialog, never carried over to the next confirmation.
- Quick input's accessible label is translated.
- Tests (web build): cheat sheet focused → Esc closes; palette focus stays inside on Tab.

## E3 — Visual polish

- Files: `src/lib/views/settings/SettingRow.svelte`, `src/lib/panels/search/SearchPanel.svelte`,
  `src/lib/i18n/parts/panels.{en,es,pt}.ts`, `prefs.{en,es,pt}.ts`, `src/lib/editor/Editor.svelte`,
  `src/lib/editor/cm/widgets.ts`, `src/lib/markdown/render.ts`, `src/lib/app/bootstrap.ts`.
- Settings rows wrap: the control moves below the text when the description would get narrower than 15rem
  (Language and other segmented controls no longer squeeze their description).
- Settings category "Local AI & summaries" → "AI & summaries" (no truncation; es/pt too).
- Search panel: short placeholder ("Search…"); the syntax help is a two-column list (filter chip + what it does,
  translated); chips render without font ligatures (`>=` instead of `⩾`).
- Editor: soft skeleton while the editor bundle loads, and the bundle is preloaded when the app is idle — no more
  empty white sheet on first open.
- Mermaid diagrams re-render with the right theme after a light/dark switch (live preview and rendered views);
  rendered views keep the Inter font.
- `[[` card-link autocomplete was re-tested key by key and works (the earlier report came from the test tool
  typing whole strings).
- Tests: visual checks in the web build (light theme).

## E4 — Menus, untranslated strings, permissions, updater placeholder, logging

- Files: `src-tauri/src/menu.rs`, `windows.rs`, `lib.rs`, `rpc.rs`, `tauri.conf.json`, `capabilities/default.json`,
  `src/lib/i18n/index.svelte.ts`, `src/lib/shell/WindowControls.svelte`, `src/lib/markdown/render.ts`,
  `src/lib/editor/cm/widgets.ts`, `src/lib/commands/builtin/app.ts`, `src/lib/backend/types.ts`,
  `crates/luau-core/src/app/mod.rs`, i18n (`window.*`, `updates.notConfigured`).
- Native menu events with no focused window go to the **last focused** window (tracked on focus) instead of every
  window (no more duplicate "New card").
- Native menus are translated (en/es/pt) and rebuilt whenever the app language changes (`menu.setLocale`).
  Window-control buttons have translated accessible names.
- Least privilege: removed `clipboard-manager:allow-read-image` (unused). CSP `img-src` no longer allows plain
  `http:`; rendered and live-preview images use `referrerpolicy=no-referrer` (and `loading=lazy` when rendered).
- Updater: `app.info.updatesConfigured` is false while the endpoint is the `OWNER` template; "Check for updates"
  then says so instead of failing, and the startup check stays quiet (set the real repo in G1 / release).
- Logging (no secrets, no bodies): RPC failures as `method + code`, slow RPCs (> 1.5 s), "board opened"
  (id, card count, read-only reason, warnings) and "discovery finished" (boards, roots, ms).
- Tests: clippy, cargo test, svelte-check, vitest, i18n en/es/pt.
