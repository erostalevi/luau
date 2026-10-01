// Shared helpers for commands: active board/card resolution, native dialogs.

import { isTauri, rpc } from '$lib/backend/rpc';
import { boards, type BoardModel } from '$lib/state/boards.svelte';
import { activeTab } from '$lib/state/workspace.svelte';
import { ui } from '$lib/state/ui.svelte';
import { selection } from '$lib/state/selection.svelte';
import { inputBox } from '$lib/quickinput/qi.svelte';
import { t } from '$lib/i18n/index.svelte';

/** Board the user is acting on: editor (if focused/open) or the active tab. */
export function activeBoard(): BoardModel | null {
  if (ui.editor.open && ui.editor.boardId) return boards.get(ui.editor.boardId) ?? null;
  const tab = activeTab();
  if (tab?.boardId) return boards.get(tab.boardId) ?? null;
  return null;
}

/** Board of the active tab only (ignores the editor). */
export function tabBoard(): BoardModel | null {
  const tab = activeTab();
  return tab?.boardId ? (boards.get(tab.boardId) ?? null) : null;
}

/** Card the user is acting on: open editor card, doc tab, or selection focus. */
export function activeCard(): { board: BoardModel; id: string } | null {
  if (ui.editor.open && ui.editor.cardId) {
    const b = boards.get(ui.editor.boardId);
    if (b?.node(ui.editor.cardId)) return { board: b, id: ui.editor.cardId };
  }
  const tab = activeTab();
  if (tab?.kind === 'doc' && tab.boardId && tab.cardId) {
    const b = boards.get(tab.boardId);
    if (b) return { board: b, id: tab.cardId };
  }
  const b = tabBoard();
  if (b && selection.boardId === b.id && selection.focus && b.node(selection.focus)) return { board: b, id: selection.focus };
  return null;
}

export function selectedCards(): { board: BoardModel; ids: string[] } | null {
  const b = tabBoard();
  if (!b || selection.boardId !== b.id) return null;
  const ids = selection.ids.filter((id) => b.node(id));
  return ids.length ? { board: b, ids } : null;
}

export async function pickFolder(title: string, defaultPath?: string): Promise<string | null> {
  // Native dialogs run in the backend so it can remember what the user picked
  // (file RPCs only accept picked locations).
  if (isTauri) return rpc<string | null>('dialog.pickFolder', { title, defaultPath });
  const r = await inputBox({ title, placeholder: '~/Documents/My board', value: defaultPath });
  return typeof r === 'string' && r ? r : null;
}

export async function pickFile(title: string, filters?: { name: string; extensions: string[] }[], multiple = false): Promise<string[] | null> {
  if (!isTauri) return null;
  return rpc<string[] | null>('dialog.pickFiles', { title, filters, multiple });
}

export async function pickSavePath(title: string, defaultPath: string, filters?: { name: string; extensions: string[] }[]): Promise<string | null> {
  if (!isTauri) return null;
  return rpc<string | null>('dialog.save', { title, defaultPath, filters });
}

export async function reveal(path: string) {
  if (!isTauri) return;
  const { revealItemInDir } = await import('@tauri-apps/plugin-opener');
  await revealItemInDir(path);
}

export async function openExternal(url: string) {
  if (!/^(https?:|mailto:)/i.test(url)) return;
  if (isTauri) {
    const { openUrl } = await import('@tauri-apps/plugin-opener');
    await openUrl(url);
  } else window.open(url, '_blank', 'noopener');
}

export async function copyText(text: string) {
  if (isTauri) {
    const { writeText } = await import('@tauri-apps/plugin-clipboard-manager');
    await writeText(text);
  } else await navigator.clipboard.writeText(text);
}

/** Plain text on the clipboard ('' when empty or unreadable). Never reads images. */
export async function readClipboardText(): Promise<string> {
  if (isTauri) {
    const { readText } = await import('@tauri-apps/plugin-clipboard-manager');
    return (await readText().catch(() => '')) ?? '';
  }
  return navigator.clipboard.readText().catch(() => '');
}

export function joinPath(...parts: string[]): string {
  const sep = parts[0]?.includes('\\') && !parts[0].includes('/') ? '\\' : '/';
  return parts
    .map((p, i) => (i === 0 ? p.replace(/[\\/]+$/, '') : p.replace(/^[\\/]+|[\\/]+$/g, '')))
    .filter(Boolean)
    .join(sep);
}

export function baseName(p: string): string {
  return p.split(/[\\/]/).filter(Boolean).pop() ?? p;
}

export async function pathExists(path: string): Promise<{ exists: boolean; isDir: boolean; isBoard: boolean }> {
  return rpc('path.exists', { path });
}

export const untitled = () => t('common.untitled');
