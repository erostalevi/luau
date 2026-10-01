# Integrations: Jira Cloud + Data Center, Trello, Slack

## Objective

Implement SPEC §9 (all) and §8.4: connect Jira Cloud, Jira Server / Data
Center, Trello and Slack; search remote issues and drag them onto boards as
linked copies; mirror remote boards locally (read-only); pull and push through a
single WriteGate that always shows a change list ("This will update X and Y on
Z") and is gated by "allow push / allow pull" toggles.

## Files

Rust core (`crates/luau-core/src/integrations/`, feature folder):

| File | Role |
|---|---|
| `mod.rs` | module map, `start_watcher` re-export |
| `types.rs` | normalized provider model (§9.7): `RemoteIssue`, `RemoteBoard`, `RemoteColumn`, `RemoteUser`, `Transition`, `Capabilities`, … |
| `adf.rs`, `wiki.rs`, `md_util.rs` | ADF (Cloud) ↔ Markdown, wiki markup (DC) ↔ Markdown |
| `accounts.rs` | `config/integrations.json` (no secrets), URL normalization, per-account host allow-list |
| `secrets.rs` | OS keychain (`keyring`), memory-only fallback, redacted `Debug` |
| `http.rs` | reqwest/rustls client: allow-list on every request **and redirect**, auth headers only (never URLs), `429`/`Retry-After` + exponential backoff with jitter, optional CA PEM and mTLS client PEM, download size cap |
| `jira.rs` | Jira adapter (Cloud `/rest/api/3` + `/search/jql` `nextPageToken`; DC `/rest/api/2` + `/search` `startAt`; agile boards/columns/issues) and pure mappers |
| `trello.rs` | Trello adapter (key + token in `Authorization: OAuth …` header); lists act as statuses |
| `slack.rs` | `auth.test`, `conversations.list` (cursor pagination), `chat.postMessage` |
| `provider.rs` | the `IssueProvider` port as an enum (`Provider::Jira | Trello`) + capabilities |
| `links.rs` | `<board>/.luau/remote.json`: card ↔ issue links, mirror state, private notes path, card compose/split |
| `mirror.rs` | desired-state computation (lanes = board columns / statuses / Trello lists), plan (change rows), idempotent apply |
| `gate.rs` | WriteGate: `prepare` → single-use token (10 min TTL) → `take` on commit |
| `service.rs` | application layer: connect/test/remove/update accounts, search, linked copies, copy-from-mirror, refresh strips, mirrors CRUD + sync, prepare/commit, images, notes, watch loop |

Other files:

| File | Change |
|---|---|
| `src-tauri/src/integrations_rpc.rs` | RPC dispatch for `integrations.*`, `remote.*`, `slack.*` (all async) |
| `src/lib/integrations/*` | `types.ts`, `state.svelte.ts`, `actions.ts`, `gate.ts`, `message.ts` (+ test), `RemoteStrip.svelte`, `RemoteHeader.svelte`, `integrations.commands.ts` |
| `src/lib/panels/integrations/IntegrationsPanel.svelte` | panel UI |
| `src/lib/backend/mocks/integrations.ts` | browser mock with a fake Jira project `LUAU` |
| `src/lib/i18n/parts/integrations.{en,es,pt}.ts` | strings |

### Shared files touched (surgical, for merging)

- `src-tauri/src/lib.rs`: one line `luau_core::integrations::start_watcher(core.clone());` in `setup`.
- `crates/luau-core/src/store/mod.rs` (`load_board`): a board whose manifest has a `mirror` key opens with `read_only = "mirror:<provider>"`.
- `crates/luau-core/src/app/mod.rs` (`register`): registry `mirror` flag = manifest has `mirror` (was always `false`).
- `src/lib/backend/types.ts`: `RemoteInfo.mirror?: boolean`.

No new crates (`keyring`, `reqwest`, `image`, `percent-encoding` were already
dependencies). The earlier draft's `p12-keystore` dependency was dropped: mTLS
takes a PEM file (cert + key); a `.p12` converts with
`openssl pkcs12 -in id.p12 -out id.pem -nodes`.

## Logic

```mermaid
flowchart LR
  UI[Command / panel / editor header] -->|remote.prepare| P[service::prepare]
  P -->|allowPush / allowPull?| G{gate}
  G -- off --> E[error push_disabled / pull_disabled + toast "Allow"]
  G -- on --> F[fetch remote, diff fields] --> T[gate::prepare → token + change rows]
  T --> D[Confirm dialog: “This will update X and Y on Z”]
  D -- Cancel --> C[remote.cancel]
  D -- Confirm --> K[remote.commit token] --> X[gate::take → Action] --> R[Provider / local write + journal]
```

- **Accounts**: `integrations.connect` validates the URL (HTTPS only; plain
  HTTP only for Data Center after an explicit danger confirmation; no
  credentials/query in URLs), builds the default allow-list (site host; Cloud
  also `api.media.atlassian.com` for attachments), **tests the connection**
  (`serverInfo` flavour check + `myself`), then stores the secret in the
  keychain. `config/integrations.json` never contains tokens or emails.
- **Default JQL**: `assignee = currentUser() AND statusCategory != Done ORDER BY updated DESC`, editable per account; saved queries per account.
- **Search/pagination**: Cloud `nextPageToken`/`isLast`; DC `startAt`+`total`;
  agile boards/issues `startAt`; Slack `next_cursor`; Trello page numbers.
  Bulk refresh uses `key in (…)` in chunks of 100.
- **Linked copies** (drag from the panel, or out of a mirror): a normal card
  `# Summary\n\n<description md>\n\nJira: [KEY](url)` + a link in
  `.luau/remote.json`. Only title + content are mirrored; local moves never
  touch the service.
- **Mirrors**: stored in `<app data>/mirrors/<boardId>/` as normal boards with
  `manifest.mirror`, opened read-only. Lanes = Jira board columns (status ids
  mapped by the board configuration), statuses (JQL mirrors), or Trello lists.
  Cards ordered by rank. The sync applies ops under the store lock with the
  read-only flag lifted (user ops stay refused), journals one `remoteSync`
  entry (`Origin::Remote`), is idempotent (key → card, column → lane and
  content hashes). Private local notes live in `.luau/notes/<card>.md`.
- **Pull / push**: pull on a linked card diffs Title/Description local vs
  remote (skipped when the remote did not change since the last pull — hash in
  the link); pull on a mirror shows the plan rows (columns, new/removed issues,
  status moves). Push sends Summary/Description of a linked copy (mirrors are
  never pushed); local image refs are mapped back to `jira-attachment:` names.
- **Watch**: a background thread (own tokio runtime) every
  `integrations.watchIntervalSec` (default 60 s): pulls watched mirrors and
  refreshes the strip of linked cards on open boards (only when pull is allowed).
- **Other writes** (all gated): create issue from card (project + type, or
  Trello board + list), comment, transition (with "Revert" toast), assign,
  Slack post.
- **Images**: attachments referenced in the description are downloaded with
  the account credentials (≤ 20 per card, ≤ 20 MB each), optimized with
  `files::optimize_image` (≤ 2400 px, JPEG/PNG) and stored as card attachments.
- **UI**: card-face strip (status, type, priority, assignee), editor header
  (status → transitions menu, assignee, pull/push/comment, comment thread,
  mirror notes), card actions, `cardIsRemote` context key, DnD handlers
  `remoteIssue` and `copyFromMirror`.

### Commands

`integrations.connectJira`, `integrations.connectTrello`,
`integrations.connectSlack`, `integrations.mirrorBoard`, `remote.pull`,
`remote.push`, `remote.createIssue`, `remote.openInBrowser`, `remote.comment`,
`remote.transition`, `remote.assign`, `remote.unlink`, `remote.refresh`,
`remote.actions` (hidden), `remote.toggleAllowPush`, `remote.toggleAllowPull`,
`slack.post`.

### RPC methods

`integrations.accounts|connect|test|remove|update|capabilities|mirrors|tick`,
`remote.search|issue|transitions|comments|users|projects|issueTypes|boards`,
`remote.links|link|copyFromMirror|refresh|unlink|notes.get|notes.set`,
`remote.mirror.create|sync|watch|remove`, `remote.prepare|commit|cancel`,
`slack.channels`.

### Settings

`integrations.allowPush` (default **false**) and `integrations.allowPull`
(default true) registered by the module; existing `integrations.confirmPush`
(dialog for pushes, default on) and `integrations.confirmPull` (dialog for
pulls, default off) decide whether the dialog is shown. Rust enforces the
allow toggles on prepare **and** commit.

## Technical decisions

- WriteGate tokens live only in memory and are single-use; the commit executes
  exactly what was previewed (captured action), so the UI cannot alter a write
  after confirmation.
- Provider port as an enum (no `async_trait` dependency); adding Linear/GitHub
  is one variant + one adapter file.
- Trello credentials go in the `Authorization` header instead of the usual
  `?key=&token=` query string, so they never appear in URLs or logs.
- Logs contain method, path (no query), HTTP status and issue keys only.
  `RemoteUser.email` is dropped when mapping Jira users.
- Mirror ids are allocated before taking the store lock (id allocation checks
  every open board; taking it under the lock would deadlock).

## Risks / assumptions / pending

- **Slack sender for AI**: `luau_core::ai::set_slack_sender` does not exist on
  this branch; TODO: once the AI module lands, register a sender that calls
  `service::prepare(PrepareReq::SlackPost …)` + commit (it must stay gated).
- Mirrors do not nest sub-tasks as groups and do not implement the Sprint /
  Backlog views of §9.4 yet (all board issues are shown in their columns).
- Board column configuration is fetched on every sync (no 10-minute cache).
- Jira Server legacy username+password auth is not exposed in the UI (PAT only).
- mTLS accepts PEM only; no CA bundle import UI yet (`caCertPath` is supported
  by the backend).
- Card attachments downloaded for linked copies are not re-mapped if the user
  renames them.
- The webview never talks to the services directly; remote images inside
  comments are not rendered (CSP).
- The discovery crawler calls `registry.upsert(..., mirror = false)`; mirrors
  live outside the default search roots, so this is not expected to flip.

## Tests

Rust (`cargo test -p luau-core integrations`, 56 tests):
- ADF ↔ Markdown and wiki ↔ Markdown round trips (previous draft, kept).
- Mapping: Jira Cloud/Server issues (no emails kept), Trello cards.
- Pagination: offset/`nextPageToken`, Slack cursor.
- Host allow-list (look-alike hosts, HTTP opt-in only to the site host,
  credentials in URLs), URL normalization.
- Backoff (`Retry-After`, cap, jitter), auth headers per provider.
- WriteGate: field diff, single-use tokens, empty plans, cancel.
- Mirror: lanes by columns/statuses, plan + confirmation rows, idempotent
  apply on a real board (second sync writes nothing, user edits refused,
  removed issues trashed, notes, remove mirror).
- Accounts update keeps own host, saved queries, no token in config.

TypeScript (`npx vitest run`): `message.test.ts` — confirmation sentence in
en/es, pull wording, mirror rows collapsed, field labels.

Also run: `pnpm check` (0/0), `pnpm i18n en|es|pt`, `cargo clippy --workspace
--all-targets -D warnings`, `cargo fmt --all`, `cargo test --workspace`.
Manual check in `pnpm dev:web` (mock): drag an issue onto a board → linked
card with strip; transition from the editor header → push-disabled toast →
"Allow" → confirmation dialog → status updated + "Revert" toast.

Recommended (not automated): an HTTP-level integration test with a local mock
server (e.g. `wiremock`) for 429 retries and redirect refusal; manual tests
against a real Jira Cloud and a Data Center instance with mTLS.
