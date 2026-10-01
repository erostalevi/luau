# 📅 Dates and properties

## Dates

A date in square brackets becomes a chip: [2026-10-08], with an optional time: [2026-10-08 14:30].

While you type, Luau turns natural words into dates. Try writing any of these, then keep typing:

- `[today]`, `[tomorrow]`, `[next friday]`
- `[in 3 days]`, `[in 2 weeks]`
- Spanish and Portuguese too: `[mañana]`, `[amanhã]`

Words inside code or link labels are never converted. `/today` inserts today's date.

## The properties footer

The end of a card can hold a few `key: value` lines after a `---` line. The board reads them for the card face, and you can edit them in the panel under the editor.

| Key         | Example                           | Shows as                      |
| ----------- | --------------------------------- | ----------------------------- |
| `priority`  | `urgent`, `high`, `medium`, `low` | a colored flag                |
| `due`       | `2026-10-08`                      | a due date (red when overdue) |
| `start`     | `2026-10-01`                      | a start date                  |
| `assignees` | `@ana, @sam`                      | avatars                       |
| `labels`    | `guide, docs`                     | labels                        |

This card has one. Look at its face on the board:

#guide #organize

---

priority: high
start: 2026-10-01
due: 2026-12-31
assignees: @ana, @sam
labels: guide
