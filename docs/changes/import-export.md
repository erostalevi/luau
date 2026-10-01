# Import / export, templates, schema upgrade

## Objective

Implement SPEC §15 (import / export through a standard interchange model), §4.1
(schema rules: newer boards open read-only and can be converted) and the
settings portability of §16 / QUESTIONNAIRE M.87, plus "new board from template"
(QUESTIONNAIRE 31). Trello import is out of scope (owned by integrations).

## Files

Rust core (`crates/lull-core/src/io/`, new):

| File | Role |
|---|---|
| `mod.rs` | Shared helpers: safe relative paths, file names from titles, natural sort, `is_within`, HTML escaping |
| `interchange.rs` | Interchange model (`lull-interchange` v1), `from_state`, import planner (interchange → ops, attachment copies, id map) |
| `vault.rs` | Loose adapter: any Markdown folder → tree → interchange |
| `loose.rs` | Mode C for plain folders: read-only board "as is" (`LooseLayout`), `Core::open_loose`, `lull://` resolution |
| `export.rs` | Markdown file, Markdown bundle (.zip), self-contained HTML |
| `html.rs` | Safe Markdown → HTML (tables, callouts, code, tags, card links, CSP meta, print CSS) |
| `archive.rs` | Zip writer (temp file + rename), board `.zip`, hardened extraction |
| `templates.rs` | Card templates on disk (`templates.list`), `create_from_template` |
| `upgrade.rs` | Schema status rules, `upgrade_board`, `Core::upgrade_board_schema` |
| `settings.rs` | Settings bundle validation, export, import |
| `service.rs` | `Core::io_export`, `io_render_html`, `io_inspect`, `io_import`, `apply_interchange` + integration tests |

Small, surgical edits outside the feature (merge-relevant):

- `crates/lull-core/Cargo.toml`: `pulldown-cmark` feature `html`.
- `crates/lull-core/src/model.rs`: `LooseLayout`, `BoardState.loose`; path helpers consult it.
- `crates/lull-core/src/store/mod.rs`: `BoardStore::from_state`, `reload` / `read_content` handle loose boards.
- `crates/lull-core/src/history.rs`: no journal / blobs when the folder has no `.lull` (never create a marker implicitly).
- `crates/lull-core/src/app/files.rs`: no thumbnails cache / unlinked sweep for loose folders.
- `crates/lull-core/src/app/registry.rs`: `BoardEntry.loose`.
- `crates/lull-core/src/app/mod.rs`: reopen loose entries by id, rescan doesn't mark them missing, `board_file` consults loose boards.
- `src-tauri/src/protocol.rs`: resolve through `core.board_file` (loose-aware).
- `src-tauri/src/exports.rs`: RPC dispatch (below).
- `src/lib/backend/types.ts`: `BoardEntry.loose?`.
- `src/lib/board/BoardView.svelte`: banner shows "Convert to this version" only for `newer_schema:*`; loose folders get "Import a copy".
- `src/lib/commands/builtin/app.ts`: `app.exportSettings` / `app.importSettings` delegate to the new validated commands.

Frontend (new): `src/lib/io/io.ts` (pure helpers), `src/lib/io/io.commands.ts`,
`src/lib/io/io.test.ts`, `src/lib/backend/mocks/io.ts`, `src/lib/i18n/parts/io.{en,es,pt}.ts`.

## RPC methods

| Method | Params | Result |
|---|---|---|
| `io.export` | `board, card?, format (zip/md/mdBundle/html/json), dest, includeHistory?` | `{ path, files, bytes }` |
| `io.renderHtml` | `board, card?` | HTML string (PDF printing) |
| `io.inspect` | `path` | `{ kind: board/boardZip/zip/folder/interchange, name, boardId, registered, schema, boardKind, lanes, notes, truncated }` |
| `io.import` | `path, mode (copy/overwrite/inPlace), dest?, target?, name?, inboxLane?` | `{ boardId, cards, lanes, attachments, warnings }` |
| `templates.list` | `board?` | card templates (`.lull/templates/*.md` + `<appData>/templates/*.md`) |
| `board.createFromTemplate` | `path, name, template {kind, lanes[{name,cards}], notes}, git?` | snapshot |
| `board.upgrade` | `board` | `{ report {from,to,files}, snapshot }` |
| `settings.export` | `path, bundle` | validated bundle |
| `settings.import` | `path` | validated bundle (UI applies settings + keybindings) |

## Commands

`board.export`, `card.export`, `file.export` (document of a files board),
`board.import` (accepts `{ path }`), `board.newFromTemplate`, `board.upgradeSchema`,
`settings.export`, `settings.import` (hidden; the visible `app.*Settings` commands run them).

## Logic

