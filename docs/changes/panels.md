# Left-panel Explorer, Search panel, saved-search virtual board, files-board home

SPEC §7.2, §7.4, §8.1, §8.2, §8.6.

## Objective

Replace the stubs of the left-panel **Explorer** and **Search** sections, the
**saved-search virtual board** tab and the **files-board home** with complete,
keyboard-navigable UIs, plus their commands and strings (en/es/pt).

## Files

New
- `src/lib/panels/panels.commands.ts` — `explorer.revealActive`, `explorer.collapseAll`,
  `explorer.toggleHidden`, `explorer.focus`, `search.saveCurrent`, `search.clear`,
  `search.toggleCase`, `search.toggleFilters`, `search.openSaved`, `files.newDocument`,
  `files.focusFilter` (hidden, `when: boardKind == files`).
- `src/lib/panels/explorer/explorerActions.ts` — open / open to side (new pane), reveal,
  inline rename, new card/lane/doc, archive, trash, pin, hide, relocate / remove missing
  boards, board drag source + external drop (pinned reorder), context menus.
- `src/lib/panels/explorer/nav.ts` (+ `nav.test.ts`) — pure keyboard nav (step, edges,
  left/right, type-ahead) and pinned reorder.
- `src/lib/panels/search/searchActions.ts` — focus/clear/case toggle, saved searches
  (save/rename/delete with undo/open as board/pick), hit open + context menu.
- `src/lib/panels/search/filterMenus.ts` — filter menus (board, lane, tag, type, has,
  priority, person, due, modified) with date presets and custom ranges.
- `src/lib/panels/search/sync.ts` (+ `sync.test.ts`) — two-way sync text ↔ filter UI,
  keeps an explicit `case:` token.
- `src/lib/files/filesHome.ts` (+ `filesHome.test.ts`), `filesActions.ts`,
  `filesState.svelte.ts` — files-home model, new/rename document, doc context menu.
- `src/lib/i18n/parts/panels.{en,es,pt}.ts`.

Replaced stubs
- `src/lib/panels/explorer/Explorer.svelte`, `src/lib/panels/search/SearchPanel.svelte`,
  `src/lib/views/VirtualBoard.svelte`, `src/lib/files/FilesHome.svelte`.

Small additions to committed helpers
- `explorerState.svelte.ts`: `focusTick`.
- `searchState.svelte.ts`: `GroupBy` (incl. `tag`), `groupBy`, `focusSeen`, `updateSaved`;
  backend query is serialized without `@` sugar.
- `query.ts`: `@person` ⇄ `mention:person` sugar (`toText(q, { sugar: false })` for backend).
- `results.ts`: `groupByTag`, `navOrder`.

No shared files touched (no RPC, mock, App, dnd or defaults changes).

## Logic

- **Explorer**: `flatten()` turns registry + loaded board models into rows; sections
  Boards then Mirrors; pinned first, then manual order / alphabetical. Boards load lazily
  when expanded (`openBoard`). Expanded keys persist in `ui.json` (`explorerExpanded`,
  capped at 4000). Rows are fixed-height and virtualized (only the visible window +
  overscan is rendered). Rows carry `data-tree` so the shared pointer DnD engine drops
  cards onto boards/lanes/cards (including across boards); card rows are drag sources;
  board rows reorder pinned boards. Missing boards are dimmed with Locate…/Remove.
  Keyboard: ↑/↓, Home/End, ←/→, Enter, ⌥Enter (side), F2, type-ahead, context-menu key.
- **Search**: the text is the source of truth; filter chips/menus edit the parsed query
  and write it back with `rewrite()`. Case toggle edits the `case:` token relative to the
  `search.caseSensitive` default. Results grouped by board or lane with highlighted
  snippets; ↑/↓/Enter navigation from the box. Saved searches persist in `ui.json`.
- **Virtual board** (`savedSearch` tab): runs the query, lanes are groups by board / lane /
  tag; cards are real cards (open, drag, context menu); refreshes on board changes.
- **Files home**: list or grid of all documents (tree order → sorted by recent or A–Z),
  filter on title/ancestor path/#tags (accent-insensitive), archived toggle, keyboard
  navigation (arrows, Home/End, Enter, context-menu key), "New document" button. Layout
  and sort persist in `ui.json` (`filesHome`).

```
search box text ──parse──▶ Query ──filter UI edits──▶ rewrite() ──▶ text
      │                                                     ▲
      └──effectiveText(default case)──▶ prepare() ──▶ rpc search.query
```

## Decisions

- `search.focus` already exists in `commands/builtin/workspace.ts`; it was not redefined
  (same id would override). `explorer.toggleHidden` reuses the same setting as the
  existing `app.toggleHiddenBoards`.
- Document titles are validated (≤ 200 chars, newlines collapsed) before creating the file.
- Pinned-board reorder only within pinned boards (manual order for the rest is via the
  existing board commands).

## Risks / assumptions

- The browser mock's `registry.update` ignores patches, so pin/hide/reorder are not
  visible in `pnpm dev:web` (they are with the real core).
- Grid keyboard navigation estimates columns from the first item's width.
- Visual check in the browser was not completed (shared Browser pane was in use by another
  session); verify manually with `pnpm dev:web`.

## Tests

- `npx vitest run` — 10 files / 93 tests pass (new: `nav.test.ts`, `sync.test.ts`,
  `filesHome.test.ts`).
- `pnpm check` — 0 errors, 0 warnings.
- `pnpm i18n en|es|pt` — all keys present, full parity.
- Recommended: E2E for drag from the tree onto another board and for saved-search tabs
  surviving restart.
