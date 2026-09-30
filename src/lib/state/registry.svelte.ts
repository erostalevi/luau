// Known boards (from discovery) for the explorer, start page and pickers.

import { rpc, onCoreEvent } from '$lib/backend/rpc';
import type { BoardEntry, Registry } from '$lib/backend/types';

export const registry = $state<{ data: Registry; scanning: boolean; indexed: { done: number; total: number } }>({
  data: { boards: [], order: [], mirrorOrder: [] },
  scanning: false,
  indexed: { done: 0, total: 0 },
});

export async function loadRegistry() {
  registry.data = await rpc<Registry>('registry.get');
  onCoreEvent((e) => {
    if (e.type === 'registryChanged') registry.data = e.registry;
    if (e.type === 'progress') {
      if (e.task === 'discovery') registry.scanning = true;
      if (e.task === 'index') {
        registry.indexed = { done: e.done, total: e.total };
        registry.scanning = e.done < e.total;
      }
    }
  });
}

export function boardEntry(id: string): BoardEntry | undefined {
  return registry.data.boards.find((b) => b.id === id);
}

/** Explorer ordering: pinned first, then manual order, then alphabetical. */
export function sortedBoards(section: 'boards' | 'mirrors', showHidden = false): BoardEntry[] {
  const inSection = (b: BoardEntry) => (b.section ?? (b.mirror ? 'mirrors' : 'boards')) === section;
  const list = registry.data.boards.filter((b) => inSection(b) && (showHidden || !b.hidden));
  const order = section === 'boards' ? registry.data.order : registry.data.mirrorOrder;
  const pos = (id: string) => {
    const i = order.indexOf(id);
    return i < 0 ? Number.MAX_SAFE_INTEGER : i;
  };
  return list.sort((a, b) => {
    if (a.pinned !== b.pinned) return a.pinned ? -1 : 1;
    const pa = pos(a.id);
    const pb = pos(b.id);
    if (pa !== pb) return pa - pb;
    return a.name.localeCompare(b.name, undefined, { sensitivity: 'base' });
  });
}

export async function updateRegistry(patch: Record<string, unknown>) {
  registry.data = await rpc<Registry>('registry.update', patch);
}
