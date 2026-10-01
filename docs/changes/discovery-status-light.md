# Discovery that always ends, and a status light instead of the spinner

## Objective

- Review 2, finding 1: after a long time the status bar still said "Indexing boards 0/…" with a
  spinner. Find every reason it can stay on and fix the causes. Discovery must never block forever
  and must never rescan in a loop.
- Replace the spinner with a small status light in the bottom-left corner. It has a popover that
  explains the state and lists issues. It is driven by one pure, unit-tested state model.

## Root causes found

| # | Cause | Evidence (code before this change) |
|---|---|---|
| 1 | **A start without a finish.** The UI set `registry.scanning = true` on the first `progress{task:"discovery"}` event. Only an `index` event with `done == total` cleared it. When a scan found **no boards** (or only open ones whose indexing emitted nothing new), no `index` event was sent, so the spinner stayed on forever. | `src/lib/state/registry.svelte.ts` (old line 16): `scanning = e.done < e.total` was only evaluated for `index`. `Core::rescan` (old `app/mod.rs:1306`) emitted `index` progress only inside `for f in found`. Regression test: `rescan_with_no_boards_closes_every_task`. |
| 2 | **"0/…" was not a count.** Discovery sent `done:0,total:0`. The bar rendered `total \|\| '…'`, which made a folder walk look like stuck indexing. | Old `StatusBar.svelte:28`. |
| 3 | **Blocked I/O on macOS privacy (TCC) folders.** With the default root (home), `walkdir` descended into `~/Desktop`, `~/Documents` and `~/Downloads`. The first `readdir` blocks the discovery thread until the "Luau would like to access…" prompt is answered. The whole scan, including the boards that would follow, waited on it. | Review 2 §5.1 (reproduced on the release build). Old `discovery::scan` walked every root on the calling thread, with no timeout. |
| 4 | **The scan flag was not panic-safe.** `scanning.store(false)` ran only at the end of the thread. A panic while walking or indexing left it `true`, so every later rescan returned at once, and the UI never got a closing event. | Old `Core::rescan`: the flag was reset in straight-line code at the end of the spawned closure. |
| 5 | **Errors were swallowed.** `index_closed_board` ignored every error (`let _ = …`), so a broken index or an unreadable board was invisible. | Old `index_closed_board`. |
| 6 | **Rescans were triggered twice and settings were not read.** First run called `discovery.rescan` right after `settings.set`. Settings are saved asynchronously, so the scan could use the old roots. `discovery.rescanMinutes` existed in Settings but nothing used it. | Old `FirstRun.svelte` (`void rpc('discovery.rescan')`). `rescanMinutes` had no reader in Rust or TS. |

No file-watcher loop was found. `watch.rs` and the integrations watcher never call `rescan`, and
nothing that discovery writes (`registry.json`) is watched. The new design keeps it that way: a
rescan only happens on app start, on the schedule, on an explicit command, after a discovery setting
changes, and once after a privacy prompt is answered late (see below).

## Files

Rust:
- `crates/luau-core/src/app/activity.rs` (new): `TaskEvent`, `TaskPhase` (`started`, `progress`,
  `waiting`, `finished`, `failed`), `TaskIssue`, `TaskGuard` and `TaskBoard`, plus their tests.
- `crates/luau-core/src/discovery.rs`: `plan`, which puts protected folders last and only includes
  them when opted in; `walk_unit`; and the watchdog runner `run`, which reports waiting, gives a unit
  up and delivers its results late. Also `Found.damaged`, root error classification, and tests.
- `crates/luau-core/src/app/mod.rs`: `CoreEvent::Task` replaces `CoreEvent::Progress`. Also
  `rescan` (coalesced, guard-released flag), `scan_once`, `merge_found`, `index_found`,
  `clear_late_issue`, `update_settings`, `start_discovery` / `rescan_due` (honours
  `discovery.rescanMinutes`), `task_status`, and the `include_protected` option.
- `crates/luau-core/src/app/tests.rs`: integration tests (listed below).
- `src-tauri/src/lib.rs`: `core.start_discovery(1.5 s)`.
- `src-tauri/src/rpc.rs`: `settings.set` goes through `update_settings`; new `status.tasks` returns
  the last event per task.

UI:
- `src/lib/status/activity.ts` (new): the pure reducer and summary (`reduceTask`, `mergeSnapshot`,
  `summarize`, `taskMessage`, `barMessage`, `issueMessage`). Tests are in `activity.test.ts`.
- `src/lib/status/activity.svelte.ts` (new): a store fed only by `task` events, plus one
  `status.tasks` snapshot at load. It does no polling.
- `src/lib/shell/StatusLight.svelte` (new) renders the light and its popover.
  `src/lib/shell/StatusBar.svelte` mounts it and no longer has the spinner.
