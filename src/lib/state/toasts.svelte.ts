// Toast notifications (bottom-center, calm, auto-dismiss).

export interface ToastAction {
  label: string;
  run: () => void;
}

export interface Toast {
  id: number;
  level: 'info' | 'success' | 'warn' | 'error';
  message: string;
  action?: ToastAction;
  timeout: number;
}

export const toasts = $state<{ list: Toast[] }>({ list: [] });
let nextId = 1;

function push(level: Toast['level'], message: string, opts: { action?: ToastAction; timeout?: number } = {}) {
  const id = nextId++;
  const timeout = opts.timeout ?? (level === 'error' ? 7000 : opts.action ? 6000 : 3200);
  toasts.list.push({ id, level, message, action: opts.action, timeout });
  if (toasts.list.length > 4) toasts.list.shift();
  if (timeout > 0) setTimeout(() => dismiss(id), timeout);
  return id;
}

export function dismiss(id: number) {
  const i = toasts.list.findIndex((t) => t.id === id);
  if (i >= 0) toasts.list.splice(i, 1);
}

/** Change a toast's text in place (progress); no-op when it is gone. */
export function updateToast(id: number, message: string) {
  const it = toasts.list.find((t) => t.id === id);
  if (it) it.message = message;
}

export const toast = {
  info: (m: string, o?: { action?: ToastAction; timeout?: number }) => push('info', m, o),
  success: (m: string, o?: { action?: ToastAction; timeout?: number }) => push('success', m, o),
  warn: (m: string, o?: { action?: ToastAction; timeout?: number }) => push('warn', m, o),
  error: (m: string, o?: { action?: ToastAction; timeout?: number }) => push('error', m, o),
};
