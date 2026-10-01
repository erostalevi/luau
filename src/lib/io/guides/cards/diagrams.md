# 🧭 Diagrams

Code blocks marked `mermaid` turn into diagrams that follow the light and dark theme. Click a diagram to edit its source.

## Flowchart

```mermaid
graph LR
  Idea[💡 Idea] --> Todo[To do]
  Todo --> Doing[Doing]
  Doing -->|review| Done[✅ Done]
  Doing -->|blocked| Todo
```

## Sequence

```mermaid
sequenceDiagram
  participant You
  participant Luau
  participant Disk
  You->>Luau: Edit a card
  Luau->>Disk: Save card.md
  Disk-->>Luau: Saved
  Luau-->>You: History updated
```

## Pie

```mermaid
pie title Where the week went
  "Building" : 55
  "Reviews" : 20
  "Meetings" : 25
```

More diagram types (Gantt, state, class, mind maps…) are listed at https://mermaid.js.org.

#guide #markdown
