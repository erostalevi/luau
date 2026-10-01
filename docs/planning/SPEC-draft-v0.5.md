# Luau — Planning draft (v0.5, historical)

> Superseded by [`docs/SPEC.md`](../SPEC.md). Kept for the design history; `§` numbers in `docs/changes/*` refer to this draft.

> Status: **DRAFT v0.5** — ✅ agreed · 🟡 proposal awaiting confirmation
> Last updated: 2026-09-30

A local-first, file-based kanban and notes app for macOS, Windows and Linux. Every card is a
plain Markdown file; grouping is expressed with folders. The files stay fully usable without
the app. It includes an optional Jira integration: browse issues, copy them into boards, and
mirror whole Jira boards.

---

## 1. Goals & non-goals

**Goals**
- Very lightweight and fast: small binary, low memory, instant interactions.
- A best-in-class feel: smooth drag-and-drop, an Obsidian/Notion-style editor, a VS Code-style command palette, polished visuals.
- Local first. No accounts and no special OS permissions. **The only network access is the opt-in Jira integration**, limited to the configured Jira sites.
- Resilient: survives crashes, rapid edits, external edits (git, Dropbox, text editors), boards moved around the file system, and loss of connectivity.
- Extensible later: the core, including the Jira integration, is built on the same contribution points third-party extensions will use.

**Non-goals (v1)**
- Cloud sync or multi-user collaboration (other than mirroring Jira).
- Loading third-party extensions. The API is designed now and shipped later (§14).
- Built-in optional modules such as AI summaries. These will be extensions.
- Mobile.

---

## 2. Tech stack ✅

| Layer | Choice | Why |
|---|---|---|
| Shell | **Tauri 2** | Uses the system webview. Binary ~5–10 MB. |
| Core / backend | **Rust** | Owns the file system, watcher, discovery, search, history and Jira sync. It is the single writer to disk and the only component that talks to the network. |
| UI | **Svelte 5 + TypeScript + Vite** | Compiles away, so almost no runtime. Fine-grained reactivity. |
| Editor | **CodeMirror 6** with custom live-preview extensions | Obsidian-style editing. |
| Search index | **SQLite FTS5** (`rusqlite`, bundled) | ~1.5 MB. A disposable cache. |
| File watching | `notify` crate | Cross-platform. |
| HTTP | `reqwest` + `rustls` | HTTPS only. No OpenSSL dependency. |
| Secrets | `keyring` crate → macOS Keychain / Windows Credential Manager / Linux Secret Service | Jira tokens never touch disk in plain text. |
| Styling | Hand-rolled design tokens (CSS variables), light/dark, Lucide icons | Full control over the look, no heavy UI kit. |

Drag-and-drop is **pointer-event based**, not HTML5 DnD. HTML5 drag-and-drop is buggy on Linux's WebKitGTK, and a custom engine gives 60 fps animations and precise drop zones.

---

## 3. Concepts

| Concept | Description |
|---|---|
| **Board** | A folder anywhere on disk, marked by a hidden `.luau/` directory. Types: **kanban**, **files** (§7.4), **jira** (a mirror, §9.4). |
| **Lane** (`k…`) | A column or row of a kanban or jira board. Rows vs columns is only a view toggle. |
| **Card** (`c…`) | A Markdown document. A card with children is a **group card**. |
| **Jira-linked card** | A local card copied from a Jira issue. It is locally owned, but shows live Jira info and Jira actions (§9.3). |
| **Tag** | `#word` in a card body (local). Jira *labels* are separate and shown differently. |
| **Link** | `[[c8x1q0]]`, a card-to-card reference by ID, displayed as the card's title. |
| **Command** | Any user action, with an ID, title and handler. It powers the palette, shortcuts, menus and extensions. |
| **Tab** | A **board**, or a **document from a files board** (§7.1). |

### 3.1 IDs ✅
- A prefix plus 6 random characters from `[a-z0-9]`. `c` = card, `k` = lane, `b` = board.
- Lowercase only, because macOS and Windows file systems are case-insensitive.
- Uniqueness is checked against all discovered boards. On a rare collision (e.g. an import), the node gets a new ID and every link to it is rewritten.

### 3.2 Human names everywhere ✅
IDs are never shown in the UI.
- Card → its `# Title` (empty → *Untitled*). Lane → `name`. Board → `name`.
- Jira items also show their issue key (`PROJ-123`) next to the title, because the key is a human identifier.
- `Copy Card ID`, `Reveal in Finder/Explorer`, and Settings → Advanced → *Show IDs* are available for power users.

---

## 4. On-disk format ✅

