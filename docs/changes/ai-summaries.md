# Local AI activity summaries, schedules, card summaries, code runner, link previews

## Objective
SPEC §12.3 (activity summary), §13 (card summaries) and §5.5 (code cells, link previews):
summarize what happened on the boards over a period, optionally with a **local** LLM
(Ollama or an OpenAI-compatible server on localhost), with a deterministic fallback;
run summaries on a schedule; run Python cells; show safe link previews.

## Files
Rust core (`crates/luau-core/src/ai/`, feature-owned):
- `mod.rs` — module layout + Slack hook: `pub type SlackSender`, `set_slack_sender`,
  `clear_slack_sender`, `slack_connected`, `send_to_slack` (RwLock; no Slack client here).
- `stage.rs` — lane name → `Done / InProgress / ToDo / Other` (en/es/pt keywords,
  case/accent-insensitive, whole words, ignores emoji and numbering); Jira status category.
- `facts.rs` — journal entries + current board views → `ActivityFacts` (completed,
  started, created, net moves, edit sessions, deleted, archived, external, pending, overdue).
- `prompt.rs` — facts → chat messages (detail 1–5, custom prompt; notes treated as data).
- `render.rs` — deterministic Markdown renderer (en/es/pt via `locale.rs`), intent detection
  for "pending / overdue / priorities" prompts, `plain_text`.
- `llm.rs` — `AiConfig` from `ai.*` settings; endpoint validation (http only for loopback,
  else https); Ollama `/api/chat` + `/api/tags` + `/api/pull`, OpenAI-compatible
  `/v1/chat/completions` + `/v1/models`; streaming, output cap, timeouts.
- `schedule.rs` — schedule model, validation, `next_run`, `due_at`, `period_range` (pure).
- `scheduler.rs` — background thread (30 s tick, one catch-up for missed runs), `due_ids` (pure).
- `store.rs` — app-data JSON: `ai/schedules.json`, `ai/summaries/<id>.json` (max 200),
  code trust list, caches (`cardsum`, `code`, `previews`) keyed by SHA-256.
- `code.rs` — `python3` subprocess (no shell, temp cwd, scrubbed env, timeout, output/image caps).
- `web.rs` — link previews with SSRF protection.
- `service.rs` — `impl Core`: facts, summarize (streams `summary.chunk`, `summary.progress`),
  card summaries, schedules CRUD/run, code trust/run/cache, previews.

Tauri: `src-tauri/src/ai_rpc.rs` (glue), **one line** in `src-tauri/src/lib.rs` setup:
`ai_rpc::start_scheduler(&handle, core.clone());`.

Frontend:
- `src/lib/summaries/api.ts` (typed client), `range.ts` (+ `range.test.ts`),
  `summaries.commands.ts`, `SchedulesEditor.svelte`, `code.ts` (trust prompt + retry).
- `src/lib/views/SummaryView.svelte` — presets, boards, detail slider, prompt, writer
  (auto / AI / basic), streaming + progress, copy (Markdown / text / Slack), export .md,
  past summaries, AI status chip; second tab: schedules editor.
- `src/lib/backend/mocks/ai.ts`; `src/lib/i18n/parts/summaries.{en,es,pt}.ts`.
- `src/lib/editor/Editor.svelte` — `runCode` now calls `runCodeCell` (1-line change).

## RPC methods
async: `ai.status`, `ai.models`, `ai.pullModel` (events `ai.pull`), `activity.summarize`,
`card.summarize`, `schedules.runNow`, `code.run`, `web.preview`.
sync: `activity.facts`, `schedules.list|save|delete`, `summaries.list|get|delete|export`,
`code.cached`, `code.trusted`, `code.trust`, `ai.slackConnected`.
Events (`CoreEvent::Custom`): `summary.chunk`, `summary.progress`, `ai.pull`, `schedules.ran`.

## Commands
`summary.create` (⌘⇧S), `summary.yesterday`, `summary.openSchedules`, `card.summarize`,
`ai.testConnection`, `ai.pullModel`. Settings registered: `ai.timeoutSec`,
`summaries.defaultDetail`, `summaries.links` (core already had `ai.provider|endpoint|model|temperature`;
code cells use the existing `editor.python` and `editor.codeTimeout`).

## Flow
```
SummaryView ─activity.summarize─▶ Core::summarize
   ▲                                 ├─ activity_facts (journal per board, UTC bounds)
   │ summary.chunk / progress        ├─ engine=basic or no events → render::render
   └─────────────────────────────────├─ llm::resolve → prompt::build_messages → llm::chat (stream)
                                     │     └─ error/empty → render::render (fallbackReason)
                                     └─ save → ai/summaries/<id>.json
scheduler thread ─30s─▶ due_ids → run_schedule → summarize → notification (+ Slack hook) → schedules.ran
```

## Technical decisions
- Facts are computed deterministically in Rust; the LLM only rewrites them (and is told the
  notes are data), so the basic renderer and the AI share one source of truth.
- Endpoint: plain http only for loopback; a non-loopback endpoint is allowed over https and
  the UI shows "your notes will be sent there" (`remote`).
- Missed schedule runs collapse into a single catch-up run; `last_run` is written after the run.
- Code cells require per-board trust (`Conflict("needs_trust")` → confirm dialog → `code.trust`).
  The child gets a scrubbed environment (no app secrets), a temp cwd, `kill_on_drop` and caps.
- Link previews: http(s) only, no credentials in URL, common ports only, `localhost`/`.local`/
  `.internal` rejected, DNS resolved and **every** address must be public (v4/v6 private,
  loopback, link-local, CGNAT, ULA, mapped…), connection pinned to the checked address
  (`resolve`), redirects followed manually (max 5) and re-checked, body capped (1 MiB HTML,
  1.5 MiB image), 7-day disk cache + in-memory cache.
- Slack is only a hook (`set_slack_sender`), registered by the Slack integration.

## Risks / assumptions
- Stop in the UI only stops listening; the backend generation runs until done/timeout.
- Code cells run with the user's OS permissions (by design, after explicit trust); no sandbox.
- The scheduler only runs while the app (or its background mode) is running.
- `card.summarize` is exposed as a command; no card-face/contribution UI was added.
- The browser mock builds summaries from the current board state, not a journal.

## Tests
- Rust (`cargo test --workspace`, 100 core tests): stage inference (en/es/pt, emoji, numbering),
  facts (completed vs started, net moves, edit sessions, remote exclusion), prompt (detail,
  custom prompt), render, LLM stream parsing/model choice/endpoint rules, SSRF address
  rules/URL checks/meta parsing, schedule validation/next run/period ranges, scheduler `due_ids`
  (no double runs, single catch-up), code runner (env scrub, timeout), store, service
  end-to-end on a real board (facts → basic summary → saved), schedules CRUD + trust gate.
- Frontend: `range.test.ts` (presets, custom ranges, plain/Slack export, link resolution).
- `pnpm check` 0/0, `npx vitest run`, `pnpm i18n en|es|pt`, `cargo clippy -D warnings`, `cargo fmt`.
- Manual: `pnpm dev:web` — summary view, generate (mock), schedules editor.
- Recommended: manual test against a real Ollama (`ollama serve`) and LM Studio; notification
  delivery on macOS/Windows.
