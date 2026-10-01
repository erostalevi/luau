# Contributing to Luau

Thanks for helping! Luau aims to stay calm, fast and local-first. Small, focused pull requests are easiest to review.

## Setup

```bash
pnpm install
pnpm dev:web     # UI in the browser with a mock backend — fastest loop for UI work
pnpm dev         # full desktop app
```

Read [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) first: it explains the layers (Svelte UI → one RPC command →
framework-free Rust core) and the conventions every feature follows.

## Ground rules

- **Every board mutation is an `Op`** applied through `Core::apply` (undoable, journaled, indexed). Never write board
  files directly.
- **Every user action is a command** (`*.commands.ts`). Strings go through `t('…')`, with en/es/pt in
  `src/lib/i18n/parts/<feature>.<locale>.ts`.
- **Security:** the webview is untrusted. No new RPC may accept arbitrary file paths (use `grants::require`), never log
  card text, tokens or emails, keep secrets in the OS keychain, and keep network access scoped and opt-in.
- **UI:** use the design tokens and shared classes; keep it keyboard accessible (focus, Esc, Tab order).
- Add a browser mock (`src/lib/backend/mocks/<feature>.ts`) so the feature works in `pnpm dev:web`.

## Before opening a pull request

```bash
pnpm lint && pnpm check && pnpm test
pnpm i18n en && pnpm i18n es && pnpm i18n pt
pnpm check:tauri
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

- Add tests for business rules (Rust `#[cfg(test)]` next to the code, `*.test.ts` for the UI).
- Add a short note in `docs/changes/` describing the objective, files, logic, decisions, risks and tests.
- Use [Conventional Commits](https://www.conventionalcommits.org/) (`feat(editor): …`, `fix(core): …`).

## Releases

Tag `vX.Y.Z` and run the **release** workflow (`.github/workflows/release.yml`). It builds all six targets, signs
updater artifacts with `TAURI_SIGNING_PRIVATE_KEY` and, when the `APPLE_*` secrets are set, signs and notarizes the
macOS build.