```
Project Alpha/                   ← board root: any folder name, anywhere
├── .luau/                   ← hidden, app-internal; also the board marker
│   ├── board.json               ← board manifest
│   ├── jira.json                ← Jira links/mirror state (only if used, §9.5)
│   ├── history/                 ← operation journal + content blobs (§12)
│   ├── trash/                   ← deleted items, restorable
│   └── cache/                   ← disposable: summaries, thumbnails, Jira field cache
├── k4m2p9/                      ← lane
│   ├── index.json               ← { id, name, order }
│   ├── c8x1q0.md                ← plain card
│   ├── c8x1q0.3f9a.png          ← image owned by c8x1q0
│   └── c2mz7p/                  ← group card
│       ├── index.md
│       ├── index.json           ← { id, order }
│       ├── c77ab1.md
│       └── c0k2dd/              ← nested group (unlimited depth)
│           ├── index.md
│           └── index.json
└── k7q1ww/
    └── index.json
```
A **files board** has no lane folders. Nodes sit in the root, and the root's order is kept in `board.json`.

### 4.1 Manifests
```jsonc
// .luau/board.json
{ "schema": 1, "id": "b9za0e", "name": "Project Alpha",
  "type": "kanban",                              // "kanban" | "files" | "jira"
  "lanes": ["k4m2p9", "k7q1ww"],                 // kanban / jira
  "order": [],                                   // files only
  "view": { "orientation": "columns" },
  "tagColors": { "urgent": "#e5484d" } }

// <lane>/index.json
{ "schema": 1, "id": "k4m2p9", "name": "Backlog", "order": ["c8x1q0", "c2mz7p"] }

// <group>/index.json
{ "schema": 1, "id": "c2mz7p", "order": ["c77ab1", "c0k2dd"] }
```
Card files are **pure Markdown**, with no front-matter. Titles and tags are never duplicated into JSON.

### 4.2 Group card lifecycle ✅
- Dropping a card onto a plain card: `c1.md` becomes `c1/index.md` + `index.json`, and images move in.
- When the last child leaves, the group automatically turns back into a plain card and its images move back out.

### 4.3 Images ✅
- Pasted or dropped images are copied into the board: next to a plain card as `<cardId>.<rand4>.<ext>`, or inside a group card's folder.
- Images move with their card everywhere, including cross-board moves. Deleting a card sends its images to trash.

### 4.4 Recovery on load
- `order` arrays are the truth for ordering; the file system is the truth for existence.
- Unlisted files are appended at the end. Listed IDs with no file are dropped.
- Malformed JSON is backed up to `cache/recovered/` and rebuilt from the files, with a warning badge on the board.
- Unknown files are ignored and left untouched.

---

## 5. Cards, Markdown & links ✅

### 5.1 Title
- The first line must be `# Title`. If removed, the editor restores `# Untitled`. It is styled as a large H1.

### 5.2 Tags
- Syntax: `#` followed by `[\p{L}\p{N}_-]+`, at the start of a line or after whitespace, anywhere in the body.
- Code, URLs and link targets are ignored. `# Heading` is a heading, not a tag.
- Colours are automatic (a stable hash into a curated palette) and can be overridden per board. They are shown as chips on the card footer.

### 5.3 Card-to-card links
- **Stored** as `[[c8x1q0]]`. **Displayed** as a chip with the target's current title, and the board name when the target is on another board.
- Resolution: the current board first, then every discovered board. Created with `[[` autocomplete or `Insert Card Link…`.
- The chip is atomic (Backspace deletes it whole). Clicking it opens the target. `⌥/Alt-click` or `Edit Link…` changes the target. The raw `[[id]]` is visible only in source mode.
- Broken links show a *Missing card* chip, with *Restore from trash* when possible.
- The editor footer lists **backlinks**. Markdown export replaces links with titles.

### 5.4 Card face on the board
1. **Title**, plus the issue key for Jira cards.
2. **Structured preview:** the first checklist, bullet list or table, truncated. Checklists show a progress badge and can be ticked on the face.
3. **Otherwise, an extractive summary** (§13).
4. **Footer:** tag chips and indicators (images, links, backlinks, child count).
   - **Jira cards** add a Jira strip: status pill · assignee avatar · priority icon · labels (outlined chips, visually distinct from local tags).

Group cards render their children inside themselves and can be collapsed. The collapsed state is app-local UI state.

### 5.5 Editor (Obsidian/Notion feel)
- CodeMirror 6 **live preview**: Markdown marks are hidden when the cursor isn't on that line. Clickable checkboxes, inline images, tables rendered as a widget.
- A **slash menu** (`/`), `#` and `[[` autocomplete, pasting or dropping images.
- A source/preview toggle. Autosave 300 ms after typing stops, plus a flush on close or blur.
- Opens as a **modal** or a **right sidebar** (a setting). Files-board documents open full-page in their tab (§7.4).
- **Jira cards** show a Jira header section in the editor: key, status, type, assignee, reporter, priority, labels, sprint, "Open in Jira", and action buttons (§9.3).

---

## 6. Drag-and-drop ✅
- Reorder within a lane, move between lanes, reorder lanes.
- **Drop zones on a target card:**
  - top 30% of the card: insert above;
  - middle 40%: nest into it;
  - bottom 30%: insert below.
