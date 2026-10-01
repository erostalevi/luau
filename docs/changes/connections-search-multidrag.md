# Connections panel: JQL / plain-text search and multi-select drag

## Objective

1. Give the connections (integrations) search bar a **JQL / Text** toggle.
   Jira supports both; Trello only plain text (toggle shown disabled with a
   tooltip). Plain text is turned into a safe JQL **in core** — user input is
   never concatenated raw into JQL. Invalid JQL shows the server's message,
   sanitized.
2. **Multi-select** results (click, ⌘/Ctrl-click, ⇧-click range, ⌘A, Esc,
   hover checkboxes, Space on a focused row) and **drag all selected at once**
   into a lane, onto a card, into the explorer tree or a files board. All
   cards are created in **one undo step**, in list order; the ghost shows the
   count; at most 100 per drop. Keyboard alternative: "Add selected to…".

## Files

| File | Change |
|---|---|
| `crates/luau-core/src/integrations/query.rs` (new) | Pure query resolution: `SearchMode`, `escape_text` (Lucene + JQL string escaping), `text_jql` (optional project scope), `resolve` per provider/mode, length caps; unit tests |
| `crates/luau-core/src/integrations/mod.rs` | registers `query` |
| `crates/luau-core/src/integrations/accounts.rs` | `SavedQuery.mode` (defaults to JQL for older configs) |
| `crates/luau-core/src/integrations/http.rs` | `server_message` (sanitized 400 body message), bounded body read (`read_capped`), `400` → `Invalid("remote_bad_request:<msg>")`; tests |
| `crates/luau-core/src/integrations/service.rs` | `search` (uses `query::resolve`), `link_many` + `plan_link_keys` + `create_linked_batch` (one `Op::Batch`), `fetch_ordered` (bulk fetch with per-key fallback); tests |
| `src-tauri/src/integrations_rpc.rs` | `remote.search` takes `mode` / `project`; new `remote.linkMany` |
| `src/lib/integrations/resultSelection.ts` (+ `.test.ts`) (new) | Pure list selection model (click / toggle / range / all / prune / drag keys / cap) |
| `src/lib/integrations/state.svelte.ts` | `integ.selection`, `isJiraAccount`, per-account search mode remembered via `uiGet`/`uiSet` |
| `src/lib/integrations/actions.ts` | mode-aware `search`, `setMode`, `runSaved`, `linkMany`, `addSelectedTo`; invalid-JQL message |
| `src/lib/integrations/integrations.commands.ts` | `remoteIssues` external drop handler, `integrations.addSelectedTo` command |
| `src/lib/integrations/types.ts` | `SearchMode`, `IssuesDragPayload`, `LinkManyResult` |
| `src/lib/panels/integrations/IntegrationsPanel.svelte` | toggle, selection UI (checkboxes, sticky selection bar), keyboard handling, multi-drag |
| `src/lib/board/dnd.svelte.ts` | **one generic line**: optional `DragSource.count` for the ghost badge of external payloads |
| `src/lib/backend/mocks/integrations.ts` | web-build mock: text mode, fake 400 for broken JQL, `remote.linkMany` as one batch |
| `src/lib/i18n/parts/integrations.{en,es,pt}.ts` | new strings |

## Logic

### Search modes

```
UI (query, mode) ──rpc remote.search──▶ service::search ──▶ query::resolve ──▶ Provider::search
                                                             │
                     Jira + JQL   → query as typed (≤ 4000 chars)
                     Jira + Text  → [project = "KEY" AND] text ~ "<escaped>" ORDER BY updated DESC
                     Trello       → cleaned text (one URL-encoded parameter, mode ignored)
                     empty query  → account default query
```

Plain-text escaping (`escape_text`), in two layers:

1. Clean: control characters and bidi overrides dropped, whitespace collapsed,
   max 500 characters.
2. Lucene: every special character `+ - & | ! ( ) { } [ ] ^ ~ * ? : / " \`
   gets a backslash; the case-sensitive operators `AND` / `OR` / `NOT` are
   lower-cased (plain words).
3. JQL string literal: `\` → `\\`, `"` → `\"`.

So `x" OR project = SECRET` becomes one literal search term; a test asserts
the resulting JQL has exactly two unescaped quotes. The optional project scope
only accepts a valid Jira key shape (`[A-Za-z][A-Za-z0-9_]*`).

