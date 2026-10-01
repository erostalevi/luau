# Luau: product and technical specification

> **Version 1.0** (describes the app as built, October 2026). The original planning draft and the design
> questionnaire are kept in [`docs/planning/`](planning/).

Luau is a calm, local-first **kanban + Markdown notes** desktop app for macOS, Windows and Linux.
- **Your data are plain files.** Every card is a Markdown file and every board is a folder. They stay readable
  and editable without the app, and work well with git, Dropbox or iCloud.
- **Luau adds the rest:** a polished board, an Obsidian/Notion-style live-preview editor, a VS Code-style command
  palette, full history, local AI summaries, and optional Jira / Trello / Slack integrations.

---

## 1. Principles

- **Local first.** No accounts and no cloud. Network use is opt-in and scoped:
  - integrations, only to the hosts you connect;
  - a **local** AI endpoint;
  - link previews, fetched by the backend with SSRF protection.
- **Files are the truth.** Loading never rewrites a card. Writes are atomic: a temp file, `fsync`, then `rename`.
  External edits are picked up live.
- **Human names everywhere.** Files use random ids, and the UI shows titles and names only.
- **Undo everything.** Every mutation is an invertible operation: undoable, journaled and indexed.
- **Keyboard first, mouse friendly.**
  - Every action is a command.
  - Every command can be bound to keys (chords, presets).
  - Drag and drop covers the rest.
- **Calm by design.** Pastel "Hawaii sunset" palette (coral, mango, hibiscus, dusk lavender), generous spacing,
  optional translucency, and reduced-motion support.

## 2. Concepts

| Term | Meaning |
|---|---|
| **Board** | A folder marked by a hidden `.luau/` directory. Types: **kanban** (lanes of cards) or **files** (a tree of documents). |
| **Lane** | A column (or row) of a kanban board; a sub-folder `k??????/`. |
| **Card** | A Markdown file `c??????.md`. The first line is always `# Title`. |
| **Group** | A card with children: the folder `c??????/` with `index.md` and `index.json`. It forms when a card is dropped onto another, nests without limit, and turns back into a plain card when it is empty. |
| **Document** | A card in a files board (opened in its own tab). |
| **Mirror** | A read-only board kept in sync with a Jira or Trello board. It is stored in app data, not in your folders, and allows private local notes. |
| **Linked card** | A card in your own board that is linked to a remote issue; title and description sync both ways (with confirmation). |

**Ids** are random: a one-letter prefix plus six `[a-z0-9]` characters:
- `b…` board;
- `k…` lane;
- `c…` card;
- `t…` trash entry.

## 3. On-disk format

```text
My board/
├── .luau/
│   ├── board.json            schema, id, name, type, lane order, archive, covers, view, tag colors (+ unknown keys kept)
│   ├── history/2026-10-01.jsonl   append-only journal (one JSON entry per line)
│   ├── history/blobs/ab/….gz      content-addressed card versions
│   ├── trash/t??????/entry.json + payload/   deleted items (default 7-day retention)
│   ├── cache/                thumbnails, recovered copies of damaged files, sweep tracker
│   └── .gitignore            keeps caches out of git
├── k1a2b3c/                  lane
│   ├── index.json            { name, order, color, width, wip, collapsed, … }
│   ├── c4d5e6f.md            card
│   ├── c4d5e6f.9f3a-photo.png   attachment of that card
│   └── c7g8h9i/              group card
│       ├── index.md
│       ├── index.json
│       └── cj0k1l2.md        child card
└── …
```

**Ordering and integrity**
- `index.json` order is the truth for ordering; the file system is the truth for existence. Unknown files are
  ignored, and missing entries are recovered on load.
- Line endings are normalized to LF on write.
- JSON is pretty-printed with stable key order (diff-friendly).

**Schema and repair**
- A board with a **newer schema** opens read-only and offers *Convert to this version*.
- A **damaged** `board.json` is never rewritten on load. The board opens read-only from a salvaged copy and offers
  *Repair*.

**Card file**

```markdown
# Ship the beta

Body in Markdown: tasks, tables, code, math, mermaid, callouts, [[c4d5e6f]] card links, ![[embeds]], #tags, @people.

---
priority: high
due: 2026-10-08
assignees: @ana, @eros
labels: beta
```

The optional **properties footer** (a `---` line followed by `key: value` lines) holds the priority, due/start
dates, assignees and labels.

## 4. Board