- Groups drag as a unit. Nesting depth is unlimited, with a visual indent cap and breadcrumb headers for deep levels.
- Auto-scroll near edges. Hovering a collapsed group expands it.
- The explorer accepts the same drops, including **cross-board moves** (images come along, IDs are kept, the move is journaled in both boards).
- **Jira sources:** dragging an issue from the **Jira panel**, or a card from a **Jira mirror board**, into a local board **copies** it as a Jira-linked card (§9.3). The original stays in Jira and in the mirror.
- Every drag action also exists as a keyboard shortcut and a palette command.

---

## 7. App layout

```
┌──────────────────────────────────────────────────────────────────────────┐
│ ● ● ●  [Project Alpha ×] [📄 Pasta recipe ×] [◆ PROJ Sprint ×]  +   ⇅ ⚙   │
├────────────────────────┬─────────────────────────────────────────────────┤
│[Exp|Srch|Hist|Jira|Ext]│  Backlog        Doing         Done               │
│ ▾ Project Alpha        │  ┌────────┐   ┌────────┐    ┌────────┐          │
│   ▾ Backlog            │  │ card   │   │ group  │    │ PROJ-12│          │
│     • Fix login        │  └────────┘   │ ┌────┐ │    │ ● Done │          │
│     ▸ Onboarding       │               │ │sub │ │    └────────┘          │
│ ▸ Notes                │               └────────┘                        │
└────────────────────────┴─────────────────────────────────────────────────┘
```

### 7.1 Tab bar ✅
- Tabs hold either a **board** (any type) or a **document from a files board**, including nested documents.
- Cards and subcards of kanban or jira boards never get their own tab; they open in the modal or sidebar.
- A board or document that is already open is focused instead of duplicated.
- New, close, reopen closed, drag to reorder, middle-click to close. Tabs are restored on launch.
- Tab icons tell the types apart: board, document, and Jira mirror (◆).
- A custom title bar. Only the active tab is mounted in the DOM.

### 7.2 Left panel ✅
- Can be hidden (`⌘B`). A segmented selector at the top: **Explorer | Search | History | Jira | Extensions**.
- The Jira segment is a panel contributed by the Jira integration through the same `panels` contribution point extensions will use (§14).

### 7.3 Orientation toggle ✅
- **Columns** (default) or **Rows**, per board, for kanban and jira boards.

### 7.4 Files boards ✅
- No lanes and no board view. A tree of Markdown documents, navigated through the explorer and quick open.
- Opening a document opens (or focuses) a **document tab** with a full-page editor, a breadcrumb and backlinks.
- The board tab itself shows recent documents and a "New document" button.
- **Conversion commands (v1):**
  - **Files → Kanban**: all root nodes go into an "Inbox" lane.
  - **Kanban → Files**: each lane becomes a group document titled with the lane name.
  - Document tabs of a converted board are closed.
- Jira mirror boards cannot be converted. Use "Copy Board as Local…" instead.

---

## 8. Left panel sections

### 8.1 Explorer ✅
- A virtual file-system tree: Boards → Lanes → Cards → Subcards, collapsible, **human names only**.
- Files boards show Documents → Subdocuments. Jira mirror boards show ◆ and their Jira columns.
- Click to open, drag to move (including across boards), F2 to rename, context menu. The tree is virtualized.

### 8.2 Search ✅
- Full-text search across all discovered boards, including Jira mirror boards (they are local files).
- Filters: board(s), lane, tag(s), type, has open tasks, has images, links to / linked from, modified date. Jira filters: status, assignee, issue type (these apply to Jira cards).
- The Jira panel has its own **remote** search (§9.2).

### 8.3 History ✅ → §12.

### 8.4 Jira ✅ → §9.2.

### 8.5 Extensions ✅
- v1: a search bar with an empty state. No built-in modules.

### 8.6 Board discovery ✅
- The `.luau/board.json` marker holds a stable ID. A registry cache maps `id → last path`.
- A background incremental crawler walks the search roots (default: home folder), skipping system and heavy folders.
- Moved boards are relocated by ID. Copies are detected by a duplicate ID, and the app offers a new ID.
- The one-time macOS permission prompts for Desktop, Documents and Downloads are accepted and appear lazily.

---

## 9. Jira integration ✅ (details 🟡)

### 9.1 Connection & security
- **Accounts:** one or more Jira sites, added in Settings → Jira. **Cloud and self-hosted are fully supported** ✅, as two adapters behind the same port (§9.7):

  | | Jira Cloud | Jira Server / Data Center |
  |---|---|---|
  | REST | `/rest/api/3` (ADF descriptions), `/rest/agile/1.0` | `/rest/api/2` (wiki-markup descriptions), `/rest/agile/1.0` |
  | Search | `/search/jql` + `nextPageToken` | `/search` + `startAt` |
  | Auth | email + API token (Basic) | Personal Access Token (Bearer, DC ≥ 8.14); username + password (Basic) for older servers, with a warning |
  | Users | `accountId` | `name` / `key` |
  | Network | public HTTPS | HTTPS with **custom CA certificate** import per site, system/corporate **proxy** support |

  - The flavour and version are detected automatically from `/rest/api/2/serverInfo` when a site is added.
  - OAuth 2.0 could be added later. It needs a registered app and a local redirect listener.
