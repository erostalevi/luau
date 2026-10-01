// Code cells: run through the backend, asking once per board for trust.

import { rpc, RpcError } from '$lib/backend/rpc';
import { confirm } from '$lib/state/dialogs.svelte';
import { t } from '$lib/i18n/index.svelte';
import type { CodeResult } from '$lib/editor/cm/context';

export const isNeedsTrust = (e: unknown) => e instanceof RpcError && e.code === 'conflict' && e.message.includes('needs_trust');

/** Run a cell; on `needs_trust` ask the user, record trust for the board and retry once. */
export async function runCodeCell(lang: string, code: string, board: string | undefined, card?: string): Promise<CodeResult> {
  const run = () => rpc<CodeResult>('code.run', { lang, code, board, card });
  try {
    return await run();
  } catch (e) {
    if (!isNeedsTrust(e)) throw e;
    const ok = await confirm({ title: t('code.trustTitle'), message: t('code.trustMessage'), confirmLabel: t('code.trustConfirm'), cancelFocused: true });
    if (!ok) throw new Error(t('code.trustMessage'));
    await rpc('code.trust', { board, trusted: true });
    return run();
  }
}