- `src/lib/state/registry.svelte.ts`: `scanning` is derived from paired events. It is `true` while
  discovery or index is open.
- `src/lib/views/FirstRun.svelte`: a toggle for Desktop/Documents/Downloads. The duplicate rescan
  is removed, because saving the settings triggers the scan.
- `src/lib/settings/schema.ts`: `discovery.protectedFolders` (boolean, default `true`).
- `src/lib/backend/types.ts`: `TaskEvent` types. `src/lib/backend/mocks/status.ts` (new): paired
  events for the web build, plus demo states.
- `src/lib/app/bootstrap.ts`: `initActivity()`.
- i18n: `src/lib/i18n/parts/status.{en,es,pt}.ts` (new) and the `discovery.protectedFolders` label
  in `prefs.{en,es,pt}.ts`.

## Logic

### Pairing (Rust)

`TaskGuard::start(emit, task, total)` emits `started`. `Drop` emits exactly one `finished`, or
`failed` when `fail(code)` was called or the thread is panicking (`taskCrashed`). Because of this,
early returns, `?` and panics all close the task. The guard collects issues and sends them, deduplicated,
with the closing event. `Core::task_emitter` records every event in `TaskBoard`, so a window that
opens later can call `status.tasks`, and forwards it as `CoreEvent::Task`.

### Discovery plan and watchdog

1. `plan(opts)` turns the roots into units. Ordinary folders come first and protected folders come
   last. The home unit always skips `Desktop`/`Documents`/`Downloads`, which get their own units.
   - Protected folders inside a root are only planned when `include_protected` is set. That
     requires `general.firstRunDone` (first-run is closed) and `discovery.protectedFolders`
     (default on).
   - A root that is itself inside a protected folder was chosen explicitly, so it is always
     scanned (last).
2. `run` walks each unit on its own thread. A watchdog polls an entry counter.
   - No progress for 2 s: it emits `waiting`, with `waitingAccess` for protected units and
     `waitingFolder` otherwise. The UI shows "Waiting for folder access…".
   - No progress for 30 s: it gives the unit up and reports `accessPending` or `folderStalled`.
     Discovery then finishes normally, so indexing and the rest of the boards are not held back.
   - After a protected unit is given up, the remaining protected units are not started in this
     pass, because macOS queues the prompts and they would block too. They are also reported as
     `accessPending`.
   - A unit whose thread from an earlier pass is still alive is not started again. This avoids a
     pile-up of blocked threads.
3. A blocked syscall cannot be cancelled, so the thread lives on. If it finishes later, its in-flight
   slot is released and its boards go to the `late` callback. That callback merges them into the
   registry and indexes them. It also removes that folder's `accessPending`/`folderStalled` warning
   from the last closed discovery result by re-emitting `finished` with the remaining issues. If other
   protected folders are still pending, it requests one rescan. This is bounded: each further pass
   needs another prompt to be answered.
4. Registry entries below folders that were not scanned (given up on, or not opted in) are neither
   marked missing nor touched. Touching them could raise the prompt again.

### Rescan scheduling (no loops)

`rescan()` sets `rescan_wanted` and starts a worker only if none is running. The worker drains the
flag, so requests made during a pass coalesce into one more pass. The `scanning` flag is cleared by
a guard (`Drop`), and a final re-check closes the race between "drained" and "released". Triggers:

- start, after 1.5 s;
- `discovery.rescanMinutes`, default 30 (0 means only at start). A one-minute tick checks
  `rescan_due`, which counts from the end of the last pass, so a long scan cannot chain into the
  next one;
- the `board.rescan` command;
- `settings.set` when one of `discovery.roots`, `discovery.exclude`,
  `discovery.protectedFolders` or `general.firstRunDone` changed;
- the bounded late-answer case above.

### Status light (UI)

- `reduceTask` handles each phase:
  - `started` resets the task (a new run clears the issues of the previous run);
  - `progress` updates the counts and clears `waiting`;
  - `waiting` keeps the task running and records why;
  - `finished` replaces the task's issues with the ones it reported;
  - `failed` always produces an error issue (`taskFailed` if the core sent none).
- `mergeSnapshot` fills in only tasks that have not been seen live, because live events are newer.
- `summarize(state, extra)` picks the light. Precedence is **error > warn > active > idle**:
  - red: any error issue (`boardDamaged`, `indexFailed`, `taskCrashed`, `taskFailed`);
  - yellow: any warning (`waitingAccess`, `waitingFolder`, `accessPending`, `accessDenied`,
    `folderMissing`, `folderStalled`, `folderFailed`, `indexSkipped`, and `integrationOffline`
    from `extra`);
  - green: a task is running;
  - gray: none of the above.
  `busy` makes the dot pulse whatever the colour. A waiting scan is yellow and pulsing.
