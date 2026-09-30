<p align="center">
  <img src="assets/icon.png" width="96" height="96" alt="Lull icon">
</p>

<h1 align="center">Lull</h1>

<p align="center">
  A calm, local-first kanban board and Markdown notes app for macOS, Windows and Linux.<br>
  Every card is a plain Markdown file. No accounts, no cloud, no lock-in.
</p>

<p align="center">
  <a href="LICENSE">MIT license</a> ·
  <a href="docs/ARCHITECTURE.md">Architecture</a> ·
  <a href="SPEC.md">Spec</a>
</p>

---

<!-- Screenshots: replace with real captures (light + dark) before the first release. -->
<p align="center">
  <em>Screenshot placeholder: board view (light)</em> ·
  <em>card editor</em> ·
  <em>command palette</em> ·
  <em>board view (dark)</em>
</p>

## Why Lull

- **Your files, your folders.** A board is any folder on disk with a hidden `.lull/` directory.
  Lanes are folders, cards are `.md` files. Open them in any editor, sync them with git,
  Dropbox or iCloud; they stay readable without the app.
- **Fast and light.** Tauri 2 + Rust core + Svelte 5: a small native binary that uses the system
  webview, starts quickly and keeps memory low.
- **Keyboard first.** Everything is a command: a VS Code–style command palette, fully
  rebindable shortcuts with chords, and optional single-key board shortcuts.
- **Calm by design.** Soft pastel palette, light and dark themes, translucent sidebars where the OS
  supports them, and motion that respects “reduce motion”.

## Features

### Boards
- **Kanban boards** with lanes shown as columns or rows, drag-and-drop for cards and lanes
  (pointer-based, smooth on every OS), lane colours, collapse, resizable width and WIP limits.
- **Group cards**: drop a card onto another to nest it; unlimited depth, collapsible on the board.
- **Files boards**: a tree of Markdown documents with full-page editor tabs, convertible to and from kanban.
- Board **templates** (Simple, Product, Personal, Content pipeline, Bug triage…) and per-board card templates.
- Inline **quick add**, multi-select with bulk move / tag / archive / delete, a board **filter bar**
  (`#tag`, `@person`, `is:open`, `due:overdue`…), per-board zoom and archive.
- Cards show tags, due date, priority, assignees, task progress, attachments and an optional cover image.

### Editor
- Obsidian-style **live preview** built on CodeMirror 6, with a source mode toggle.
- Slash menu for blocks: headings, lists, checklists, tables, callouts, code blocks,
  **math (KaTeX)**, **diagrams (Mermaid)**, dividers, images and files.
- Card-to-card **links** `[[…]]` shown by title, embeds, backlinks and `#tags` with autocomplete.
- **Python cells** that run locally, rich-text paste converted to Markdown, spell check and optional **Vim mode**.
- Opens as a centred modal or a pinnable sidebar; autosaves as you type.

### Find and remember
- **Explorer** of every board discovered on your machine (boards are found automatically, even after you move them).
- **Full-text search** across all boards (SQLite FTS5) with a query syntax and filters.
- **Per-board undo/redo** and persistent **history** with diffs, restore, and a **trash** (7 days by default, configurable).
- **Activity summary**: “what did I do yesterday?” grouped by board, ready to paste into a stand-up.

### Integrations
- **Jira** (Cloud and Server/Data Center): browse and search issues, drag them into boards as linked copies,
  run Jira actions (transition, assign, comment), and keep **mirror boards** synced with a Jira board.
- **Trello** and **Slack** integrations through the same provider and contribution points.
- Every write to an external service can require explicit confirmation. Tokens are stored in the OS keychain
  (macOS Keychain, Windows Credential Manager, Linux Secret Service), never in settings or logs, and all
  traffic is HTTPS to the hosts you configured.

### Local AI
- Optional **card and activity summaries** with a model running on your own computer
  (Ollama, LM Studio or any local OpenAI-compatible server). Nothing leaves your machine.

### Everywhere
- VS Code–style **Settings** view with search and a **keyboard shortcuts editor** (record keys, chords,
  `when` clauses, conflicts), plus presets for **VS Code, Trello, Vim and Emacs**.
- Tabs (pin, reopen closed, split right), multiple windows, native macOS menus.
- Import from a Markdown folder, export to Markdown, HTML, PDF or a board `.zip`; export/import settings.
- UI in **English, Español and Português**.

## Install

Pre-built installers are published on the GitHub Releases page:

| Platform | Package | Requirements |
|---|---|---|
| macOS (Apple Silicon, Intel) | `.dmg` / `.app` | macOS 13 Ventura or later |
| Windows (x64, ARM64) | `.exe` (NSIS) / `.msi` | Windows 10 1809+ with WebView2 |
| Linux (x64, ARM64) | `.AppImage`, `.deb`, `.rpm` | WebKitGTK 4.1 (Ubuntu 22.04+, Fedora 38+) |

Early builds are unsigned: on macOS, right-click the app and choose **Open** the first time;
on Windows, choose **More info → Run anyway** in SmartScreen.

## Build from source

