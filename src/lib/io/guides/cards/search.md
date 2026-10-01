# 🔍 Search

**Find in board** (⌘F) dims every card that doesn't match. **Search** (⌘⇧F) looks through every board, with a small query language:

| Query                 | Finds                                      |
| --------------------- | ------------------------------------------ |
| `sunset`              | cards that contain the word                |
| `"exact phrase"`      | the phrase as written                      |
| `tag:guide`           | cards tagged #guide                        |
| `-tag:wip`            | cards _without_ the tag                    |
| `mention:ana`         | cards that mention or are assigned to @ana |
| `assignee:ana`        | cards assigned to @ana                     |
| `priority:high`       | cards by priority                          |
| `board:Guide`         | cards on one board                         |
| `is:open` / `is:done` | cards with open tasks, or all tasks done   |
| `due:overdue`         | cards past their due date                  |
| `in:title`            | match the title only                       |

Combine them: `tag:guide is:open -tag:wip`. The filters under the search box and the query stay in sync, and you can save a search and open it as a board.

## Remember everything

- **Undo** (⌘Z) works for every action, even moving 100 cards.
- **History** (⌘⇧H) shows every change with a word-by-word diff, and _Restore this version_ brings any version back.
- **Trash** keeps deleted cards for 7 days.

#guide #organize
