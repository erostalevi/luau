import type { Component } from 'svelte';
import {
  Settings2,
  Palette,
  LayoutGrid,
  PenLine,
  Tag,
  Files,
  FolderSearch,
  Search,
  History,
  Sparkles,
  Plug,
  Keyboard,
  Upload,
  Wrench,
  Puzzle,
} from '@lucide/svelte';

const ICONS: Record<string, Component<any>> = {
  general: Settings2,
  appearance: Palette,
  board: LayoutGrid,
  editor: PenLine,
  tags: Tag,
  files: Files,
  discovery: FolderSearch,
  search: Search,
  history: History,
  ai: Sparkles,
  integrations: Plug,
  keyboard: Keyboard,
  export: Upload,
  advanced: Wrench,
};

export function categoryIcon(id: string): Component<any> {
  return ICONS[id] ?? Puzzle;
}