- **Token storage:** tokens are kept in the OS keychain and never written to settings, logs, exports or history.
  - Linux without a running Secret Service: the token is kept in memory for the session only, with a warning.
- **Network rules:**
  - HTTPS only, and only to the configured site hosts (an allow-list checked in Rust before every request).
  - Redirects to other hosts are refused. The webview itself has **no network access** (CSP `connect-src` is limited to IPC).
  - Avatars are downloaded by Rust and served from the local cache.
- **Personal data:** Jira data (names, emails, avatars, issue content) is cached locally only as long as needed.
  - Removing an account purges its cache.
  - Logs contain issue keys and HTTP status codes only; never tokens, emails or bodies.
- **Rate limiting:** honours `429` and `Retry-After`, with exponential backoff and jitter. Requests are batched (`key in (…)` in chunks of 100).
- **Offline:** everything keeps working from the last synced state. A banner shows "Jira offline · last synced 10:42", and Jira actions are disabled until the connection returns.

### 9.2 Jira panel (left sidebar)
- An account/site switcher at the top.
- **Sections:** My open issues · Recently viewed · Projects → Boards · Saved filters.
- **Search:** a JQL box with autocomplete, plus a simple filter builder (project, status, assignee, type, labels, sprint, text) for users who don't write JQL.
- Results are virtualized and paginated (Jira Cloud's `/search/jql` with `nextPageToken`). Each row shows key, title, status, assignee and type icon.
- **Drag an issue into any local board or files board** to create a **Jira-linked copy** at the drop position.
- On a Jira board entry: **Mirror this board…** (§9.4).

### 9.3 Jira-linked cards (copies in local boards)
- **Creation:** by dragging from the Jira panel or a mirror board, or with the `Add Jira Issue…` command.
  - A normal local card is created: title = Jira summary, body = the description converted to Markdown (ADF / wiki markup → MD), plus a footer line `Jira: [PROJ-123](https://…)`, so the file stays understandable without the app.
  - The link (card ID → site, issue ID, key) is stored in `.luau/jira.json`, not in the Markdown.
- **Local ownership:** moving, reordering, grouping or editing the card in the app **never changes anything in Jira** ✅.
- **Live Jira info:** the Jira strip (status, assignee, priority, labels, sprint) is refreshed from Jira on every sync cycle. The local body is **never** overwritten automatically.
- **Jira actions** (the card menu, the editor header, and palette commands with `when: cardIsJira`). These are the **only** operations that write to Jira ✅. Each one goes through the external-write confirmation (§9.6):
  - Transition status… (lists the transitions available for that issue)
  - Assign to… / Unassign (user search)
  - Edit labels… · Set priority… · Add comment…
  - Open in Jira · Refresh from Jira
  - Pull description from Jira (replaces the body, after a confirmation and with a diff preview)
  - Unlink from Jira (becomes a plain local card)
- Jira actions are not on the local undo stack (they are remote side effects). A toast offers the obvious reverse action where possible, e.g. "Transitioned to Done · Revert".
- Dropping an issue that is already on the target board shows a warning and a "Jump to existing" option.
- If the issue is deleted in Jira or access is lost, the card is kept and marked *Jira issue unavailable*.

### 9.4 Jira mirror boards (whole-board import, auto-synced)
- **Creation:** "Mirror this board…" in the Jira panel, then pick a destination folder, then pick a name.
  - This creates a normal board folder with `type: "jira"`, so it is discoverable, searchable, has history and is readable without the app.
- **Jira is the source of truth.** The app keeps the mirror identical to the Jira board:
  - **Lanes = Jira board columns.** Columns added, removed, renamed or re-ordered in Jira are applied automatically.
  - **Cards = issues on the board**, ordered by Jira **rank**. Sub-tasks are nested inside their parent as group cards.
  - **Sprint / Backlog views** ✅: a segmented switch in the board header, the same pattern as Jira.
    - **Sprint** (the default for scrum boards) shows the active sprint in the board's columns.
    - If several sprints are active at once, a sprint filter chip lets you pick which to show.
    - **Backlog** (for scrum boards, and for kanban boards with a backlog enabled) shows one lane per future sprint plus a final "Backlog" lane, ordered by rank. It is read-only like the rest of the mirror.
    - The board's column lanes never get polluted with backlog items. Both views are stored in the same board folder and switched in the UI.
  - Card content: summary → title, description → Markdown, the Jira strip, and the Jira header in the editor.
- **Local edits are not allowed** on mirror boards ✅. The body is read-only, and **dragging between lanes inside a mirror is disabled**; the drag is shown as a "not allowed" cursor with a hint to use *Transition…*. Jira actions are available as in §9.3, and the resulting change comes back through the sync.
- **Copying out** ✅: dragging a mirror card into any **non-Jira board** (kanban or files) creates a Jira-linked copy, exactly as if it came from the Jira panel (§9.3), with the same Jira strip and actions. The mirror is untouched.
- **Sync engine:**
  - Incremental polling with JQL `updated >= lastSync`, every 2 minutes by default (configurable), plus when the window gains focus and on "Refresh". Webhooks aren't possible for a local app.
  - Board configuration (columns and status mapping) is checked every 10 minutes and on refresh.
  - A lightweight reconciliation (fetching keys only) detects issues that were deleted or moved out of the board.
  - Each change is applied through the board's single-writer actor as `RemoteSync` operations, journaled in history (marked "from Jira").
  - Idempotent: the key ↔ card-ID and column ↔ lane-ID maps live in `.luau/jira.json`, so a re-sync never duplicates anything.
- A mirror board that has lost its account or access keeps its last state, read-only, with a "Reconnect" banner.

### 9.5 `jira.json`
```jsonc
{ "schema": 1,
  "site": "https://acme.atlassian.net",           // account reference; the token is in the keychain
  "mirror": { "boardId": 42, "lastSync": "…", "columns": { "10001": "k4m2p9" } },  // jira boards only
  "issues": { "c8x1q0": { "key": "PROJ-123", "id": "10234" } } }                     // links and mirrored cards
```

### 9.6 External-write confirmation ✅ (development-phase policy)
**Every** operation that changes data in an external service goes through a single **WriteGate** in the Rust application layer. This covers Jira today and any future provider or extension, so nothing can bypass it. Before the request is sent, a modal is shown:

```
┌──────────────────────────────────────────────────────────┐
│  Update Jira?                                            │
│  This will update Status and Assignee on Jira            │
│  (acme.atlassian.net) for PROJ-123 "Fix login bug":      │
│                                                          │
│    Status     In Progress  →  Done                       │
│    Assignee   Ana Pérez    →  Unassigned                 │
│                                                          │
│                              [ Cancel ]  [ Confirm ]     │
└──────────────────────────────────────────────────────────┘
```
- It lists the service, site, item, and each field with **before → after** values. Several changes from one action are listed in a single dialog.
- `Esc` or Cancel aborts, and nothing is sent. The focus defaults to **Cancel**.
- There is no "don't ask again" during development. The policy is the setting `integrations.confirmExternalWrites` (default `true`), locked on for now.
- Confirmed writes are journaled as `ExternalWrite` in history (origin *You → Jira*), with the fields changed.

### 9.7 Provider abstraction (for Linear, GitHub, Trello later)
Jira is implemented behind a generic **`IssueProvider` port**:
```
search(query, page) · getIssues(ids) · getBoard(boardRef) · listChanges(since)
getTransitions(id) · transition(id, t) · assign(id, user) · setLabels(id, labels)
setPriority(id, p) · addComment(id, md) · searchUsers(q)
```
Plus normalized types (`RemoteIssue`, `RemoteBoard`, `RemoteColumn`, `RemoteUser`, `Transition`) and a **capabilities** flag set, so a provider without priorities simply hides that action.
- The panel UI, the Jira strip, linked cards, mirror boards and the sync engine all work against the port. Adding Linear means writing one adapter.
- The same port will be exposed to extensions as the `issueProviders` contribution point.

---

## 10. Command system & palette ✅

### 10.1 Command registry
Every action is a command, `{ id, title, category, when, args, run }`, used by the palette, shortcuts, menus, context menus and (later) extensions.

### 10.2 Palettes
| Shortcut | Mode |
|---|---|
| `⌘P` | Quick Open: cards, documents, boards by title (Jira keys match too) |
| `⇧⌘P` | Command Palette (`>` prefix) |
| `#` · `@` · `?` prefixes | Tags · lanes/headings · help |

Fuzzy matching, recently used commands first, keybindings shown on each row, and `when`-aware filtering.

### 10.3 Multi-step input (VS Code QuickInput style)
- QuickPick (single/multi, fuzzy) and InputBox (validated) steps, with a step counter. Back with `⌫` on an empty field or `⌥←`. `Esc` cancels.
- Examples:
  - `Move Card…` → board → lane → position.
  - `Insert Table…` → `3x4`.
  - `Insert Link…` → Card or URL.
  - `New Board…` → type → folder → name.
  - `Jira: Transition…` → pick a transition.
  - `Jira: Assign…` → user search.
  - `Jira: Mirror Board…` → site → project → board → folder.

### 10.4 Command catalogue (v1)
- **App:** Open Settings, Open Keyboard Shortcuts, Open Settings (JSON), Toggle Left Panel, Focus Explorer/Search/History/Jira/Extensions, Toggle Theme, Reload Window, Rescan Boards.
- **Tabs:** New Tab, Close Tab, Reopen Closed Tab, Close Other Tabs, Next/Previous Tab.
- **Board:** New Board…, Open Board…, Rename Board…, Convert Board Type…, Copy Board as Local…, Toggle Rows/Columns, Export Board…, Import…, Reveal in Finder/Explorer.
- **Lane:** New Lane…, Rename Lane…, Delete Lane, Move Lane Left/Right.
- **Card/File:** New Card…, New Subcard…, New Document…, Open Card…, Rename…, Duplicate, Delete, Restore from Trash…, Move Card…, Move to Lane…, Group Into…, Ungroup, Move Up/Down, Indent/Outdent, Copy Card Link, Copy Card ID, Reveal in Finder/Explorer.
- **Editor, text:** Toggle Upper/Lower Case, Title Case, Sentence Case, Sort Lines, Remove Duplicate Lines, Join Lines, Duplicate Line, Move Line Up/Down, Trim Trailing Whitespace.
- **Editor, Markdown:** Toggle Bold/Italic/Strikethrough/Inline Code/Highlight, Heading 1–6, Toggle Bullet/Numbered/Checklist, Toggle Task Done, Toggle Quote, Insert Code Block…, Insert Table…, Add Row/Column, Format Table, Insert Link…, Insert Card Link…, Insert Image…, Insert Divider, Insert Date/Time, Toggle Source/Preview.
- **History:** Undo, Redo, Show History for Card/Board, Summarize Activity…
- **Jira:** Add Account…, Search Issues…, Add Jira Issue…, Mirror Board…, Sync Now, Transition…, Assign…, Unassign, Edit Labels…, Set Priority…, Add Comment…, Open in Jira, Refresh from Jira, Pull Description from Jira, Unlink from Jira.

---

## 11. Keyboard shortcuts & keybinding editor ✅

### 11.1 Model (VS Code-compatible)
- `keybindings.json` stores **user overrides only**. Defaults come from the core (and later from extensions). A `-command` entry removes a default.
- **Chords:** two steps, e.g. `⌘K ⌘S`, with a "waiting for second key" hint.
- **When clauses:** `boardFocus`, `editorFocus`, `paletteOpen`, `explorerFocus`, `cardSelected`, `cardIsJira`, `boardType == 'files'`, `boardType == 'jira'`, `editorHasSelection`, …
- Platform-aware (`cmd`/`ctrl`). Keys are recorded by physical key code, so they work on non-US layouts.

### 11.2 Keybinding editor (`⌘K ⌘S`)
- A table: Command · Keybinding · When · Source (*Default / User / Extension*).
- Search by title or ID, or use **record-keys mode** to search by pressing the actual shortcut.
- Double-click a row to open the recording overlay (chords allowed).
- Live conflict detection. A context menu to change, add, remove or reset a binding, change its when clause, or show other commands with the same keybinding.
- Extension and integration commands (including Jira) are listed automatically, labelled with their source.

### 11.3 Default shortcuts (⌘ = Ctrl on Windows/Linux)
| Action | Shortcut |
|---|---|
| Command palette / Quick open | `⇧⌘P` / `⌘P` |
| New card / new lane / new board | `⌘N` / `⇧⌘N` / `⌥⌘N` |
| Open board / new tab | `⌘T` |
| Close tab / reopen closed tab | `⌘W` / `⇧⌘T` |
| Next / previous tab · go to tab N | `⌃Tab` / `⌃⇧Tab` · `⌘1`…`⌘9` |
| Toggle left panel | `⌘B` |
| Explorer / Search / History / Jira / Extensions | `⇧⌘E` / `⇧⌘F` / `⇧⌘H` / `⇧⌘J` / `⇧⌘X` |
| Undo / Redo | `⌘Z` / `⇧⌘Z` (+ `Ctrl+Y` on Windows) |
| Navigate cards · open · close | Arrows · `Enter` · `Esc` |
| Move card up/down · to prev/next lane | `⌥↑`/`⌥↓` · `⌥←`/`⌥→` |
| Nest into card above / outdent | `Tab` / `⇧Tab` |
| Delete (to trash) · rename | `⌫`/`Del` · `F2` |
| Toggle rows/columns | `⇧⌘L` |
| Toggle source/preview | `⌘E` |
| Bold / italic / link / card link | `⌘B`* / `⌘I` / `⌘K`* / `⌘⇧K` |
| Jira: transition / assign (on a Jira card) | `⌘J ⌘T` / `⌘J ⌘A` |
| Settings / keyboard shortcuts | `⌘,` / `⌘K ⌘S` |

\* These are resolved by context. `⌘B` means Bold in the editor and Toggle Left Panel elsewhere. `⌘K` means Insert Link in the editor and is a chord prefix elsewhere.

---

## 12. Undo/Redo & History ✅

### 12.1 Operations & undo
- Every local change is a typed, invertible operation: `CreateNode`, `EditCard`, `MoveNode` (including across boards), `Group`/`Ungroup`, `RenameLane`, `DeleteNode`, `ReorderLanes`, `ConvertBoard`, `Import`, `LinkJira`/`UnlinkJira`, …
- Per-tab in-memory undo/redo stacks, with a precondition check before each undo. A finished editor session is one step.
- `RemoteSync` (changes from Jira) and Jira actions are journaled but **not undoable** locally.

### 12.2 Persistent history
- A per-board append-only journal (daily JSONL segments) plus content-addressed, gzip-compressed text blobs.
- External edits and Jira syncs are journaled and labelled by their origin: *You · External edit · Jira*.
- **History view:** a timeline filterable by board, lane, card, operation type and origin. A diff view with Restore (for local boards). Restore from trash.
- Retention defaults: structure entries forever; text blobs pruned after 180 days or at 50 MB per board.

### 12.3 Activity summary ("what did I do yesterday?") ✅
- Available from the History panel button and `Summarize Activity…` (period → scope).
- **Deterministic, built from the journal**, grouped by board, with human titles and clickable links.
- **Stage inference from lane names** ✅. No per-board configuration.
  - A multilingual keyword dictionary classifies each lane as:
    - **Done:** done, complete(d), finished, closed, resolved, shipped, released, deployed, archived, hecho, terminado, listo, completado, cerrado, finalizado, resuelto, entregado.
    - **In progress:** doing, in progress, wip, working, active, started, review, testing, QA, en curso, en progreso, haciendo, revisión.
    - **To do:** todo, to do, backlog, next, planned, inbox, ideas, pendiente, por hacer.
    - **Other:** anything else.
  - Matching is case- and accent-insensitive, on whole words, and ignores emoji and numbering (so "✅ Done", "3. Hecho" and "DONE!" all match).
  - For Jira cards and mirror boards, Jira's own `statusCategory` (To Do / In Progress / Done) takes precedence over the lane name.
- Sections: **Completed** (entered a Done stage) · **Started** (entered In progress) · Created · Moved (net moves only) · Edited · Deleted · Jira actions performed.
  - Changes that came *from* Jira syncs are excluded by default, with a toggle to include them.
- Output: a Markdown preview with **Copy**, **Save as card…** and **Open as document**. Extension point: `activitySummarizers`.

---

## 13. Card summaries ✅
- A core, extractive summary (TextRank-style, in Rust, about a millisecond, deterministic), cached by content hash.
- Extension point `cardSummarizers`, for future AI extensions.

---

## 14. Extension architecture (designed now, shipped later) ✅
**Principle: the core uses the same contribution points extensions will use.** The Jira integration is the first real consumer. It registers its panel, commands, keybindings, settings, card-face strip and issue provider through these registries.

**Manifest** (`<appData>/extensions/<publisher.name>/manifest.json`):
```jsonc
{ "id": "acme.pomodoro", "name": "Pomodoro", "version": "1.0.0", "engine": "^1.0.0",
  "main": "dist/extension.js",
  "permissions": ["boards:read", "boards:write", "network:api.acme.com"],
  "contributes": {
    "commands": [], "keybindings": [], "settings": {}, "panels": [],
    "cardFaceProviders": [], "cardActions": [], "cardSummarizers": [],
    "activitySummarizers": [], "issueProviders": [], "importers": [], "exporters": [] } }
```
**Runtime (planned):**
- Extensions run in a sandboxed iframe or Web Worker and use a message-based API only (`mk.commands`, `mk.boards`, `mk.ui.quickPick`, `mk.settings`, `mk.net`, …).
- No direct access to IPC, the file system or the network. Capabilities are permission-gated and approved at install time; network access is allow-listed per domain and proxied through Rust.

**v1 deliverables:** the registries and contribution points, the Extensions panel shell (search bar, empty state), and Settings, shortcuts and palette listing contributions by source. **No loader yet.**

---

## 15. Import / Export ✅
- **Standard interchange model:** a normalized JSON of board, lanes and cards (title, Markdown body, tags, links, attachments, parent, order, `extra`).
  - Every importer or exporter is only a *Source ↔ Interchange* mapper.
  - The core applies an import as one journaled `Import` transaction (one undo step). A key → ID map makes re-imports idempotent.
- **Export:**
  - board `.zip` (the folder without `cache/`; history is optional);
  - a single Markdown file (links replaced with titles);
  - interchange JSON.
  - Jira mirrors export as local snapshots.
- **Import:** board `.zip` or folder · a folder of `.md` files (subfolders become groups) · interchange JSON.
- Jira is **not** a file import: it is the live integration in §9. Future Trello, Linear and GitHub support can be an importer or an `IssueProvider`.

---

## 16. Settings ✅
A VS Code-style Settings view with search, a category sidebar, modified indicators and reset buttons, plus "Open Settings (JSON)". Stored in `<appConfig>/settings.json`. Board-specific settings live in `board.json`.

| Category | Settings |
|---|---|
| **General** | Restore tabs, open on start, confirm before delete, language (English v1, i18n-ready), check for updates (manual) |
| **Appearance** | Theme, accent colour, density, UI font & size, card and lane width, animations, card indicators |
| **Board** | Default type, default orientation, card face options, group collapse default, new-card position |
| **Editor** | Open mode (modal/sidebar), live preview default, font & size, line width, line numbers, spell check, autosave delay, tab size, smart lists, auto-pair, image naming |
| **Tags & Links** | Tag palette, auto-colour, link chip style, backlinks |
| **Keyboard Shortcuts** | → keybinding editor |
| **Jira** | Accounts (add, test connection, remove and purge), sync interval, sync on focus, default JQL for the panel, scrum backlog option, show labels on face, avatar cache, clear Jira cache |
| **Discovery** | Search roots, exclusions, rescan interval, "Rescan now" |
| **Search** | Included boards, rebuild index |
| **History** | Retention, max size, version interval, "Prune now" |
| **Import/Export** | Default format, include history in zip |
| **Extensions** | (empty in v1) |
| **Advanced** | Show IDs, log level, open logs/data folders, reset all |

---

## 17. Performance, memory & resilience ✅

**Budgets (targets)**
- Cold start < 400 ms. Tab switch < 50 ms. Palette opens in < 30 ms.
- Drag-and-drop at 60 fps with 1,000 cards on a board.
- Memory use under 120 MB with 5 boards and 5,000 cards (the webview baseline is about 40–80 MB). Binary < 15 MB.
- Jira sync runs entirely in the background and never blocks the UI thread. With nothing new to fetch, an incremental sync is a single request.

**How**
- Rust keeps metadata only in memory. Card bodies are loaded on demand.
- A single-writer actor per board serializes all mutations: local, external and Jira sync.
- Optimistic UI with coalesced write-behind. Atomic writes (temp file, fsync, rename).
- Watcher self-suppression by content hash. A merge prompt appears on conflicting external edits.
- An in-memory link index. Virtualized lanes, trees and lists. Flush on blur, close and quit.
- Jira: a request queue per site with concurrency 4 and backoff. Field projection (only needed fields are requested). ETag and `updated` caching.

---

## 18. Architecture

```
┌──────────────────────────── UI (Svelte, TS) ─────────────────────────────┐
│ Tabs · Board/Files/Doc views · DnD engine · Editor (CM6) · Left panel    │
│ Command registry · Keybinding resolver · QuickInput · Settings UI        │
│ Contribution registries ← core features, Jira integration, (extensions)  │
└───────────────── Tauri IPC (typed commands + events) ────────────────────┘
┌──────────────────────────────── Rust core ───────────────────────────────┐
│ application: command handlers, per-board actor, undo, journal, import,   │
│              sync engine (IssueProvider → RemoteSync ops)                │
│ domain:      tree, ops + inverses, recovery, ids, parsing (title, tags,  │
│              links, face), summaries, stage inference, interchange,      │
│              remote models (RemoteIssue/Board/…)      (pure, no IO)      │
│ ports:       BoardStore, Watcher, SearchIndex, HistoryStore, Discovery,  │
│              IssueProvider, SecretStore, HttpClient                      │
│ adapters:    fs (atomic), notify, SQLite FTS5, crawler, keyring,         │
│              jira-cloud / jira-dc (reqwest+rustls, host allow-list)      │
└──────────────────────────────────────────────────────────────────────────┘
```

---

## 19. Testing
- **Rust unit tests:** recovery, IDs, parsing (title, tags, links), face and summary extraction, operation inverses, **stage inference** (multilingual, accents, emoji), activity summary, interchange validation, **ADF / wiki markup → Markdown**, Jira JSON → remote models, sync diffing (columns, rank, sub-tasks, deletions).
- **Property-based tests:** random operation and undo sequences hold the invariants and match a reload from disk. Random interleavings of Jira sync and local operations never corrupt a board.
- **Integration tests:**
  - external edits, crashes mid-move, moved and copied boards, cross-board moves;
  - **Jira against a mock HTTP server:** pagination, 429/backoff, auth failure, offline, column changes, idempotent re-sync, host allow-list and redirect rejection, no token in logs.
- **UI tests (Vitest):** drop-zone maths, keybinding resolution (chords, when clauses, conflicts), fuzzy matching, QuickInput flows.
- **Manual QA:** macOS, Windows, Linux, plus a real Jira Cloud sandbox project.

---

## 20. Milestones
1. **M0:** scaffold, design tokens, window chrome, CI for 3 OSes.
2. **M1:** Rust domain and file-system store (kanban and files), recovery, watcher, board rendering.
3. **M2:** command registry, keybindings, palette and QuickInput.
4. **M3:** drag-and-drop engine, undo/redo, keyboard navigation.
5. **M4:** editor (live preview, slash menu, tags, links, images, text/Markdown commands), files-board document tabs.
6. **M5:** tabs, left panel, explorer, discovery, cross-board moves.
7. **M6:** search, quick open.
8. **M7:** history, trash, activity summary with stage inference.
9. **M8:** card faces, summaries, Settings, keybinding editor, Extensions shell, board conversion.
10. **M9:** Jira: accounts and keychain, panel and search, linked cards and actions.
11. **M10:** Jira mirror boards and sync engine.
12. **M11:** import/export, packaging (dmg, msi, AppImage/deb), performance and security pass.

---

## 21. Open questions
The final round is in [QUESTIONNAIRE.md](QUESTIONNAIRE.md). Its answers will be folded into this spec as v1.0.
