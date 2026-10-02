// "Set up local AI…": pick a backend and a model, then Luau downloads,
// verifies, installs and connects (Ollama), or guides you through LM Studio.
// Progress shows in a toast; the shell also sends a notification at the end.

import { rpc, onCoreEvent } from '$lib/backend/rpc';
import { pickOne, inputBox, BACK } from '$lib/quickinput/qi.svelte';
import { settings } from '$lib/settings/store.svelte';
import { toast, dismiss, updateToast } from '$lib/state/toasts.svelte';
import { openExternal } from '$lib/app/helpers';
import { t } from '$lib/i18n/index.svelte';

interface SetupProgress {
  step: 'check' | 'start' | 'download' | 'verify' | 'install' | 'connect' | 'model' | 'done';
  completed?: number | null;
  total?: number | null;
  detail?: string;
}

/** Suggested models: small enough for a laptop, good at writing. */
export const SUGGESTED_MODELS = [
  { name: 'gemma3:4b', size: '3.3 GB', note: 'balanced' },
  { name: 'llama3.2:3b', size: '2.0 GB', note: 'fastest' },
  { name: 'qwen3:8b', size: '5.2 GB', note: 'best' },
] as const;

const LMSTUDIO_URL = 'https://lmstudio.ai/download';

function gb(n: number) {
  return `${(n / 1024 ** 3).toFixed(n > 10 * 1024 ** 3 ? 0 : 1)} GB`;
}

/** Toast text for one progress event (pure). */
export function progressText(p: SetupProgress): string {
  const pct = p.total ? ` ${Math.min(100, Math.round(((p.completed ?? 0) / p.total) * 100))}%` : '';
  const size = p.total ? ` (${gb(p.completed ?? 0)} / ${gb(p.total)})` : '';
  if (p.step === 'download') return t('localSetup.step.download') + pct + size;
  if (p.step === 'model') return t('localSetup.step.model', { name: p.detail ?? '' }) + pct;
  return t(`localSetup.step.${p.step}`);
}

export async function openLocalSetup() {
  const backend = await pickOne(
    [
      { label: 'Ollama', description: t('localSetup.ollama'), value: 'ollama' as const },
      { label: 'LM Studio', description: t('localSetup.lmstudio'), value: 'lmstudio' as const },
    ],
    { title: t('aiSource.local.setup'), placeholder: t('localSetup.pickBackend') },
  );
  if (!backend || backend === BACK) return;

  if (backend === 'lmstudio') {
    void openExternal(LMSTUDIO_URL);
    settings.set('ai.provider', 'openai');
    settings.set('ai.endpoint', 'http://localhost:1234');
    toast.info(t('localSetup.lmstudioSteps'), { timeout: 15000 });
    return;
  }

  let model = await pickOne<string>(
    [
      ...SUGGESTED_MODELS.map((m) => ({ label: m.name, description: `${m.size} · ${t(`localSetup.models.${m.note}`)}`, value: m.name })),
      { label: t('localSetup.otherModel'), value: '__other' },
      { label: t('localSetup.noModel'), value: '' },
    ],
    { title: t('aiSource.local.setup'), placeholder: t('localSetup.pickModel') },
  );
  if (model === undefined || model === BACK) return;
  if (model === '__other') {
    const v = await inputBox({ title: t('aiSource.local.setup'), placeholder: t('ai.pullPrompt'), value: '' });
    if (!v || v === BACK) return;
    model = v.trim();
  }

  const tid = toast.info(t('localSetup.step.check'), { timeout: 0, action: { label: t('common.cancel'), run: () => void rpc('ai.setupCancel') } });
  const off = onCoreEvent((e) => {
    if (e.type === 'custom' && e.name === 'ai.setup') updateToast(tid, progressText(e.payload as SetupProgress));
  });
  try {
    const r = await rpc<{ endpoint: string; model: string | null; installed: boolean }>('ai.setupLocal', {
      model: model || null,
      texts: { doneTitle: t('localSetup.doneTitle'), doneBody: t('localSetup.doneBody'), failedTitle: t('localSetup.failedTitle') },
    });
    settings.set('ai.provider', 'ollama');
    settings.set('ai.endpoint', r.endpoint);
    if (r.model) settings.set('ai.model', r.model);
    toast.success(r.model ? t('localSetup.readyWith', { model: r.model }) : t('localSetup.ready'));
  } catch (e) {
    const msg = (e as Error).message;
    if (msg === 'cancelled') toast.info(t('localSetup.cancelled'));
    else toast.error(t('localSetup.failed', { message: msg }));
  } finally {
    off();
    dismiss(tid);
  }
}
