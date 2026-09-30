// Board-relative paths and `lull://` URLs for attachments.

import { isTauri } from '$lib/backend/rpc';
import type { BoardModel } from '$lib/state/boards.svelte';
import type { Parent } from '$lib/backend/types';

let convert: ((path: string, protocol: string) => string) | null = null;
if (isTauri) {
  void import('@tauri-apps/api/core').then((m) => (convert = m.convertFileSrc));
}

export function containerRel(b: BoardModel, p: Parent): string {
  if (p.kind === 'root') return '';
  if (p.kind === 'lane') return p.id;
  const n = b.node(p.id);
  if (!n) return p.id;
  const up = containerRel(b, n.parent);
  return up ? `${up}/${p.id}` : p.id;
}

/** Folder holding a card's attachments (relative to the board root). */
export function attachmentDirRel(b: BoardModel, cardId: string): string {
  const n = b.node(cardId);
  if (!n) return '';
  const base = containerRel(b, n.parent);
  return n.isGroup ? (base ? `${base}/${cardId}` : cardId) : base;
}

export function fileUrl(boardId: string, rel: string, width?: number): string {
  const path = `${boardId}/${rel.split('/').map(encodeURIComponent).join('/')}`;
  const base = convert ? convert(path, 'lull') : `lull://localhost/${path}`;
  return width ? `${base}?w=${Math.round(width * (window.devicePixelRatio || 1))}` : base;
}

/** URL for a file referenced from a card's markdown (relative to its folder). */
export function cardFileUrl(b: BoardModel, cardId: string, ref: string, width?: number): string {
  if (/^(https?:|data:|blob:)/i.test(ref)) return ref;
  const dir = attachmentDirRel(b, cardId);
  const clean = decodeURIComponent(ref.replace(/^\.\//, ''));
  return fileUrl(b.id, dir ? `${dir}/${clean}` : clean, width);
}