```
export:  BoardState ──► Interchange JSON | Markdown | Markdown bundle | HTML ──► atomic write / zip (tmp + rename)
         PDF = io.renderHtml ──► hidden sandboxed iframe ──► window.print() ("Save as PDF")

import:  path ─► io.inspect ─► mode?
           copy (B, default)  : source ─► Interchange ─► plan ─► create board ─► Op::Batch (1 undo) ─► copy attachments
           overwrite (A)      : Op::Batch[ Trash(all lanes/root), plan… ] on the target (1 undo restores it)
           inPlace (C)        : Lull board → open; plain folder → read-only "as is" (no .lull, nothing written)
         sources: Lull board folder · board .zip · Markdown .zip/folder (loose adapter) · interchange .json
```

Loose adapter: top-level subfolders → lanes, root notes → inbox lane (kanban)
or root documents (no folders → files board); deeper folders → group cards (folder
note `index.md`/`README.md`/`<Folder>.md` is the body); first heading is the title,
otherwise the file name becomes `# Title`; front-matter tags → `#tags`, scalars →
property footer; `[[wiki]]` and `[text](note.md)` → card links; local images/files →
copied attachments. Originals are only read.

Schema: `schema > SCHEMA` → store opens read-only (`newer_schema:N`, existing core
behaviour) → banner + "Convert to this version" (danger confirm) → `board.upgrade`
stamps `schema` on every `index.json` and finally `board.json` (an interrupted run
stays read-only), keeping unknown JSON keys, then reopens. `schema < SCHEMA` runs the
(currently empty) migration table.

## Technical decisions

- **PDF through the print dialog** (QUESTIONNAIRE 86b) instead of bundling a PDF
  engine: zero dependencies, same HTML/CSS as the HTML export (print stylesheet
  included). Depends on the webview supporting `print()` from an iframe.
- **Mode C for foreign folders is read-only**: Lull's format needs id file names;
  converting in place (renaming user files) was rejected in QUESTIONNAIRE 85. Ids
  are derived from the relative path so reloads/watcher events keep them stable.
- **Overwrite is undoable**: implemented as one batch (trash + recreate) instead of
  replacing folders on disk.
- **Interchange JSON**: `sourceRoot` is honoured only when it is a Lull board
  folder; otherwise attachments resolve next to the JSON (prevents a crafted file
  from pulling arbitrary files into a board).
- Board templates stay localized in the UI and are sent to the core as data
  (validated: ≤ 50 lanes, ≤ 500 cards, ≤ 256 KB per card).

## Security (OWASP)

- Zip extraction rejects absolute paths / `..`, skips symlinks, caps entries (200k)
  and unpacked size (8 GB, zip-bomb guard); temp folder removed after import.
- Exports refuse destinations inside the board; files are written atomically.
- HTML export: raw HTML in cards is rendered as text, only `http(s)`/`mailto`/relative
  links survive, SVG is never embedded, CSP meta `default-src 'none'`, no scripts.
- Loose `lull://` resolution: regular, non-hidden files inside the folder only.
- Settings bundles: strict schema validation, size cap (8 MB), keys that look like
  secrets (`token`, `password`, `apiKey`…) are dropped on export and import; secrets
  stay in the OS keychain. Logs carry ids/counts only, never card bodies or paths of
  user content beyond the board id.

## Risks / assumptions

- Markdown bundle: attachments not referenced from the card body are not re-imported
  by the loose adapter (they are still exported).
- Loose boards show no attachment chips (files are served by path); editing requires
  "Import a copy".
- Print support of iframes varies per OS webview (WKWebView / WebView2 / WebKitGTK).
- No migrations exist yet (schema 1); the table is ready for v2.
- `board.upgrade` is not journaled (it only rewrites JSON manifests).

## Tests

- Rust (`cargo test -p lull-core io::`, 28 tests): round trip export → import for board
  `.zip`, interchange JSON and Markdown bundle (titles, lanes, groups, attachments,
  links, tags preserved; new ids; source untouched); overwrite in one undo step;
  foreign folder copy (inbox lane, attachment copied, originals untouched, no marker)
  and in-place read-only open (`lull://` traversal refused); Markdown/HTML export
  (links → titles, heading shifts, embedded images, no scripts, no export inside the
  board); loose adapter scan/convert; schema rules + convert (unknown keys kept,
  idempotent); settings validation; template validation; zip traversal rejection.
- Vitest `src/lib/io/io.test.ts`: file names, formats, modes, read-only reasons,
  settings bundle secret stripping, template specs.
- Run: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo fmt --all`, `pnpm check`, `npx vitest run`, `pnpm i18n en|es|pt`.
- Recommended manual: print to PDF in the packaged app on each OS; import a real
  Obsidian vault; open a board written with `schema: 2`.
