import type { Component } from 'svelte';
import { FilePlus2, ArrowRightLeft, Pencil, Trash2, RotateCcw, Flame, Archive, Columns3, FolderSync, Plug, Circle, Undo2 } from '@lucide/svelte';
import type { KindGroup } from './model';

const BY_GROUP: Record<KindGroup, Component<any>> = {
  created: FilePlus2,
  moved: ArrowRightLeft,
  edited: Pencil,
  deleted: Trash2,
  archived: Archive,
  lanes: Columns3,
  external: FolderSync,
  integrations: Plug,
  other: Circle,
};

export function entryIcon(group: KindGroup, kind: string): Component<any> {
  if (kind === 'restore') return RotateCcw;
  if (kind === 'purge') return Flame;
  if (kind === 'undo' || kind === 'redo') return Undo2;
  return BY_GROUP[group] ?? Circle;
}
