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
