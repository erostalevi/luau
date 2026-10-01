# Scroll retention across tab switches

## Objective

Switching tabs remounts the view (`PaneView` renders the active tab inside `{#key tab.id}`), so every
scroller started again at the top/left. Tabs now keep their scroll position, and so does every
scroller inside them: the board (horizontal across lanes, vertical when lanes are rows), each lane's
own card list, the full-page document editor, files-board home, start page, settings (list and
category index), keybindings table, saved-search results and summary views. Two left-panel lists
(search results, history timeline) keep theirs when you switch panel sections. Positions also
survive an app restart through the existing UI-state persistence.

## Files

New
- `src/lib/state/scrollMemory.ts`: pure store (no DOM, no runes). Keys, clamping, tolerance,
  last-used ordering with a cap, lazy load of the persisted snapshot, throttled trailing save,
  `forget(scope)`.
- `src/lib/components/keepScroll.ts`: the `use:keepScroll={key}` Svelte action
  (`attachKeepScroll` does the work), plus the app singleton wired to `uiGet/uiSet('scrollPositions')`
  and `forgetScroll(tabId)`.
- `src/lib/state/scrollMemory.test.ts`: unit tests for the store and the action.

Wiring (one attribute or action per file)
- `src/lib/shell/PaneView.svelte`: wraps the tab view in `<div class="scroll-scope" data-scroll-scope={tab.id}>`
  (`display: contents`, so layout does not change). With no tab, the start page uses `pane:<paneId>`.
- `src/lib/shell/LeftPanel.svelte`: `data-scroll-scope="left"` on the panel content.
- `src/lib/state/workspace.svelte.ts`: `closeTab` forgets the tab's positions.
- Scrollers: `board/BoardView.svelte` (`board`), `board/Lane.svelte` (`lane:<id>`),
  `editor/CardEditor.svelte` (`doc:<cardId>`, page variant only), `files/FilesHome.svelte` (`files`),
  `views/StartPage.svelte` (`start`), `views/settings/SettingsView.svelte` (`toc`, `settings`),
  `views/settings/KeybindingsView.svelte` (`keybindings`), `views/VirtualBoard.svelte` (`results`),
  `views/SummaryView.svelte` (`summary`), `panels/search/SearchPanel.svelte` (`search`),
  `panels/history/Timeline.svelte` (`history`).
- `views/VirtualBoard.svelte`: the "scroll back to the left" effect now runs only when the grouping
  changes, not on mount, so it no longer cancels the restore.

## Logic

```
mount scroller ─► scope = closest [data-scroll-scope] (captured once) ─► key = "<scope>|<local>"
   │
   ├─ remembered position? ─ no ─► just record scroll events
   │
   └─ yes ─► restoring:
         apply(): clamp target to current max scroll, write scrollLeft/Top, remember what we wrote
         re-apply on ResizeObserver (scroller + children, and children added later)
         stop when: target reached │ user intent (wheel/touch/pointer/key) │
                    a scroll we did not write (keyboard nav, find, scrollIntoView, DnD auto-scroll) │
                    timeout 2.5 s (memory is then clamped to what the content allows)
scroll event (not restoring) ─► memory.set(key, pos) ─► throttled (1 s, trailing) uiSet('scrollPositions')
```

- Restoring runs in the action's mount effect, before the browser paints, so views whose content is
  already in memory (boards stay loaded across tab switches) come back with no visible jump. Content that
  arrives later (async board load after restart, CodeMirror reading the file, lazy views, images) is
  handled by the ResizeObserver retries.
- Our own writes echo back as `scroll` events. They are recognised by comparing with the last position
  we wrote (1.5 px tolerance for zoom and sub-pixel rounding), so they never overwrite the target while
  the content is still short.
- Persisted shape: `{ "<tabId>|<key>": [x, y] }`. Zero positions are left out, at most 400 scrollers are
  kept (least recently scrolled dropped first), and a closed tab's entries are removed.

## Decisions

- **Scope from the DOM, not Svelte context.** Actions run in effects, where `getContext` is not allowed.
  Reading `[data-scroll-scope]` keeps each view unaware of tab ids, and changes to `BoardView`/`Lane`
  stay at one action per scroller. The key is captured at mount, so late events from the outgoing view
  cannot land on the incoming tab.
- **Per tab id.** Tab ids are unique across split panes and stay the same when a tab moves between
  panes (`moveTab`), so the position follows the tab. A board can only be open in one tab at a time;
  if that changes, positions are still separate per tab.
- **Document editor.** In the page variant the CodeMirror `.cm-scroller` is `overflow: visible` and the
  surrounding `.body` scrolls (so backlinks and sub-documents can sit under the text), so the action goes
  on `.body`. The modal and sidebar editors are not tabs and pass `null` (no-op).
- **No save on destroy.** By the time an action is torn down its node is already detached, and reading
  scroll offsets then returns 0. Positions are recorded on every scroll event instead, which is cheap
  because only an in-memory Map is touched and persistence is throttled.
- **Explorer not covered.** It is virtualised with its own `scrollTop` state and does not remount on tab
  switches.

## Risks / assumptions

- If content becomes shorter than the remembered position (cards moved away, document edited
  elsewhere), the scroller is clamped to its new maximum. After the timeout the memory is clamped too.
- A restore that is still waiting for content is cancelled by any scroll it did not cause. That is on
  purpose (it never fights the user), but browser scroll anchoring during a long async load can also end
  the restore early. The position is then close but not exact.
- Programmatic smooth scrolls (keyboard navigation, find in board, palette "go to lane") take priority:
  they end any restore still running.
- Natively (WKWebView/WebView2/WebKitGTK) the same DOM APIs are used. This was not verified in the
  desktop build.

## Tests

- `pnpm test`: `src/lib/state/scrollMemory.test.ts` (13 tests) covers key scoping, clamping and
  tolerance, rounding and invalid values, the throttled trailing save without zero entries, lazy
  snapshot load that does not overwrite newer positions, `forget` matching the scope prefix exactly
  (`t1` vs `t10`), the last-used cap, and the action with a faked jsdom layout (record and restore,
  no-op without a scope or key, our own echo ignored while content is short, clamping on timeout, and
  ending on user intent or a scroll from somewhere else).
- Manual, `pnpm dev:web` with the browser pane at 1100×520:
  - Product Roadmap: board scrolled right to 524 px, Backlog lane scrolled down to 163 px.
  - Second board: scrolled right to 210 px.
  - Notes document: lengthened, then scrolled to 271 px.
  - Switching between Start, both boards, the document and Settings (900 px) restored every position,
    each time into a freshly mounted element.
  - After a page reload (an app restart in the mock), Product Roadmap came back at 524 / 163 once the
    board had loaded asynchronously.
- Recommended native check: switch tabs with a long document that has images, a board in rows
  orientation, and split panes. Use keyboard navigation (arrow keys) right after a switch.
