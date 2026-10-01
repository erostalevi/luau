# Luau — Architecture & Conventions

Luau is a Tauri 2 app: a framework-free Rust core (`crates/luau-core`), a thin
Tauri adapter (`src-tauri`), and a Svelte 5 UI (`src/`). See `docs/SPEC.md` for the
product spec.

## Layers

```
src/ (Svelte 5 + TS)          UI, command registry, keybindings, editor (CodeMirror 6)
  lib/backend/rpc.ts          transport: Tauri IPC or in-browser mock (pnpm dev:web)
src-tauri/src/                Tauri glue: windows, menus, `luau://` protocol, RPC dispatch
  rpc.rs                      single `rpc(method, params)` command → Core
  *_rpc.rs / exports.rs       per-feature dispatch modules (return Option<R>)
crates/luau-core/src/         framework-free core (unit-testable)
  ids, json_fmt, markdown/    pure domain
  model.rs, store/            file-backed board store, invertible ops, trash
  history.rs, search/         journal + blobs, SQLite FTS5 index
  app/                        Core service: open boards, registry, watcher, cross-board moves
  integrations/, ai/, io/     feature modules (each owns its own files)
```

## On-disk format

See SPEC §4. Key rules:
- Cards are `c??????.md`; groups are folders `c??????/index.md + index.json`.
- `index.json` order is truth for ordering, the file system for existence.
- Loading never rewrites card files. Writes are atomic (`fsutil::atomic_write`).
- App-internal data lives in `<board>/.luau/` (manifest, history, trash, cache).

## Core rules (Rust)

- Every board mutation is an `store::Op` applied through `Core::apply` (journaled,
  indexed, undoable, emits `CoreEvent::BoardDelta`). Never write board files directly.
- Board-kind rules are checked in `Core::check_kind_rules`; the store only checks existence.
- Feature modules add `impl Core { … }` blocks in their own files; `Core` fields are `pub(crate)`.
- Events to the UI: `core.sink.emit(CoreEvent::Custom { name, payload })` for feature events.
- Errors: `luau_core::Error` (`code()` is surfaced to the UI). Never log secrets, tokens,
  emails or card bodies; log ids and HTTP statuses only.
- Network: only through `reqwest` with rustls, HTTPS by default, host allow-list per account.
- Tests: unit tests next to the code; use `tempfile` for FS tests. `cargo test -p luau-core`.

## RPC

- UI calls `rpc<T>('area.method', params)` (`src/lib/backend/rpc.ts`). Params/results are
  camelCase JSON. Rust helpers: `rpc::arg`, `rpc::opt`, `rpc::R`.
- Add methods in your feature's dispatch module (`src-tauri/src/<feature>_rpc.rs`), not in `rpc.rs`.
- Long-running/network methods go in `dispatch_async`; file work in `dispatch_sync`
  (already runs on the blocking pool).
- Add a browser mock in `src/lib/backend/mocks/<feature>.ts` (see README there).

## UI conventions (Svelte 5)

- Runes only (`$state`, `$derived`, `$effect`, `$props`); snippets instead of slots.
- Shared state in `*.svelte.ts` modules under `src/lib/state/` or the feature folder.
- Every user action is a **command** (`src/lib/commands/registry.svelte.ts`). Feature modules
  export `commands: Command[]` from a file named `*.commands.ts` (auto-registered; optional
  `init()` runs once — use it for `settings.register([...])` and `contribute(...)`).
- Command titles are i18n keys; categories are keys under `commands.categories`.
- Multi-step input: `quickPick` / `inputBox` / `steps` from `$lib/quickinput/qi.svelte`
  (supports BACK). Confirmations: `confirm()` from `$lib/state/dialogs.svelte`.
  Toasts: `toast.*`. Context menus: `openMenu(e, items)`.
- Keybindings: add defaults to `src/lib/keybindings/defaults.ts` only if essential; users can
  bind any command in the keybinding editor.
- i18n: all strings through `t('key')`. Add your strings in `src/lib/i18n/parts/<feature>.en.ts`
  (auto-merged; `.es.ts` / `.pt.ts` siblings for translations). Run `pnpm i18n en`.
- Styling: design tokens in `src/styles/tokens.css` (calm, pastel, generous spacing, soft radii).
  Use shared classes from `src/styles/components.css` (`.btn`, `.btn.primary|soft|ghost|danger`,
  `.icon-btn`, `.field`, `.chip`, `.row`, `.section-title`, `.empty`, `.card-surface`, `.glass`).
  Components: `Segmented`, `Toggle`, `Kbd`, `TagChip`, `ColorPicker`, `tip` tooltip action.
  Icons: `@lucide/svelte`, size 14–16, strokeWidth ~1.8.
- Motion: use `var(--dur*)` and `var(--ease*)` so "reduce motion" works.
- Contribution points (`src/lib/contributions/registry.svelte.ts`): card actions, card-face
  strips, editor header sections — used by integrations.
- Type-check with `pnpm check` (svelte-check must report 0 errors / 0 warnings).

## Documentation

Every change lands with a short `.md` in `docs/changes/` (objective, files, logic,
decisions, risks, tests).
