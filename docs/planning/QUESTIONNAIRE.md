# Luau — Final Questionnaire

Every question has a ⭐ **recommended default**.
**How to answer:** reply `defaults` plus your exceptions only, e.g. `defaults except 7b, 21c, 58: "my text"`.
Questions marked 🔴 have no safe default, so please answer those explicitly.

---

## A. Product, identity & distribution

1. 🔴 **App name.** It fixes the hidden folder name (`.luau/`), the bundle ID and the file-format identity, so it's costly to change later.
   a) ⭐ "Luau" (working name)
   b) Other: ___

2. 🔴 **Project ownership.** It decides the repo host, license, and whether a company process applies.
   a) Personal project
   b) Internal company tool
   c) Personal, but open source

3. **License** (if open source).
   a) ⭐ MIT
   b) Apache-2.0
   c) GPL-3.0
   d) Proprietary / none

4. **Repo & CI host.** Note: Bitbucket Pipelines has **no hosted macOS runners**, and macOS builds need one.
   a) ⭐ GitHub + Actions (hosted macOS, Windows and Linux runners)
   b) Bitbucket + self-hosted macOS runner
   c) Local builds only for now

5. **Code signing & notarization.** Unsigned builds trigger "unidentified developer" warnings. An Apple Developer ID costs about $99/yr; Windows certificates vary.
   a) ⭐ Unsigned during development; decide before any public release
   b) Sign from day one (you provide the certificates)

6. **Auto-update.** The Tauri updater needs a hosted update feed and a signing key, which means network access.
   a) ⭐ None in v1 (manual "check GitHub releases" link)
   b) Built-in updater

7. **Telemetry / crash reports.**
   a) ⭐ None. Local crash log only, with an "Open logs folder" button.
   b) Opt-in anonymous crash reports

8. **UI languages.**
   a) English only (i18n-ready)
   b) ⭐ English + Spanish from v1
   c) Other: ___

9. **Minimum OS versions.**
   a) ⭐ macOS 11+ · Windows 10 (1809+) · Ubuntu 22.04+ / Fedora 38+ (WebKitGTK 4.1)
   b) Other: ___

10. **Architectures.**
    a) ⭐ macOS universal (Apple Silicon + Intel) · Windows x64 + ARM64 · Linux x64
    b) Also Linux ARM64
    c) Apple Silicon only for now

11. **Linux package formats.**
    a) ⭐ AppImage + .deb
    b) + .rpm
    c) + Flatpak (sandboxed, so board discovery would need portal work)

---

## B. Development setup

12. **Toolchain.** Rust is **not installed** on your machine; Node 24 and npm 11 are. May I install Rust via `rustup` (user-level, no sudo) when we start M0?
    a) ⭐ Yes
    b) I'll install it myself

13. **JS package manager.**
    a) ⭐ pnpm
    b) npm
    c) bun

14. **Commits & branching.**
    a) ⭐ Conventional Commits, trunk-based, short-lived feature branches
    b) Other: ___

15. **Change documentation.** One `.md` per change under `docs/changes/` (objective, files, logic, decisions, risks, tests).
    a) ⭐ Yes, always
    b) Only for milestones

16. **CI quality gates.**
    a) ⭐ `cargo fmt` + `clippy -D warnings` + `cargo test` + `svelte-check` + `eslint` + `vitest` on every push; no coverage gate
    b) Also enforce a coverage threshold: ___ %

17. **Release scope.** Which milestones make the first build you'll actually use daily?
    a) ⭐ M0–M6: a local kanban with editor, palette, tabs, explorer and search. Then history, then Jira.
    b) Pull Jira earlier (right after M4)
    c) Everything before first use

---

## C. File format & data safety

18. **Schema from a newer app version.** An older app opens a board written by a newer one:
    a) ⭐ Open it read-only with a banner "Created with a newer version"
    b) Refuse to open it

19. **Line endings & encoding.**
    a) ⭐ UTF-8 only, preserve existing LF/CRLF and BOM, new files use LF
    b) Normalize everything to LF

20. **JSON formatting** (for git-diff friendliness).
    a) ⭐ Pretty-printed, 2 spaces, stable key order, trailing newline
    b) Minified

21. **Attachments beyond images.**
    a) Images only
    b) ⭐ Any file (PDF, zip, …) copied next to the card like images, shown as a file chip; images render inline

22. **Large images.**
    a) ⭐ Keep the original, show a lazy thumbnail
    b) Auto-downscale above ___ px
    c) Convert to WebP

23. **Unreferenced images/attachments** (removed from the Markdown but still on disk).
    a) ⭐ Moved to trash automatically 7 days after they stop being referenced (restorable), plus a "Clean up unused files" command
    b) Never auto-clean; command only

