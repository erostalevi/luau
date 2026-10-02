<script lang="ts">
  // Settings › AI: where the AI runs. Three sources (native Apple model, local
  // models via Ollama / OpenAI-compatible servers, remote services with an API
  // key) plus Auto and Off. Status, connection test and key management live
  // here; keys go to the OS keychain and are never read back into the page.
  import { Sparkles, Cpu, Server, Cloud, PowerOff, Circle, RefreshCw, KeyRound, ShieldCheck, ExternalLink, Download, Wand2, Check } from '@lucide/svelte';
  import Segmented from '$lib/components/Segmented.svelte';
  import { ai, isRemote, REMOTE_PROVIDERS, type AiStatus, type AiTestResult, type RemoteProvider, type RemoteState } from '$lib/summaries/api';
  import { describeStatus } from '$lib/summaries/aiStatus';
  import { pullModel } from '$lib/summaries/summaries.commands';
  import { settings, flushSettings } from '$lib/settings/store.svelte';
  import { openExternal } from '$lib/app/helpers';
  import { toast } from '$lib/state/toasts.svelte';
  import { tip } from '$lib/components/tooltip';
  import { t } from '$lib/i18n/index.svelte';

  type Source = 'auto' | 'native' | 'local' | 'remote' | 'off';

  const KEY_URL: Record<RemoteProvider, string> = {
    anthropic: 'https://platform.claude.com/settings/keys',
    chatgpt: 'https://platform.openai.com/api-keys',
    gemini: 'https://aistudio.google.com/apikey',
    openrouter: 'https://openrouter.ai/keys',
  };

  const provider = $derived(String(settings.get('ai.provider') ?? 'auto'));
  const sourceOf = (p: string): Source =>
    p === 'apple' ? 'native' : p === 'ollama' || p === 'openai' ? 'local' : isRemote(p) ? 'remote' : p === 'off' ? 'off' : 'auto';

  // The tab is UI state: the Remote tab shows the services but changes nothing
  // until one is picked (and allowed).
  let tab = $state<Source>(sourceOf(String(settings.get('ai.provider') ?? 'auto')));
  let status = $state<AiStatus | null>(null);
  let remote = $state<RemoteState[]>([]);
  let models = $state<string[]>([]);
  let busy = $state(false);
  let testing = $state(false);
  let test = $state<{ ok: true; r: AiTestResult } | { ok: false; message: string } | null>(null);
  let keyDraft = $state('');
  let showKeyInput = $state(false);
  let endpointDraft = $state(String(settings.get('ai.endpoint') ?? ''));

  const view = $derived(status ? describeStatus(status) : null);
  const selected = $derived<RemoteProvider | null>(isRemote(provider) ? provider : null);
  const stateOf = (p: RemoteProvider) => remote.find((r) => r.provider === p);

  async function refresh() {
    busy = true;
    test = null;
    await flushSettings().catch(() => {});
    const [s, r] = await Promise.all([ai.status().catch(() => null), ai.remoteState().catch(() => [] as RemoteState[])]);
    status = s;
    remote = r;
    busy = false;
    void loadModels();
  }

  async function loadModels() {
    models = [];
    const p = provider;
    if (isRemote(p)) {
      if (stateOf(p)?.hasKey) models = await ai.remoteModels(p).catch(() => []);
    } else if (p === 'ollama' || p === 'openai' || (p === 'auto' && status?.available && status.provider !== 'apple')) {
      models = (await ai.models().catch(() => [])).map((m) => m.name);
    }
  }

  $effect(() => {
    void settings.get('ai.provider');
    void settings.get('ai.endpoint');
    void settings.get('ai.model');
    void settings.get('ai.remoteModel');
    void refresh();
  });

  function pickTab(s: Source) {
    tab = s;
    if (s === 'auto') settings.set('ai.provider', 'auto');
    else if (s === 'native') settings.set('ai.provider', 'apple');
    else if (s === 'local') settings.set('ai.provider', provider === 'openai' ? 'openai' : 'ollama');
    else if (s === 'off') settings.set('ai.provider', 'off');
  }

  const serviceName = (p: RemoteProvider) => t(`aiSource.remote.${p}.name`);

  async function allow(p: RemoteProvider): Promise<boolean> {
    try {
      await ai.consent(p, true, {
        service: `${serviceName(p)} · ${t(`aiSource.remote.${p}.by`)}`,
        title: t('aiSource.consent.title'),
        message: t('aiSource.consent.message', { service: serviceName(p) }),
        confirm: t('aiSource.consent.allow'),
        cancel: t('common.cancel'),
      });
      return true;
    } catch (e) {
      if ((e as { code?: string }).code !== 'cancelled') toast.error((e as Error).message);
      return false;
    }
  }

  async function pickRemote(p: RemoteProvider) {
    if (!stateOf(p)?.consent && !(await allow(p))) return;
    if (provider !== p) {
      settings.set('ai.remoteModel', '');
      settings.set('ai.provider', p);
    }
    showKeyInput = !stateOf(p)?.hasKey;
    await refresh();
  }

  async function withdraw(p: RemoteProvider) {
    await ai.consent(p, false).catch(() => {});
    if (provider === p) settings.set('ai.provider', 'auto');
    tab = provider === p ? 'auto' : tab;
    await refresh();
  }

  async function saveKey(p: RemoteProvider) {
    try {
      const { persisted } = await ai.setKey(p, keyDraft);
      keyDraft = '';
      showKeyInput = false;
      toast.success(persisted ? t('aiSource.key.saved') : t('aiSource.key.sessionOnly'));
      await refresh();
    } catch (e) {
      toast.error((e as Error).message);
    }
  }

  async function removeKey(p: RemoteProvider) {
    await ai.deleteKey(p).catch(() => {});
    showKeyInput = true;
    await refresh();
  }

  async function runTest() {
    testing = true;
    test = null;
    try {
      test = { ok: true, r: await ai.test() };
    } catch (e) {
      test = { ok: false, message: (e as Error).message };
    }
    testing = false;
  }

  function setEndpoint() {
    const v = endpointDraft.trim();
    if (v !== settings.get('ai.endpoint')) settings.set('ai.endpoint', v || 'http://localhost:11434');
  }

  async function openSetup() {
    const { openLocalSetup } = await import('./localSetup');
    void openLocalSetup();
  }

  // What the status line says.
  const line = $derived.by(() => {
    if (!status) return { ok: false, title: t('ai.testing'), hint: '' };
    if (isRemote(status.provider)) {
      return {
        ok: status.available,
        title: `${serviceName(status.provider)}${status.model ? ` · ${status.model}` : ''}`,
        hint: status.available ? t('aiSource.status.remoteReady') : (status.error ?? ''),
      };
    }
    if (!view) return { ok: false, title: '', hint: '' };
    const model = view.ok && status.model && view.provider !== 'apple' ? ` · ${status.model}` : '';
    return { ok: view.ok, title: t(`ai.providerName.${view.provider}`) + model, hint: view.hint ? t(view.hint) : '' };
  });
