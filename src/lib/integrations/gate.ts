// UI side of the WriteGate (SPEC §9.6): prepare in Rust → confirmation
// dialog "This will update X and Y on Z" → commit with the single-use token.

import { rpc, RpcError } from '$lib/backend/rpc';
import { confirm } from '$lib/state/dialogs.svelte';
import { toast } from '$lib/state/toasts.svelte';
import { settings } from '$lib/settings/store.svelte';
import { t, i18n } from '$lib/i18n/index.svelte';
import { runCommand } from '$lib/commands/registry.svelte';
import type { Prepared, PrepareRequest } from './types';
import { changeMessage, fieldLabel, serviceName } from './message';

export { changeMessage, fieldLabel, serviceName };

/** Human message for integration error codes (never includes secrets). */
export function errorText(e: unknown): string {
  const raw = e instanceof Error ? e.message : String(e);
  const code = raw.replace(/^.*?: /, '').trim();
  const base = code.split(':')[0];
  const key = `integrations.errors.${base}`;
  const v = t(key, { detail: code.split(':').slice(1).join(':') });
  return v === key ? raw : v;
}

function isDisabled(e: unknown, which: 'push' | 'pull') {
  return e instanceof RpcError && e.message.includes(`${which}_disabled`);
}

/** Run a gated write. Returns the commit result, or null when cancelled / nothing to do. */
export async function runGated<T = unknown>(req: PrepareRequest, opts: { quietEmpty?: boolean } = {}): Promise<T | null> {
  let p: Prepared;
  try {
    p = await rpc<Prepared>('remote.prepare', { request: req });
  } catch (e) {
    for (const which of ['push', 'pull'] as const) {
      if (isDisabled(e, which)) {
        toast.warn(t(`integrations.errors.${which}_disabled`), {
          action: { label: t('integrations.gate.enable'), run: () => void runCommand(which === 'push' ? 'remote.toggleAllowPush' : 'remote.toggleAllowPull') },
        });
        return null;
      }
    }
    toast.error(errorText(e));
    return null;
  }
  if (p.empty) {
    if (!opts.quietEmpty) toast.info(t('integrations.gate.upToDate', { target: p.target }));
    return null;
  }
  const ask = p.direction === 'push' ? settings.get<boolean>('integrations.confirmPush') !== false : settings.get<boolean>('integrations.confirmPull') === true;
  if (ask) {
    const ok = await confirm({
      title: t(p.direction === 'push' ? 'integrations.gate.titlePush' : 'integrations.gate.titlePull', { service: serviceName(p.service) }),
      message: changeMessage(p, i18n.locale, t),
      changes: p.changes.map((c) => ({ field: fieldLabel(c.field, t), before: c.before ?? '—', after: c.after ?? '—' })),
      preview: p.preview ?? undefined,
      confirmLabel: t('integrations.gate.confirm'),
      cancelFocused: true,
    });
    if (!ok) {
      void rpc('remote.cancel', { token: p.token }).catch(() => {});
      return null;
    }
  }
  try {
    return await rpc<T>('remote.commit', { token: p.token });
  } catch (e) {
    toast.error(errorText(e));
    return null;
  }
}
