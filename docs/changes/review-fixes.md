# Review fixes (2026-10-01 macOS build review)

Objective: fix every finding of the 2026-10-01 review, most severe first, plus the
rename to Luau. Each section lists files, logic, decisions, risks and tests.

## A0 — Merge integrations and wire Slack delivery for scheduled summaries

- Files: `crates/lull-core/src/integrations/service.rs` (`register_slack_sender`, called from
  `start_watcher`), `src-tauri/src/ai_rpc.rs` (`ai.slackConnected` now also requires a Slack account).
- Logic: the AI scheduler calls `ai::send_to_slack`; the registered sender picks the first Slack
  account, checks the push toggle (`integrations.allowPush`) and posts from a fresh thread with its
  own current-thread runtime (the caller may already be inside a runtime).
- Decision: scheduled delivery is configured explicitly by the user, so it skips the interactive
  confirmation dialog but still honours the global push toggle. If push is off, the schedule records
  `push_disabled` as the delivery result.
- Tests: `cargo test --workspace` (186), `pnpm check`, vitest (142), i18n en/es/pt.
