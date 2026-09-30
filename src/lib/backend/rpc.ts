// Transport: Tauri IPC when running natively, an in-memory mock in a browser.

import type { CoreEvent } from './types';

export class RpcError extends Error {
  code: string;
  constructor(code: string, message: string) {
    super(message);
    this.code = code;
  }
}

type Listener = (e: CoreEvent) => void;
type MenuListener = (commandId: string) => void;

export interface Transport {
  call<T>(method: string, params?: Record<string, unknown>): Promise<T>;
  onEvent(fn: Listener): () => void;
  onMenu(fn: MenuListener): () => void;
}

export const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

let transport: Transport | null = null;

async function tauriTransport(): Promise<Transport> {
  const { invoke } = await import('@tauri-apps/api/core');
  const { listen } = await import('@tauri-apps/api/event');
  const { getCurrentWebviewWindow } = await import('@tauri-apps/api/webviewWindow');
  const win = getCurrentWebviewWindow();
  const listeners = new Set<Listener>();
  const menuListeners = new Set<MenuListener>();
  await listen<CoreEvent>('lull://event', (ev) => listeners.forEach((l) => l(ev.payload)));
  await win.listen<string>('lull://menu', (ev) => menuListeners.forEach((l) => l(ev.payload)));
  return {
    async call<T>(method: string, params: Record<string, unknown> = {}) {
      try {
        return (await invoke('rpc', { method, params })) as T;
      } catch (e) {
        const err = e as { code?: string; message?: string };
        throw new RpcError(err?.code ?? 'other', err?.message ?? String(e));
      }
    },
    onEvent(fn) {
      listeners.add(fn);
      return () => listeners.delete(fn);
    },
    onMenu(fn) {
      menuListeners.add(fn);
      return () => menuListeners.delete(fn);
    },
  };
}

export async function initTransport(): Promise<Transport> {
  if (transport) return transport;
  if (isTauri) {
    transport = await tauriTransport();
  } else {
    const { createMockTransport } = await import('./mock');
    transport = createMockTransport();
  }
  return transport;
}

export function rpc<T>(method: string, params?: Record<string, unknown>): Promise<T> {
  if (!transport) throw new Error('transport not initialized');
  return transport.call<T>(method, params);
}

export function onCoreEvent(fn: Listener): () => void {
  if (!transport) throw new Error('transport not initialized');
  return transport.onEvent(fn);
}

export function onMenu(fn: MenuListener): () => void {
  if (!transport) throw new Error('transport not initialized');
  return transport.onMenu(fn);
}
