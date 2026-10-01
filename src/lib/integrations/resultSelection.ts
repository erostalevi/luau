// Multi-selection of the integrations panel result list (pure; no DOM).
// Click = select one, ⌘/Ctrl-click = toggle, ⇧-click = range from the
// anchor (⌘⇧ adds the range), ⌘A = all. Keys are always kept in list order.

/** Most issues added in one drop / "Add selected to…". Mirrors `MAX_LINK_MANY` in Rust. */
export const MAX_LINK_MANY = 100;

export interface ListSelection {
  keys: string[];
  anchor: string | null;
}

export const emptySelection = (): ListSelection => ({ keys: [], anchor: null });

/** `keys` sorted by their position in `order` (unknown keys dropped). */
export function inOrder(order: string[], keys: Iterable<string>): string[] {
  const set = new Set(keys);
  return order.filter((k) => set.has(k));
}

export function clickSelect(order: string[], sel: ListSelection, key: string, mods: { toggle?: boolean; range?: boolean } = {}): ListSelection {
  if (!order.includes(key)) return sel;
  if (mods.range && sel.anchor && order.includes(sel.anchor)) {
    const a = order.indexOf(sel.anchor);
    const b = order.indexOf(key);
    const span = order.slice(Math.min(a, b), Math.max(a, b) + 1);
    return { keys: inOrder(order, mods.toggle ? [...sel.keys, ...span] : span), anchor: sel.anchor };
  }
  if (mods.toggle) {
    const has = sel.keys.includes(key);
    return { keys: inOrder(order, has ? sel.keys.filter((k) => k !== key) : [...sel.keys, key]), anchor: key };
  }
  return { keys: [key], anchor: key };
}

export function selectAll(order: string[]): ListSelection {
  return { keys: [...order], anchor: order[0] ?? null };
}

/** Keep only keys still present (after a new search / load more). */
export function prune(order: string[], sel: ListSelection): ListSelection {
  const keys = inOrder(order, sel.keys);
  return { keys, anchor: sel.anchor && order.includes(sel.anchor) ? sel.anchor : (keys[0] ?? null) };
}

/** What a drag starting on `key` carries: the selection (list order) when
 *  `key` is part of it, otherwise just `key`. */
export function dragKeys(order: string[], sel: ListSelection, key: string): string[] {
  return sel.keys.includes(key) ? inOrder(order, sel.keys) : [key];
}

/** Cap to `max` keys, telling whether anything was dropped. */
export function capKeys(keys: string[], max = MAX_LINK_MANY): { keys: string[]; capped: boolean } {
  return keys.length > max ? { keys: keys.slice(0, max), capped: true } : { keys, capped: false };
}