The mode is remembered per account (`ui-state` key `integrations.searchMode`);
saved queries store their mode and restore it when run. "Mirror from query"
only pre-fills the panel query when it is JQL.

### Invalid JQL

On `400 Bad Request` the HTTP adapter reads at most 64 KiB of the body and
extracts `errorMessages` / `errors` (Jira) or `message` / short plain text
(Trello). HTML pages are ignored; control characters and `<` `>` are dropped
and the message is capped at 300 characters. It travels as
`Invalid("remote_bad_request:<msg>")`, is **never logged** (only the status is
logged, as before), and the panel shows "Invalid JQL: <msg>" (or "The search
was rejected: …" in text mode).

### Multi-select and drag many

- Selection is a pure model (`resultSelection.ts`): keys always kept in list
  order, range from an anchor, pruned on "Load more", cleared on a new search.
- A drag starting on a selected row carries the whole selection
  (`{ type: 'remoteIssues', keys }`, ghost "N issues" + badge); on an unselected
  row it stays the existing single-issue drag (`remoteIssue`, unchanged path).
- Drop → `actions.linkMany` → `remote.linkMany` → `service::link_many`:
  keys normalized (trim, dedupe, shape check, ≤ 100), keys already linked to a
  card that is **still on the board** are skipped (reported with "jump to
  existing"), issues fetched in one `key in (…)` (per-key fallback when Jira
  rejects the bulk query), then **one `Op::Batch` of `CreateCard`s** at
  consecutive indexes before the drop target. Image localization afterwards
  uses the same coalesce key, so it stays in the same undo step. Links are
  saved in one write.
- More than 100 selected: the first 100 (list order) are added and a toast
  says so; core also rejects > 100 (`too_many_issues:100`).
- Keyboard: ⌘A / Esc / Space in the list, ⌘Enter or the "Add to…" button /
  context menu / command palette ("Add selected search results to…") opens a
  board → lane (or "top level" for files boards) picker.

## Decisions

- Query building lives in core (`query.rs`, pure) so the UI can never send a
  hand-built JQL for plain text; the UI only sends `mode` + raw text.
- JQL mode intentionally sends the query as typed: the user is writing JQL
  against their own Jira with their own credentials; only the length is capped.
- Single click now selects; double-click opens in the browser (hint updated;
  "Open in browser" is still in the context menu).
- Undo leaves link entries in `remote.json` (so redo keeps the link); therefore
  `link_many` only treats links of cards still present as "already linked".
- The dnd engine change is a single optional field (`count`) with the old
  behaviour as the default.

## Risks / pending

- **Not verified against a real Jira or Trello**: Lucene escaping inside
  `text ~` and the shape of Jira 400 bodies follow Atlassian's documentation;
  some characters remain unsearchable in Jira even when escaped (Jira limitation).
- The project scope is supported by core/RPC (`project`) but the panel has no
  project picker yet, so it is not sent.
- The single-issue `remote.link` path still treats a link of an undone card as
  existing (pre-existing behaviour, unchanged here).
- Trello `issues()` fetches cards one by one (≤ 100 requests per drop).

## Tests

- Rust: `query::tests` (quoting, injection attempt, backslashes, Lucene
  operators, control chars, unicode, length cap, project validation, provider/mode
  resolution, serde), `http::tests::server_message_from_bad_request_bodies`,
  `service::tests::link_keys_are_deduped_validated_and_capped`,
  `service::tests::linked_batch_keeps_order_and_is_one_undo_step`.
- TS: `resultSelection.test.ts` (click / toggle / range / all / prune / drag keys / cap).
- Manual, web build of this worktree with the integrations mock (port 1451):
  toggled JQL ↔ Text (persisted across reload), invalid JQL shows the server
  message, Trello account shows the disabled toggle + tooltip, click + ⇧-click
  selected 3 results, dragged them into the Review lane (ghost "3 issues", 3
  cards in list order, one toast), one ⌘Z removed all 3, dragging again re-added
  them, ⌘A / Esc in the list, "Add to…" picker.
- Recommended: an integration test of `link_many` against a stub HTTP server,
  and a manual run against a real Jira Cloud / Data Center and Trello.
