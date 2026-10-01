// Code cells: run through the backend, asking once per board for trust.

import { rpc, RpcError, isTauri } from '$lib/backend/rpc';
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
    if (!board) throw e;
    // The desktop app asks with a native dialog (backend); the web build mocks it.
    const args = { board, trusted: true, title: t('code.trustTitle'), message: t('code.trustMessage'), confirm: t('code.trustConfirm') };
    if (isTauri) await rpc('code.trust', args).catch(() => Promise.reject(new Error(t('code.trustMessage'))));
    else {
      const ok = await confirm({ title: t('code.trustTitle'), message: t('code.trustMessage'), confirmLabel: t('code.trustConfirm'), cancelFocused: true });
      if (!ok) throw new Error(t('code.trustMessage'));
      await rpc('code.trust', args);
    }
    return run();
  }
}