- Text next to the light appears only while busy: "Looking for boards…", "Indexing boards 2/5"
  (only when there is a real total), or "Waiting for folder access…". The popover shows the state,
  an explanation, the running tasks and the issues, which name the folder or board. It has a
  **Rescan for boards** button.
- Accessibility: the button's `aria-label` is the state plus the current activity, and it has
  `aria-haspopup="dialog"` and `aria-expanded`. Escape and an outside click close the popover.
  The tooltip mirrors the label. The pulse stops under `prefers-reduced-motion` and the app's
  reduced or no motion setting.
- Colours come from the existing tokens (`--ok`, `--warn`, `--danger`, `--ink-4`). `tokens.css` is
  unchanged.

## Decisions

- **One event type with phases instead of separate progress and done events.** Pairing is easier to
  enforce with a guard, and the UI reducer stays total.
- **Watchdog plus giving up, not cancellation.** A thread blocked in `open()` on a TCC prompt cannot
  be interrupted. We stop waiting for it, keep the rest of discovery moving, and accept its results
  late.
- **Protected folders are default on after first run, off during it.** This keeps today's behaviour
  of finding boards on the Desktop, but no prompt appears behind the first-run dialog, and the user
  can opt out there or in Settings. Explicitly configured roots are always honoured.
- **Issue subjects are display names** (a board name or a folder's last component), never full
  paths. Logs carry counts and error codes only.
- `status.indexing` (the existing key) is reused for real counts.

## Risks and assumptions

- **Not verified natively.** The TCC behaviour (the prompt blocks `readdir`; "Don't Allow" returns
  `EPERM`) is taken from review 2 and Apple's documented behaviour. It is covered by a simulated
  blocking walker in the tests, not by a real prompt. A release build should be checked on a fresh
  macOS user.
- Other protected locations (iCloud Drive and other volumes) are not in the list. iCloud lives under
  `~/Library`, which is already excluded. Network or removable volumes are only scanned when added as
  roots, and the watchdog then reports them as `folderStalled` instead of hanging.
- A unit that makes progress very slowly, but makes some, is never given up. That is intended
  (slow, not blocked).
- `CoreEvent::Progress` was removed. Nothing else consumed it (checked with grep in `src/` and
  `src-tauri/`).

## Tests

Rust (`cargo test --workspace`):
- `app::activity::tests`:
  - start is paired with finish, and issues are deduplicated;
  - an early return (`?`) still finishes;
  - `fail` emits `failed` with an error issue;
  - a panic emits `failed` (`taskCrashed`);
  - the snapshot keeps the last event per task;
  - events serialise to camelCase.
- `discovery::tests`:
  - protected folders are planned last;
  - they are skipped unless opted in;
  - an explicit protected root is scanned last;
  - duplicate units are merged;
  - the walk skips planned sub-folders and reports a missing root;
  - a damaged manifest is flagged;
  - a blocked protected unit is given up: it reports waiting, then access pending, does not start
    the others, and delivers late with its slot released;
  - a slow but progressing unit is not given up;
  - a panicking walker is reported as failed and its slot is released.
- `app::tests`:
  - `rescan_with_no_boards_closes_every_task` is the regression test for "0/…";
  - missing roots and damaged boards are reported, with subjects that are never paths;
  - rescans coalesce and nothing rescans by itself;
  - only discovery settings trigger a rescan;
  - protected folders wait for first run and the opt-in;
  - the schedule follows `rescanMinutes`;
  - a late folder clears its own pending issue.

UI (`pnpm test`): `src/lib/status/activity.test.ts`, 14 cases:
- idle, active, waiting and the regression;
- real counts only;
- warnings persist, and errors win;
- `failed` without an issue still turns red;
- a new run clears issues, and a late correction does too;
- extra issues are merged and deduplicated;
- snapshot merge;
- immutability;
- unknown codes;
- every code and light is phrased in en, es and pt.

Web build (`vite --port 1452`, mock backend): each state was shown with
`window.luauMockStatus(...)` or `?status=`:
- gray "All quiet";
- green, pulsing, "Looking for boards…";
- yellow, pulsing, "Waiting for folder access…";
- yellow with the issue list;
- red with a damaged board and a failed index.

A full mock pass, started from the popover's Rescan button, went "Looking for boards…" →
"Indexing boards 0/2 … 2/2" → "All quiet". Escape closes the popover, and the pulse stops with
reduced motion. The first-run folder step shows the new toggle.

Recommended:
- a native check on macOS with a fresh user (prompt open more than 30 s, then Allow; and Don't
  Allow);
- a long-running session to confirm that the light ends gray between scheduled passes.
