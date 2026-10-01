# 💻 Code blocks

Inline code uses single backticks: `npm run dev`. Blocks use three backticks plus a language name for highlighting:

```ts
interface Card {
  id: string;
  title: string;
  tags: string[];
}

export const isDone = (c: Card) => c.tags.includes('done');
```

```rust
fn main() {
    let lanes = ["To do", "Doing", "Done"];
    for (i, lane) in lanes.iter().enumerate() {
        println!("{i}: {lane}");
    }
}
```

```sql
SELECT title, due
FROM cards
WHERE tag = 'guide'
ORDER BY due;
```

```bash
git add . && git commit -m "Update my board"
```

Hover a block to copy it. Text pasted inside a code block always stays plain.

## Python cells

Blocks marked `python` get a **Run** button. The output is shown under the block:

```python
lanes = {"To do": 4, "Doing": 2, "Done": 7}
total = sum(lanes.values())
for name, n in lanes.items():
    print(f"{name:<6} {'█' * n} {n / total:.0%}")
```

> [!warning] Code runs on your computer
> Running code needs Python installed. The first time, Luau asks whether you **trust this board**, because a cell runs with your user's permissions. Only trust boards you wrote yourself.

#guide #code
