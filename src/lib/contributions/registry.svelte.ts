// Contribution points. The core registers its own features through these
// (e.g. the Jira integration adds a card-face strip, card actions and a panel),
// so the future extension API is exercised from day one.

import type { Component } from 'svelte';
import type { RemoteInfo } from '$lib/backend/types';

export interface CardCtx {
  boardId: string;
  id: string;
  remote?: RemoteInfo;
}

export interface CardAction {
  id: string;
  label: () => string;
  icon?: Component<any>;
  when?: (c: CardCtx) => boolean;
  run: (c: CardCtx) => unknown;
  source?: string;
}

export interface CardFaceProvider {
  id: string;
  /** Rendered in the card footer area. Receives `{ boardId, id, remote }`. */
  component: Component<any>;
  when: (c: CardCtx) => boolean;
  source?: string;
}

export interface EditorHeaderProvider {
  id: string;
  component: Component<any>;
  when: (c: CardCtx) => boolean;
  source?: string;
}

export interface ExtensionInfo {
  id: string;
  name: string;
  version: string;
  description?: string;
  publisher?: string;
  enabled: boolean;
}

export const contributions = $state({
  cardActions: [] as CardAction[],
  cardFace: [] as CardFaceProvider[],
  editorHeader: [] as EditorHeaderProvider[],
  extensions: [] as ExtensionInfo[],
});

export function contribute<K extends keyof typeof contributions>(point: K, item: (typeof contributions)[K][number]): () => void {
  (contributions[point] as unknown[]).push(item);
  return () => {
    const list = contributions[point] as unknown[];
    const i = list.indexOf(item);
    if (i >= 0) list.splice(i, 1);
  };
}