</script>

<section class="ai-source" aria-label={t('aiSource.title')}>
  <Segmented
    full
    value={tab}
    onchange={pickTab}
    options={[
      { value: 'auto', label: t('aiSource.tabs.auto'), icon: Sparkles },
      { value: 'native', label: t('aiSource.tabs.native'), icon: Cpu },
      { value: 'local', label: t('aiSource.tabs.local'), icon: Server },
      { value: 'remote', label: t('aiSource.tabs.remote'), icon: Cloud },
      { value: 'off', label: t('aiSource.tabs.off'), icon: PowerOff },
    ]}
  />

  <p class="about">{t(`aiSource.about.${tab}`)}</p>

  {#if tab === 'local'}
    <div class="row">
      <span class="k">{t('aiSource.local.server')}</span>
      <Segmented
        size="sm"
        value={provider === 'openai' ? 'openai' : 'ollama'}
        onchange={(v) => settings.set('ai.provider', v)}
        options={[
          { value: 'ollama', label: 'Ollama' },
          { value: 'openai', label: t('aiSource.local.compatible') },
        ]}
      />
    </div>
    <label class="row">
      <span class="k">{t('aiSource.local.address')}</span>
      <input class="input" spellcheck="false" bind:value={endpointDraft} onchange={setEndpoint} placeholder="http://localhost:11434" />
    </label>
    <div class="row">
      <span class="k">{t('aiSource.model')}</span>
      <select
        class="field"
        value={String(settings.get('ai.model') ?? '')}
        onchange={(e) => settings.set('ai.model', (e.currentTarget as HTMLSelectElement).value)}
      >
        <option value="">{t('aiSource.modelAuto')}</option>
        {#each models as m (m)}<option value={m}>{m}</option>{/each}
      </select>
      {#if provider !== 'openai'}
        <button class="icon-btn sm" aria-label={t('commands.ai.pullModel')} use:tip={t('commands.ai.pullModel')} onclick={() => void pullModel().then(refresh)}>
          <Download size={14} />
        </button>
      {/if}
    </div>
    <div class="actions">
      <button class="btn" onclick={openSetup}><Wand2 size={14} />{t('aiSource.local.setup')}</button>
    </div>
  {:else if tab === 'remote'}
    <div class="services" role="radiogroup" aria-label={t('aiSource.tabs.remote')}>
      {#each REMOTE_PROVIDERS as p (p)}
        {@const st = stateOf(p)}
        <button class="service" class:on={selected === p} role="radio" aria-checked={selected === p} onclick={() => void pickRemote(p)}>
          <span class="name">{serviceName(p)}</span>
          <span class="by">{t(`aiSource.remote.${p}.by`)}</span>
          <span class="badges">
            {#if st?.hasKey}<span class="badge" use:tip={t('aiSource.key.has')}><KeyRound size={11} /></span>{/if}
            {#if st?.consent}<span class="badge" use:tip={t('aiSource.consent.given')}><ShieldCheck size={11} /></span>{/if}
          </span>
        </button>
      {/each}
    </div>

    {#if selected}
      {@const st = stateOf(selected)}
      <div class="row">
        <span class="k">{t('aiSource.key.label')}</span>
        {#if st?.hasKey && !showKeyInput}
          <span class="val"><Check size={13} /> {t('aiSource.key.has')}</span>
          <button class="btn sm" onclick={() => (showKeyInput = true)}>{t('aiSource.key.replace')}</button>
          <button class="btn sm danger" onclick={() => void removeKey(selected)}>{t('aiSource.key.remove')}</button>
        {:else}
          <input
            class="input"
            type="password"
            autocomplete="off"
            spellcheck="false"
            placeholder={t('aiSource.key.placeholder')}
            bind:value={keyDraft}
            onkeydown={(e) => e.key === 'Enter' && keyDraft.trim() && void saveKey(selected)}
          />
          <button class="btn sm primary" disabled={!keyDraft.trim()} onclick={() => void saveKey(selected)}>{t('common.save')}</button>
          <button class="btn sm link" onclick={() => void openExternal(KEY_URL[selected])}>{t('aiSource.key.get')}<ExternalLink size={12} /></button>
        {/if}
      </div>
      <div class="row">
        <span class="k">{t('aiSource.model')}</span>
        <select
          class="field"
          disabled={!st?.hasKey}
          value={String(settings.get('ai.remoteModel') ?? '')}
          onchange={(e) => settings.set('ai.remoteModel', (e.currentTarget as HTMLSelectElement).value)}
        >
          <option value="">{t('aiSource.modelAuto')}</option>
          {#each models as m (m)}<option value={m}>{m}</option>{/each}
        </select>
      </div>
      <div class="row consent">
        <ShieldCheck size={14} />
        {#if st?.consent}
          <span class="grow">{t('aiSource.consent.given')} · {t('aiSource.consent.note', { service: serviceName(selected) })}</span>
          <button class="btn sm" onclick={() => void withdraw(selected)}>{t('aiSource.consent.withdraw')}</button>
        {:else}
          <span class="grow">{t('aiSource.consent.needed', { service: serviceName(selected) })}</span>
          <button class="btn sm primary" onclick={() => void allow(selected).then(refresh)}>{t('aiSource.consent.allowEllipsis')}</button>
        {/if}
      </div>
    {:else}
      <p class="hint">{t('aiSource.remote.pick')}</p>
    {/if}
  {/if}

  {#if tab !== 'remote' || selected}
    <div class="status" aria-live="polite">
      <span class="dot" class:ok={line.ok}><Circle size={8} strokeWidth={0} fill="currentColor" /></span>
      <span class="grow">
        <strong>{line.title}</strong>
        {#if line.hint}<span class="hint">{line.hint}</span>{/if}
        {#if status?.contextSize && line.ok && status.provider === 'apple'}<span class="hint">{t('ai.contextSize', { n: status.contextSize })}</span>{/if}
        {#if test}
          <span class="test" class:bad={!test.ok}>
            {test.ok ? t('aiSource.test.ok', { model: test.r.model, ms: test.r.ms }) : t('aiSource.test.failed', { message: test.message })}
          </span>
        {/if}
      </span>
      {#if tab !== 'off'}
        <button class="btn sm" onclick={runTest} disabled={testing || busy}>{testing ? t('ai.testing') : t('ai.test')}</button>
      {/if}
      <button class="icon-btn sm" aria-label={t('ai.checkAgain')} use:tip={t('ai.checkAgain')} onclick={refresh} disabled={busy}>
        <RefreshCw size={14} strokeWidth={1.8} />
      </button>
    </div>
  {/if}
</section>

<style>
  .ai-source {
    display: flex;
    flex-direction: column;
    gap: var(--sp-3);
    width: calc(100% - var(--sp-6));
    margin: var(--sp-2) 0 var(--sp-3) var(--sp-6);
    padding: var(--sp-4);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    background: var(--primary-softer);
  }
  .about,
  .hint {
    margin: 0;
    font-size: var(--fs-sm);
    color: var(--ink-2);
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    min-width: 0;
  }
  .row .k {
    flex: none;
    width: 96px;
    font-size: var(--fs-sm);
    color: var(--ink-3);
  }
  .row .input,
  .row .field {
    flex: 1;
    min-width: 0;
  }
  .val {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    flex: 1;
    color: var(--ok);
    font-size: var(--fs-sm);
  }
  .actions {
    display: flex;
    gap: var(--sp-2);
  }
  .btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .services {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
    gap: var(--sp-2);
  }
  .service {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
    padding: var(--sp-3);
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    background: var(--bg-elev);
    color: var(--ink);
    text-align: left;
    cursor: pointer;
  }
  .service.on {
    border-color: var(--primary);
    box-shadow: 0 0 0 2px var(--primary-soft);
  }
  .service .name {
    font-weight: var(--fw-medium);
  }
  .service .by {
    font-size: var(--fs-sm);
    color: var(--ink-3);
  }
  .badges {
    position: absolute;
    top: 8px;
    right: 8px;
    display: flex;
    gap: 4px;
    color: var(--ok);
  }
  .consent {
    font-size: var(--fs-sm);
    color: var(--ink-2);
  }
  .grow {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .status {
    display: flex;
    align-items: flex-start;
    gap: var(--sp-2);
    padding-top: var(--sp-3);
    border-top: 1px solid var(--line);
  }
  .status strong {
    font-weight: var(--fw-medium);
    color: var(--ink);
    overflow-wrap: anywhere;
  }
  .dot {
    display: inline-flex;
    padding-top: 5px;
    color: var(--ink-4);
  }
  .dot.ok {
    color: var(--ok);
  }
  .test {
    font-size: var(--fs-sm);
    color: var(--ok);
  }
  .test.bad {
    color: var(--danger);
  }
</style>