**Layout**
- Lanes as **columns or rows**, toggled at the top right.
- **Spacing behaviour:** fixed lane width, or fixed card size.

**Lanes**
- Rename, color, width, WIP limit, collapse, archive, reorder by drag.

**Cards on the board**
- Card faces show: the title, a preview (lists, tables, images), tags, task progress, due date, priority,
  assignees, a cover image and the remote-issue strip.
- Groups show their full child cards inline.
- **Quick add** (⌘N, context-aware): Enter keeps adding, Esc stops.
- **Archive** keeps cards in order at the end of their lane; archiving a group cascades to its children; *Show
  archived* toggles them.

**Drag and drop**
- A pointer-based engine with before / into (group) / after zones.
- Multi-select with ⇧-click and ⌘-drag lasso.
- Drops onto other boards and into the explorer tree.
- Auto-scroll; Esc or pointer-cancel aborts.

**Find and navigate**
- **Find in board** (⌘F): matching cards stay bright, the rest dim.
- **Keyboard navigation:** arrows, Home/End, Enter opens, Space peeks, ⌥ + arrows move cards, Tab/⇧Tab
  nest/un-nest.

**Zoom and pins**
- Pinch or ⌘± zoom per board.
- Pinned boards come first in the explorer.

## 5. Editor

CodeMirror 6 with **live preview** (Obsidian/Notion-like). Markdown stays the source of truth.

**What renders inline**
- Headings, emphasis, task lists and tables.
- Code with syntax highlighting.
- **Math** (KaTeX) and **mermaid** diagrams (follow the light/dark theme).
- Callouts.
- **Embeds:** `![[card]]` and heading links.
- **Python cells** with captured output and images.

**Links and attachments**
- `[[Title]]` links autocomplete, resolve to `[[cardId]]` and can create the card.
- External URLs open in the system browser. "Show thumbnail" gives a link preview, fetched by the backend.
- Local files and attachments open in their default app.
- Images and PDFs show thumbnails and are resizable (`![alt|320](file.png)`). Images on the same line wrap.

**Typing helpers**
- **Natural dates:** `[tomorrow]`, `[next friday]` or `[in 3 days]` become ISO dates once you type on (en/es/pt
  words; never inside code or link labels).
- **@mentions**, **#tags** and the **/slash menu** autocomplete.
- **Paste:** rich paste turns HTML into Markdown (plain text inside code); ⇧⌘V pastes plain text.

**Editor options**
- Spellcheck and Vim mode (off by default, with toggle commands).
- Line width ("Change default width"), editor font and size.

**Saving**
- Autosave about 300 ms after typing; also on blur, on card switch and on close.
- Every save carries the text the editor last saw. If the file changed outside the app, the save is refused and a
  *Keep mine / Take theirs* banner appears.
- Editor layout: centered modal or right sidebar (optionally pinned to follow the selection). Files-board
  documents open in tabs.

## 6. Workspace

**Tabs**
- A tab holds a board, a document, the start page, settings, keyboard shortcuts, the activity summary or a saved
  search.
- Tabs can be pinned and reopened, and a board can't be open in two tabs.
- **Split view**, plus **multiple windows** ("Move tab to new window" carries any tab).

**Left panel** (segmented control)
- **Explorer:** a virtual tree of boards → lanes → cards. Mirrors have their own section. Pinned items come first,
  then alphabetical or manual order. Hidden boards can be shown, expanded state persists, rename happens inline,
  and you can drag cards into the tree.
- **Search:** a query language with a filter UI kept in sync, case sensitivity, snippets, results grouped by board
  or lane, and saved searches (also openable as a virtual board).
  - Query language: `tag:` `board:` `lane:` `is:open|done|archived` `due:` `priority:` `@person` `has:image` `updated:>=` `in:title` `case:yes` `"phrases"` `-exclude`.
- **History:** a timeline grouped by day and filtered by board, card or origin. It also has a word-level diff with
  restore, **Trash** (days left, restore, delete forever, empty) and **Archive**.
- **Integrations:** accounts, JQL search with saved queries, issues you can drag onto boards, mirrors and their
  watch toggles, and the push/pull switches.
- **Extensions:** the contribution registry; third-party loading comes later.

**Start page**
- Greeting, quick actions, recent boards and cards, recent activity, and connect buttons.

**Command palette**
- ⌘⇧P for commands; ⌘P quick-opens cards, docs and boards; `#` searches tags; `@` searches the outline.
- Supports multi-step input with Back.

