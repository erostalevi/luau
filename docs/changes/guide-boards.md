# Guide boards (kanban and notes templates)

## Objective

Give new users two ready-made boards that teach the app by example: every Markdown feature the editor
renders (headings, emphasis, lists and tasks, tables, card links and embeds, code and Python cells, math,
Mermaid, callouts, images, PDFs and other files, tags, mentions, dates, the properties footer), plus board,
search and keyboard basics. Integrations (Jira, Trello, Slack) are left out on purpose.

## What the user sees

*New board from template* (command palette) lists two new entries:

| Template | Kind | Content |
|---|---|---|
| **Luau guide (kanban)** | kanban | 5 lanes (Start here, Writing, Rich blocks, Organizing, Done), 19 cards, including a drag-and-drop exercise that makes a group card |
| **Luau guide (notes)** | files | 16 numbered documents (1 · Welcome … 16 · Shortcuts) |

The welcome card links to every other card. The images card carries a sunset SVG, a PNG at three widths and a
one-page PDF quick reference, so attachments, thumbnails and resizing can be tried at once.

The guide text is English; the template names are localized (en/es/pt), and es/pt say that the content is in
English.

## Files

| File | Change |
|---|---|
| `crates/luau-core/src/io/templates.rs` | `BoardTemplate.assets` (`TemplateAsset { card, name, data }`), validation, `{{card:N}}` / `{{asset:NAME}}` placeholders, attachment writing after the batch |
| `crates/luau-core/src/app/files.rs` | `attachment_file_name()` extracted from `add_attachment` (same naming rule, reused by templates) |
| `crates/luau-core/src/app/tests.rs` | `template_links_cards_and_writes_assets` |
| `src/lib/io/guides/index.ts` | `guideSpec()`: assembles both guides from the Markdown cards and sample files |
| `src/lib/io/guides/cards/*.md` | the 21 guide cards (shared by both guides where they overlap) |
| `src/lib/io/guides/{sunset.svg,luau.png,quick-reference.pdf}` | sample attachments (the PNG is the app icon at 256 px; the PDF is hand-built, Helvetica only) |
| `src/lib/io/guides/guides.test.ts` | every link and asset placeholder resolves, assets decode |
| `src/lib/io/io.ts` | `TemplateAsset`, `TemplateSpec.assets`, `BASIC_BOARD_TEMPLATES` + guide ids in `NEW_BOARD_TEMPLATES` |
| `src/lib/io/io.commands.ts` | `specFor()`: guide content is loaded with a dynamic import |
| `src/lib/backend/mocks/io.ts` | browser mock fills placeholders; assets become data URLs |
| `src/lib/i18n/parts/io.{en,es,pt}.ts` | names and descriptions of the two guides |
| `src/lib/files/filesHome.ts` (+ test) | natural title sort (`numeric: true`) so "10 ·" comes after "9 ·" |
| `src/lib/editor/CardEditor.svelte` | **bug fix**: the editor no longer shrinks below its text inside the scrolling body (see below) |
| `src/lib/views/StartPage.svelte` | lint fix: `{' · '}` (rejected by `svelte/no-useless-mustaches`) → `<span class="sep"> · </span>` |
| `README.md`, `docs/SPEC.md` | templates list and template format |

## Logic and decisions

- **Content as Markdown files**, imported with `?raw` through `import.meta.glob`, not as i18n strings: the
  cards are long and full of backticks, and keeping them as `.md` makes them easy to review and edit.
- **Card links need ids that don't exist yet.** The core now picks all card ids before it creates the cards,
  then replaces `{{card:N}}` (N = position: lane cards in order, then notes) with the real id. The UI writes
  `{{card:key}}` in the `.md` files and numbers the keys per guide, so one card can be shared by both guides.
- **Attachments** use the same file naming as a normal upload (`<card>.<token>-<stem>.<ext>`, now one helper
  for both). Names are chosen before the cards are written, so the Markdown already points at them. The files
  are written after the batch, while the board is locked, with `atomic_write`. They are recorded with
  `touch()`, so the watcher treats them as Luau's own writes rather than outside changes.
- **Limits and validation** (the template comes from the UI over RPC): at most 20 assets, decoded size
  ≤ 2 MiB each, the card index must exist, and the name must not contain `/`, `\` or NUL. The name is also
  passed through `sanitize_name`, so it can't escape the card's folder.
- **Bundle size:** the guide chunk (55 kB, 34 kB gzip) is only loaded when a guide is created; the main chunk
  stays at 452 kB.
- **Undo:** creating the board is still one journaled batch. The attachment files sit outside that step, like
  any uploaded attachment.
- **Notes guide numbering:** the files-board home lists documents by recency or title. All guide documents are
  created in the same instant, so their titles are numbered to make A–Z (and recency ties) read in order.

## Bug found while testing: editor overlapped by the sections under it

In a files-board document with sub-documents, or in a card with linked or child sections, the editor's
`flex: 1; min-height: 0` let it shrink below its content inside the scrolling `.body`. The "Inside" /
"New sub-document" section was then drawn over the last lines (measured: editor 577 px, content 695 px).
Inside the card editor the editor is now `flex: 1 0 auto`: it still fills the space but never shrinks below its
text (after the fix: editor = content = 695 px, the section starts right below).

## Risks and assumptions

- Guide text is English only. Localized guides would mean a copy of the `.md` files per locale.
- In the **browser mock**, attachments are data URLs. The PDF card shows a broken thumbnail there, because the
  PDF preview is chosen by the `.pdf` extension. The real app uses file names and is not affected.
- Running the Python cell needs a local Python and the board trust prompt (by design).

## Tests

- `cargo test --workspace`: 203 passed, including `template_links_cards_and_writes_assets` (links resolved,
  unknown placeholders kept, file written with the right name, bad card index and `../` name rejected).
- `pnpm test`: 153 passed, including `guides.test.ts` and the natural-sort case.
- `pnpm lint`, `pnpm check`, i18n en/es/pt parity, `cargo clippy -D warnings`: clean.
- Manual check in the web build (Browser pane): both guides created. The board, card faces and links all
  render. In the editor: 3 Mermaid diagrams, 5 KaTeX formulas, 5 callouts, card links, a heading link and an
  embed. Images render at the set widths. Numbered documents sort in order, and the editor overlap is fixed.
- **Recommended in the desktop app:** create both guides, open the images card (real PDF thumbnail), click the
  PDF link (it opens in the default app), trust the board and run the Python cell, and do the drag-to-group
  exercise.
