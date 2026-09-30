// Modal confirmation dialogs (used for destructive actions and for every
// write to an external service — see SPEC §9.6).

export interface ChangeRow {
  field: string;
  before?: string | null;
  after?: string | null;
}

export interface ConfirmOptions {
  title: string;
  message?: string;
  /** Field changes rendered as a before → after table. */
  changes?: ChangeRow[];
  /** Free-form preview (e.g. a Slack message). */
  preview?: string;
  confirmLabel?: string;
  cancelLabel?: string;
  danger?: boolean;
  /** Focus Cancel by default (external writes). */
  cancelFocused?: boolean;
  /** Optional "don't ask again" checkbox label; resolves `{ ok, dontAsk }`. */
  dontAskLabel?: string;
}

interface DialogState extends ConfirmOptions {
  id: number;
  resolve: (r: { ok: boolean; dontAsk: boolean }) => void;
}

export const dialogs = $state<{ list: DialogState[] }>({ list: [] });
let nextId = 1;

export function confirmEx(opts: ConfirmOptions): Promise<{ ok: boolean; dontAsk: boolean }> {
  return new Promise((resolve) => {
    dialogs.list.push({ ...opts, id: nextId++, resolve });
  });
}

export async function confirm(opts: ConfirmOptions): Promise<boolean> {
  return (await confirmEx(opts)).ok;
}

export function closeDialog(id: number, ok: boolean, dontAsk = false) {
  const i = dialogs.list.findIndex((d) => d.id === id);
  if (i < 0) return;
  const [d] = dialogs.list.splice(i, 1);
  d.resolve({ ok, dontAsk });
}
