// Commands for activity summaries, schedules, card summaries and local AI.

import { Sparkles, CalendarClock, History, PlugZap, Download, ScrollText } from '@lucide/svelte';
import type { Command } from '$lib/commands/registry.svelte';
import { openSingleton } from '$lib/state/workspace.svelte';
import { settings } from '$lib/settings/store.svelte';
import { toast, dismiss } from '$lib/state/toasts.svelte';
import { confirm } from '$lib/state/dialogs.svelte';
import { inputBox, BACK } from '$lib/quickinput/qi.svelte';
import { activeCard, copyText } from '$lib/app/helpers';
import { t } from '$lib/i18n/index.svelte';
import { ai, onAiEvent, type PullProgress } from './api';

const openSummary = (payload: Record<string, unknown>) => openSingleton('summary', { ...payload, nonce: Date.now() });

export async function testConnection(): Promise<boolean> {
  const s = await ai.status().catch((e: Error) => ({ available: false, model: null, error: e.message, provider: 'none' }) as const);
  if (s.available && s.model) {
    toast.success(t('ai.connected', { model: s.model }));
    return true;
  }
  const message = s.provider === 'off' ? t('ai.off') : s.available ? t('ai.noModel') : (s.error ?? t('ai.unavailable'));
  toast.warn(t('ai.notConnected', { message }));
  return false;
}

export async function pullModel(name?: string) {
  const model = name ?? (await inputBox({ title: t('commands.ai.pullModel'), placeholder: t('ai.pullPrompt'), value: 'llama3.2:3b' }));
  if (!model || model === BACK) return;
  let tid = toast.info(t('ai.pulling', { name: model, pct: '' }), { timeout: 0 });
  const off = onAiEvent<PullProgress>('ai.pull', (p) => {
    if (p.name && p.name !== model) return;
    const pct = p.total ? `${Math.round(((p.completed ?? 0) / p.total) * 100)}%` : p.status;
    dismiss(tid);
    tid = toast.info(t('ai.pulling', { name: model, pct }), { timeout: 0 });
  });
  try {
    await ai.pullModel(model);
    toast.success(t('ai.pulled', { name: model }));
  } catch (e) {
    toast.error(t('ai.notConnected', { message: (e as Error).message }));
  } finally {
    off();
    dismiss(tid);
  }
}

async function summarizeCard() {
  const c = activeCard();
  if (!c) return;
  try {
    const r = await ai.cardSummarize({ board: c.board.id, id: c.id, engine: 'auto' });
    const ok = await confirm({ title: t('ai.cardSummary'), message: r.text, confirmLabel: t('summaries.copy') });
    if (ok) {
      await copyText(r.text);
      toast.success(t('summaries.copied'));
    }
  } catch (e) {
    toast.error(t('summaries.failed', { message: (e as Error).message }));
  }
}

export const commands: Command[] = [
  {
    id: 'summary.create',
    title: 'commands.summary.create',
    category: 'ai',
    icon: Sparkles,
    run: (args?: Record<string, unknown>) => openSummary({ ...(args ?? {}), view: 'summary' }),
  },
  {
    id: 'summary.yesterday',
    title: 'commands.summary.yesterday',
    category: 'ai',
    icon: History,
    run: () => openSummary({ view: 'summary', preset: 'yesterday', auto: true }),
  },
  { id: 'summary.openSchedules', title: 'commands.summary.openSchedules', category: 'ai', icon: CalendarClock, run: () => openSummary({ view: 'schedules' }) },
  { id: 'card.summarize', title: 'commands.card.summarize', category: 'card', icon: ScrollText, run: summarizeCard },
  { id: 'ai.testConnection', title: 'commands.ai.testConnection', category: 'ai', icon: PlugZap, run: () => testConnection() },
  { id: 'ai.pullModel', title: 'commands.ai.pullModel', category: 'ai', icon: Download, run: (args?: { name?: string }) => pullModel(args?.name) },
];

export function init() {
  settings.register([
    { key: 'ai.timeoutSec', type: 'number', default: 180, category: 'ai', min: 10, max: 1800, step: 10, source: 'core' },
    { key: 'summaries.defaultDetail', type: 'number', default: 3, category: 'ai', min: 1, max: 5, step: 1, source: 'core' },
    { key: 'summaries.links', type: 'boolean', default: true, category: 'ai', source: 'core' },
  ]);
}