**Keybindings**
- VS Code-compatible model: when-clauses, two-stroke chords, layout-aware letters, AltGr-safe.
- Presets: **VS Code, Trello, Vim, Emacs**. A preset is picked on first run, and there is a "Pick a keybinding
  preset → override customs?" command.
- Single-key shortcuts can be turned on or off.
- The editor lets you record keys, search by keystroke, edit when-clauses and spot conflicts.

**Settings**
- About 80 settings in 15 categories, with search (`@modified`, `@category:`) and per-setting reset.
- `settings.json` can be imported and exported. Security-sensitive keys (interpreter path, AI endpoint, discovery
  roots, push/pull guards) are never imported.

**Look and feel**
- Theme light, dark or system; primary and secondary colors (custom picker).
- Fonts: default, system, or any installed font.
- Translucency on, or *Reduce transparency*; motion full, reduced or none.
- Languages: **English, Spanish, Portuguese**.

## 7. History, undo, trash

- **Undo/redo:** per board (⌘Z / ⇧⌘Z). Typing coalesces into sessions; cross-board moves undo as one step.
  Concurrent undo/redo is safe.
- **Journal:** every change goes to `.luau/history/*.jsonl` (append-only), and card versions are kept as
  compressed blobs. Retention follows `history.retentionDays` and `history.maxMb`.
- **Trash:** deleted cards and lanes move to `.luau/trash/`. They are kept 7 days by default (1–3650), and
  purges are journaled.
- **Activity summary:** "What did I do yesterday?" — see §9.

## 8. Import and export

- **Export** a board, card or document as **HTML** (self-contained, pretty, no scripts), **PDF** (system print
  dialog), **Markdown** (single file or folder .zip), a **board archive** (.zip) or **JSON** (interchange).
- **Import** a Luau board, a board .zip, a Markdown .zip, JSON, or any Markdown folder (Obsidian vault, plain
  notes: sub-folders become lanes, and the first heading becomes the title). Three modes:
  - **(B) Copy into a new board** (default): the originals are only read.
  - **(A) Overwrite:** one undo step reverses it.
  - **(C) Use in place:** a foreign folder opens read-only, "as is".
- **Templates:** Kanban basic, Sprint, Personal, Notes, plus per-board and global card templates.
  - **Guide boards** (*Luau guide (kanban)* and *Luau guide (notes)*, English content): one card per feature,
    with sample attachments (SVG, PNG, PDF) and working card links.
  - A board template may attach files to its cards (`assets`: card index, name, base64; at most 20 files of
    2 MiB each). Card text can use `{{card:N}}` (the id of the N-th template card) and `{{asset:NAME}}` (the
    file name of that card's attachment); the core fills both in before writing.

## 9. Local AI and automation

- **Activity summaries** over a period and set of boards:
  - A detail slider (1–5) and a custom prompt.
  - Stage inference from lane names (en/es/pt), copy as Markdown, plain text or Slack, and export.
  - Past summaries are kept.
- **Models:** provider **auto** (default) uses **Apple's on-device model** (FoundationModels, macOS 26+ on Apple
  Silicon with Apple Intelligence on: no download, no setup) when available, else a running **Ollama** or any
  **OpenAI-compatible local** endpoint (LM Studio, llama.cpp), else the *basic* writer. Ollama models can be
  pulled from the app.
  - Apple's model is reached through a bundled helper (`apple-llm`, JSON over stdin/stdout, one process per
    request, timeout, prompts never logged). Its small context window (4k–8k tokens) is respected: facts are
    trimmed to fit and the answer length is capped.
  - Settings › AI shows the writer in use and why (e.g. "turn on Apple Intelligence").
  - Without a model, a deterministic *basic* writer is used.
- **Create card from clipboard** (command): *As is* (first line → `# Title`, rest verbatim) or *Let AI review it*
  (local AI only: one or several cards with title, description, tasks, `#tags` and a property footer; strict JSON
  schema, validated and capped in the core, preview with the card count before inserting). Kanban → the active
  lane after the selection; files board with a document open → Markdown sections at the cursor. One undo step.
- **Scheduled summaries:** several schedules, each with days, time, period, boards, prompt and detail level. They
  run in the background, catch up after sleep, and deliver by native notification and/or **Slack**.
- **Card summaries:** basic or AI.
- **Python code cells:** run with a local `python3`, with a timeout and an output cap.
  - The first run in a board asks for **trust in a native dialog**.
  - The code must come from that board's saved card.