24. **Boards inside git repos.** Auto-create `.luau/.gitignore`?
    a) ⭐ Yes: ignore `cache/`, keep `history/` and `jira.json`
    b) Yes: ignore `cache/` and `history/`
    c) Don't touch it

25. **Symlinks inside a board.**
    a) ⭐ Ignored (never followed)
    b) Followed

26. **Nested boards** (a board folder inside another board's folder).
    a) ⭐ Not allowed. Creating one is blocked, and the crawler stops at board roots.
    b) Allowed; the inner board is a separate board

27. **Trash retention.**
    a) ⭐ 30 days, then permanently deleted (configurable; "Empty trash" asks for confirmation)
    b) Forever until emptied manually

28. **Archive** (hide done cards without deleting them).
    a) ⭐ Yes: an "Archive" action moves cards to `.luau/archive/`, keeping their history. They are searchable with `is:archived` and restorable.
    b) No; use a "Done" lane or trash

---

## D. Board & lanes UX

29. **Lane features in v1** (multi-select):
    a) ⭐ Collapse lanes
    b) ⭐ Lane colour
    c) ⭐ Per-lane resizable width
    d) WIP limits (a warning badge when exceeded)
    e) Lane icon or emoji

30. **Rows mode layout.**
    a) ⭐ Each row scrolls horizontally with fixed-width cards
    b) Cards wrap onto multiple lines within a row

31. **New board contents.**
    a) ⭐ A template picker: Empty · To do / Doing / Done · Backlog / Next / Doing / Review / Done
    b) Always empty

32. **Card templates.**
    a) ⭐ Yes: `.luau/templates/*.md` per board plus global templates in app data, chosen from "New card ▾" and the palette
    b) Not in v1

33. **Quick add.**
    a) ⭐ Trello-style inline "+ Add card" at the lane bottom (and top via `⌘N`). Typing a title and pressing Enter creates the card and keeps the input open for the next one.
    b) Always open the editor

34. **Multi-select cards** (⌘/Shift-click, rubber-band selection) for bulk move, tag, delete and archive.
    a) ⭐ Yes in v1
    b) Later

35. **Card cover image** (the first image in the card shown as a cover on the face).
    a) Yes, automatic
    b) ⭐ Opt-in per card (the "Set as cover" action records it in the lane or group `index.json`)
    c) No

36. 🔴 **Due dates, priority and assignees for local cards.** Card files are pure Markdown with no front-matter, so this needs inline syntax:
    a) ⭐ Inline tokens parsed from the body, e.g. `📅 2026-10-03`, `⏫` (priority), `@ana`, with pickers in the editor, shown on the face, sortable and filterable
    b) Front-matter after all (a YAML block at the top, before `# Title`)
    c) Not in v1; tags only

37. **Board quick filter** (a bar that hides or dims non-matching cards: text, tag, due date, Jira status).
    a) ⭐ Yes, `/` to focus it
    b) No, left-panel search only

38. **Group card children on the face.**
    a) ⭐ Compact child rows (title, tags, progress), expandable to full cards
    b) Always full cards

39. **Board zoom** (`⌘+` / `⌘-` / `⌘0`).
    a) ⭐ Yes, per board, remembered
    b) Global UI zoom only

40. **Lane sort commands** ("Sort lane by title, created, modified, due date" as a one-shot reorder).
    a) ⭐ Yes
    b) No

---

## E. Editor

41. **External file without a `# Title` first line** (e.g. created in another editor).
    a) ⭐ Never rewrite a file just because it was loaded. Show the first H1 found, or "Untitled", as the title. Insert `# Untitled` only when the user edits it in the app.
    b) Normalize immediately on load

42. **Markdown features** (multi-select):
    a) ⭐ GFM (tables, task lists, strikethrough, autolinks)
    b) ⭐ `==highlight==` and footnotes
    c) ⭐ Syntax-highlighted code blocks (languages lazy-loaded)
    d) Math (KaTeX, ~300 KB, lazy)
    e) Mermaid diagrams (~1 MB, lazy)
    f) Callouts `> [!note]` (Obsidian style)

43. **Spell check.**
    a) ⭐ On, using the OS spell checker (partial support on Linux)
    b) Off

44. **Vim mode.**
    a) ⭐ Optional setting, off by default
    b) Not needed

45. **Links typed by title** (the user types `[[Fix login]]` by hand without using autocomplete).
    a) ⭐ Converted to `[[c8x1q0]]` on blur if exactly one card matches; otherwise shown as an unresolved link with a "Create card" action
    b) Only autocomplete creates links

46. **Advanced links** (multi-select for v1):
    a) Heading links `[[c8x1q0#Section]]`
    b) Embeds `![[c8x1q0]]` (the target's content rendered inline)
    c) ⭐ Neither for v1

47. **Pasting rich content** (HTML from the web or Google Docs).
    a) ⭐ Converted to Markdown automatically, with plain-text paste on `⇧⌘V`
    b) Always plain text

