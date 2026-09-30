// Resolve card ids to human titles across all boards (open or not).
// Open boards answer synchronously; others are fetched in batches from the
// search index and cached reactively.

import { SvelteMap } from 'svelte/reactivity';
import { rpc } from '$lib/backend/rpc';
import { boards } from '$lib/state/boards.svelte';
import { t } from '$lib/i18n/index.svelte';

interface Resolved {
  title: string;
  board: string;
  missing?: boolean;
}

const cache = new SvelteMap<string, Resolved>();
const queue = new Set<string>();
let timer: ReturnType<typeof setTimeout> | null = null;

function flush() {
  timer = null;
  const ids = [...queue];
  queue.clear();
  if (!ids.length) return;
  void rpc<{ id: string; board: string; title: string }[]>('search.titles', { ids }).then((rows) => {
    const found = new Set(rows.map((r) => r.id));
    for (const r of rows) cache.set(r.id, { title: r.title, board: r.board });
    for (const id of ids) if (!found.has(id)) cache.set(id, { title: '', board: '', missing: true });
  });
}

export function lookupCard(id: string): Resolved | null {
  for (const [bid, b] of boards) {
    const n = b.nodes.get(id);
    if (n) return { title: n.title, board: bid };
  }
  const c = cache.get(id);
  if (c) return c;
  if (!queue.has(id)) {
    queue.add(id);
    if (!timer) timer = setTimeout(flush, 30);
  }
  return null;
}

export function resolveTitle(id: string): string {
  const r = lookupCard(id);
  if (!r) return '…';
  if (r.missing) return t('links.missing');
  return r.title || t('common.untitled');
}

export function invalidateTitle(id: string) {
  cache.delete(id);
}