## 10. Integrations

- **Providers:** Jira Cloud (email + API token), Jira Data Center / Server (personal access token, optional mTLS,
  custom CA), Trello (key + token) and Slack (bot token).
- **Accounts and secrets:** account settings live in app data without secrets; tokens are kept in the **OS
  keychain**. Each account has a **host allow-list**, checked on every request and redirect.
- **Using Jira issues:**
  - JQL search with saved queries and a default JQL.
  - Drag issues onto any board to create **linked cards**: status strip on the face, status, assignee,
    transitions and comments in the editor.
  - Create an issue from a card.
  - **Mirror boards** of a Jira board or JQL (lanes from columns or statuses) or a Trello board. They pull every
    minute when watched, otherwise on demand, and allow private notes.
- **Writes need confirmation:** every write to an external service goes through *prepare → confirm → commit*. A
  dialog lists exactly what will change ("This will update Status and Assignee on Jira (acme) for LUAU-101"), and
  global **Allow push / Allow pull** toggles act as a development-phase safety switch.

## 11. Architecture

```text
src/ (Svelte 5 + TypeScript)        UI, command registry, keybindings, CodeMirror editor, i18n
  lib/backend/rpc.ts                one transport: Tauri IPC — or an in-browser mock (pnpm dev:web)
src-tauri/ (Tauri 2)                windows, native menus, dialogs, luau:// protocol, RPC dispatch
crates/luau-core/ (Rust)            framework-free core: store, ops, history, search (SQLite FTS5),
                                    discovery, watcher, import/export, AI, integrations
```

- **One RPC command:** `rpc(method, params)`. Each feature has its own dispatch module, async for network,
  blocking pool for files.
- **Every mutation** goes through `Core::apply(Op)`. Ops are invertible (`CreateCard`, `WriteCard`, `Move`,
  `Place`, `Trash`, `Restore`, `SetArchived`, lane/board ops, `Batch`, cross-board `External`), and each one is
  journaled, indexed and emitted as a UI delta.
- **Extension points:**
  - `*.commands.ts` files (auto-registered);
  - `i18n/parts/<feature>.<locale>.ts`;
  - `backend/mocks/<feature>.ts`;
  - a contribution registry (card actions, card-face strips, editor header sections, settings).
- Conventions: [`docs/ARCHITECTURE.md`](ARCHITECTURE.md).

## 12. Security model

- **The webview is not trusted with arbitrary paths.** File RPCs accept only locations picked in **native
  dialogs** (recorded by the backend), the app's own folders, and registered boards.
- **`luau://` board files:**
  - paths are normalized, never inside `.luau/` or `.git/`, and symlinks cannot leave the board;
  - responses are bounded and carry a sandbox CSP and `nosniff`.
- **CSP:** no remote scripts. Images may load over HTTPS only, with no referrer.
- **Code execution:** per-board native trust prompt; the code must be in that board's saved card.
- **Link previews:** fetched by the backend with DNS-pinned SSRF checks (private, loopback and link-local
  addresses blocked; redirects re-checked).
- **Secrets:** kept in the OS keychain. **Logs** contain method names, error codes, ids and counts only — never
  card text, tokens or emails.
- **Untrusted inputs:** trash entries, zip imports (zip-slip safe, size caps), settings bundles and window queries
  are all validated.

## 13. Platforms and builds

- **Targets:**
  - macOS 13+ on Apple Silicon and Intel: `.app` and `.dmg`.
  - Windows 10/11 on x64 and arm64: NSIS installer and `.msi`.
  - Linux on x64 and arm64: AppImage, `.deb` and `.rpm`.
- **Commands:**
  - `pnpm build --localtarget` builds this machine's target.
  - `pnpm build --release` builds every target this host can build; CI builds all six.
- **Updates:** a built-in updater, signed with the Tauri updater key. The release feed is set in
  `tauri.conf.json`.
- **Quality gates (CI):**
  - Tauri npm/crate version parity;
  - ESLint + Prettier, svelte-check, vitest;
  - i18n parity (en/es/pt);
  - `cargo fmt`, `cargo clippy -D warnings`, `cargo test`.

## 14. Limits and non-goals (v1)

- No cloud sync or real-time collaboration; boards sync through your own tools (git, Dropbox, iCloud).
- Third-party extensions can't be loaded yet (the contribution API exists).
- No mobile apps.
- Code cells run with your user's permissions once a board is trusted (no sandbox).