48. **Editing a group card.**
    a) ⭐ The editor shows `index.md` plus a "Children" section below it (a linked list, drag to reorder, "+ Add subcard")
    b) Only `index.md`

49. **Editor width.**
    a) ⭐ A readable max width (~72 characters) with a setting for full width
    b) Always full width

50. **Card modal/sidebar navigation.**
    a) ⭐ `⌘[` / `⌘]` go back and forward through recently opened cards, and ↑/↓ with `⌥` open the previous or next card in the lane
    b) Not needed

---

## F. Tags

51. **Nested tags** (`#work/client-a`, shown as a tree in search and explorer filters).
    a) ⭐ Yes
    b) No, flat only

52. **Rename tag everywhere** (one undoable batch that rewrites every file using it).
    a) ⭐ Yes, with a scope choice: this board or all boards
    b) No

53. **Tag colours scope.**
    a) ⭐ Global colours in app settings, with an optional per-board override stored in `board.json`
    b) Per board only
    c) Global only

---

## G. Tabs, windows & layout

54. **Multiple windows** (drag a tab out into a new window).
    a) Yes in v1
    b) ⭐ Later (v1 is single-window, but the architecture allows it)

55. **Split view** (two boards or documents side by side in one window).
    a) Yes in v1
    b) ⭐ Later

56. **Pinned tabs** (a compact icon-only tab that `⌘W` won't close).
    a) ⭐ Yes
    b) No

57. **No tabs open.**
    a) ⭐ A start page: recent boards, recent documents, "New board", "Open folder", shortcuts cheat sheet
    b) An empty window

58. **Card editor sidebar** (when chosen in settings).
    a) ⭐ Resizable and pinnable, so it stays open while you click through cards
    b) Fixed width

59. **Native menus.**
    a) ⭐ A full macOS menu bar (File/Edit/View/Board/Card/Window/Help) mapped to commands; an in-app menu button on Windows/Linux
    b) In-app menu everywhere

---

## H. Explorer & left panel

60. **Board ordering in the explorer.**
    a) ⭐ Pinned boards at the top, then the rest alphabetically; drag to reorder within the pinned section
    b) Fully manual order
    c) Virtual groups/folders of boards ("Work", "Personal"), stored in app data

61. **Hide a discovered board** (e.g. an old copy) without deleting it.
    a) ⭐ Yes: "Hide from explorer", with a "Show hidden" toggle
    b) No

62. **Explorer loading.**
    a) ⭐ Lazy: a board's children load when it's expanded; closed boards show only their name and card count
    b) Load everything upfront

63. **Explorer shows Jira mirror boards.**
    a) ⭐ Yes, in the same tree with a ◆ badge
    b) In a separate section of the Jira panel only

---

## I. Search

64. **Query syntax** in the search box, in addition to the filter UI: `tag:x board:"Project" is:open is:jira status:done -tag:y "exact phrase" due:<2026-10-10`.
    a) ⭐ Yes; the filter UI and the text query stay in sync
    b) Filter UI only

65. **Saved searches** (listed in the Search panel, openable as a tab showing results as cards).
    a) Yes, as a list
    b) ⭐ Yes, and a saved search can be opened as a **virtual board** (read-only, grouped by board or lane)
    c) No

66. **Substring matching** (finding "login" inside "relogin"). A trigram index is larger (~3× the text size) but more forgiving.
    a) ⭐ Prefix + trigram
    b) Prefix only (a smaller index)

---

## J. Undo, history & activity summary

67. **Undo scope.**
    a) ⭐ **Per board.** `⌘Z` undoes the last change in the board you are looking at, including changes made through the explorer or the palette.
    b) One global app-wide stack
    c) Per tab (as the spec says today)

68. **Persist undo across restarts.**
    a) ⭐ No; History/Restore covers it
    b) Yes

69. **Activity summary export formats.**
    a) ⭐ Copy as Markdown · plain text · Slack-formatted
    b) Markdown only

70. **Scheduled summary** (e.g. a daily 9:00 notification with "Yesterday you…").
    a) Yes, optional
    b) ⭐ No, on demand only

71. **Summary "Started" vs "Completed".** A card that went To do → Doing → Done in the same period:
    a) ⭐ Listed only under Completed
    b) Listed under both

---

## K. Jira

72. **Jira fields on cards.**
    a) ⭐ A standard set (status, type, priority, assignee, reporter, labels, sprint, epic/parent, due date, story points) plus **user-selectable custom fields per site** in Settings
    b) Standard set only

73. **Private local notes on mirror cards** (a notes area stored in `.luau/`, never sent to Jira).
    a) Yes
    b) ⭐ No; copy the card to a local board to annotate it