Prerequisites: [Node.js 24](https://nodejs.org), [pnpm](https://pnpm.io),
[Rust (rustup)](https://rustup.rs) and the
[Tauri 2 system dependencies](https://tauri.app/start/prerequisites/)
(on Linux: `libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf`).

```sh
pnpm install          # install JS dependencies
pnpm dev              # run the desktop app with hot reload (Tauri + Vite)
pnpm dev:web          # run the UI in a browser with an in-memory mock backend
```

### Building installers

```sh
pnpm build --localtarget   # build for this machine only (fastest)
pnpm build --release       # build every target this host can build; the rest are built by CI
pnpm build --target <id>   # one target (add --debug for a debug build, --dry-run to print commands)
```

Targets (`scripts/build.mjs`):

| Target | Rust triple | Bundles |
|---|---|---|
| `macos-arm64` | `aarch64-apple-darwin` | app, dmg |
| `macos-x64` | `x86_64-apple-darwin` | app, dmg |
| `windows-x64` | `x86_64-pc-windows-msvc` | nsis, msi |
| `windows-arm64` | `aarch64-pc-windows-msvc` | nsis, msi |
| `linux-x64` | `x86_64-unknown-linux-gnu` | AppImage, deb, rpm |
| `linux-arm64` | `aarch64-unknown-linux-gnu` | AppImage, deb, rpm |

macOS builds both Apple architectures; Windows and Linux builds of other architectures run on
GitHub Actions (`gh workflow run release.yml`). Output lands in `target/<triple>/release/bundle/`.

### Checks

```sh
pnpm check                 # svelte-check (0 errors, 0 warnings)
pnpm test                  # vitest
pnpm test:rust             # cargo test --workspace
pnpm i18n en|es|pt         # missing translations / placeholder mismatches
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```

## Data format

```
Project Alpha/                ← any folder, anywhere
├── .lull/                    ← hidden, app-internal (also the board marker)
│   ├── board.json            ← manifest: id, name, type, lane order, view
│   ├── history/              ← operation journal + compressed text versions
│   ├── trash/                ← deleted items, restorable
│   └── cache/                ← disposable (search index, summaries, thumbnails)
├── k4m2p9/                   ← a lane
│   ├── index.json            ← { id, name, order }
│   ├── c8x1q0.md             ← a card: plain Markdown, first line "# Title"
│   ├── c8x1q0.3f9a.png       ← an image owned by that card
│   └── c2mz7p/               ← a group card
│       ├── index.md
│       ├── index.json
│       └── c77ab1.md
```

- Cards are pure Markdown with no front-matter; tags (`#tag`), links (`[[c8x1q0]]`), due dates and
  priorities are inline text, so files stay meaningful in any editor.
- `index.json` order is the source of truth for ordering; the file system is the source of truth for existence.
- JSON is pretty-printed with a stable key order for clean git diffs. Writes are atomic; loading never rewrites your files.
- Boards created by a newer Lull open read-only instead of being silently changed.

## Keyboard shortcuts (highlights)

`⌘` is `Ctrl` on Windows and Linux. Every shortcut can be changed in **Keyboard Shortcuts** (`⌘K ⌘S`).

| Action | Shortcut |
|---|---|
| Command palette / quick open | `⇧⌘P` / `⌘P` |
| New card / lane / board | `⌘N` / `⇧⌘N` / `⌥⌘N` |
| Open board · close / reopen tab | `⌘T` · `⌘W` / `⇧⌘T` |
| Toggle sidebar · explorer · search · history | `⌘B` · `⇧⌘E` · `⇧⌘F` · `⇧⌘H` |
| Undo / redo | `⌘Z` / `⇧⌘Z` |
| Filter board · rows/columns · show archived | `⌘F` · `⇧⌘L` · `⇧⌘A` |
| Move card up/down/between lanes · nest / unnest | `⌥↑↓←→` · `Tab` / `⇧Tab` |
| Source / live preview · card link | `⌘E` · `⇧⌘K` |
| Back / forward through cards | `⌘[` / `⌘]` |
| Settings | `⌘,` |
| Single-key board shortcuts | `n` new · `e` open · `t` tag · `d` due · `p` priority · `a` archive · `m` move · `/` filter · `?` cheat sheet |

## Architecture

A framework-free Rust core (`crates/lull-core`) owns the file system, watcher, discovery, search,
history and integrations; a thin Tauri adapter (`src-tauri`) exposes it over a single RPC command;
the Svelte 5 UI (`src/`) is built around a command registry and contribution points.
See **[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)** for layers and conventions, and
[SPEC.md](SPEC.md) for the product specification. Change notes live in `docs/changes/`.

## Contributing

1. Read [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) and follow its conventions
   (every action is a command, all strings go through `t()`, board mutations go through `Core::apply`).
2. Work on a short-lived branch and use [Conventional Commits](https://www.conventionalcommits.org)
   (`feat(board): …`, `fix(editor): …`, `docs: …`).
3. Add strings to `src/lib/i18n/parts/<feature>.en.ts` **and** translate them in `.es.ts` / `.pt.ts`
   (glossary in `docs/changes/i18n.md`).
4. Add unit tests for business rules and a short note in `docs/changes/<feature>.md`.
5. Make sure all checks above pass before opening a pull request.

Security issues: please report them privately rather than in a public issue.

## License

[MIT](LICENSE) © 2026 Eros Talevi