74. **Push local content to Jira** (an explicit "Push title/description to Jira" action on linked copies, behind the confirmation dialog).
    a) Yes
    b) ⭐ No; Jira content is only changed with Jira actions and field pickers

75. **Create a Jira issue from a local card** ("Create Jira issue from card…": pick project and type, then the card becomes Jira-linked).
    a) ⭐ Yes, behind the confirmation dialog
    b) No

76. **Jira comments.**
    a) ⭐ Read the thread in the editor's Jira section, plus add comments (confirmed)
    b) Add only
    c) Neither

77. **Images and attachments in Jira descriptions** (these are auth-protected URLs).
    a) ⭐ Downloaded once with the account's token and stored locally with the card (linked copies) or in the mirror board folder
    b) Shown as links only

78. **Sync cadence.**
    a) ⭐ Every 2 min while the window is focused, every 10 min when unfocused, paused while the machine sleeps, plus an immediate sync on focus
    b) Fixed interval: ___

79. **The same Jira board mirrored twice** (in two folders).
    a) ⭐ Allowed, with a warning
    b) Blocked

80. **Linked copies when the Jira issue changes status** (e.g. it's marked Done in Jira).
    a) ⭐ Only the Jira strip updates; the local card never moves automatically
    b) Offer an optional per-board rule: "move linked cards to the lane whose name matches the Jira status category"

81. **Jira issues dropped into a files board.**
    a) ⭐ They become Jira-linked documents, with the Jira header in the full-page editor
    b) Not allowed

82. **Self-hosted Jira over plain HTTP** (no TLS, e.g. an internal network).
    a) ⭐ Blocked (HTTPS required; custom CA certificates supported)
    b) Allowed per site behind an explicit "insecure" toggle with a warning

83. **Client certificates (mTLS) for self-hosted Jira.**
    a) Needed in v1
    b) ⭐ Later

84. **Jira panel "My open issues" definition.**
    a) ⭐ `assignee = currentUser() AND statusCategory != Done ORDER BY updated DESC`, editable in settings
    b) Other JQL: ___

---

## L. Import / export

85. **Adopt an existing Markdown folder** (e.g. an Obsidian vault). Our format requires ID filenames.
    a) ⭐ Import as a **copy** into a new board (originals untouched; files renamed to IDs, images relinked, `[[Title]]` wiki links converted to ID links)
    b) Convert in place (renames the user's files; risky)

86. **Extra export formats** (multi-select):
    a) ⭐ HTML (a single self-contained file)
    b) ⭐ PDF through the system print dialog
    c) CSV (one row per card)
    d) None beyond the spec

---

## M. Settings & shortcuts

87. **Settings portability.**
    a) ⭐ "Export / Import settings" (settings + keybindings + templates as one JSON)
    b) Not needed

88. **Single-key board shortcuts** (Trello/Linear style, active only while the board has focus and no text field is focused): `n` new card, `e` edit, `t` add tag, `d` set due date, `a` archive, `/` filter, `?` shortcut cheat sheet.
    a) ⭐ Yes, on by default (remappable, can be disabled)
    b) No, modifier shortcuts only

89. **Keybinding presets.**
    a) ⭐ One default set (VS Code-flavoured) + the full keybinding editor
    b) Also ship Trello-like and Vim-like presets

---

## N. Visual design

90. 🔴 **Visual direction.**
    a) ⭐ Calm and minimal (Linear / Things 3): neutral greys, one accent colour, subtle depth, dense but airy
    b) Warm and playful (Notion / Trello): coloured lanes, rounded cards, emoji-friendly
    c) Native-first: mimic each OS's native look
    d) Reference app(s) you love: ___

91. **Fonts.**
    a) ⭐ Bundled **Inter** (UI) + **JetBrains Mono** (code), ~400 KB total, for an identical look on every OS
    b) System fonts only (0 KB, but a different look per OS)

92. **Window translucency** (macOS vibrancy sidebar, Windows 11 Mica).
    a) ⭐ Yes where supported, with a solid fallback, and a setting to turn it off
    b) Solid everywhere

93. **Default accent colour.**
    a) ⭐ Indigo
    b) Blue
    c) Green
    d) Other: ___

94. **Motion.**
    a) ⭐ Spring-based, short (≤ 200 ms) motion for drag, drop, collapse and modals; respects "reduce motion"
    b) Minimal: fades only

95. **App icon.**
    a) ⭐ I design a simple placeholder; you replace it later
    b) You'll provide one

---

## O. Anything missing?

96. **Features you expected that aren't in the spec** (recurring cards, reminders/notifications, card relations like "blocks", time tracking, a calendar view, a timeline view, …)? List any you want in v1: ___

97. **Hard constraints I haven't heard yet** (a deadline, a must-work-offline-on-a-specific-machine requirement, a board size you expect, e.g. "a board with 10,000 cards")? ___
