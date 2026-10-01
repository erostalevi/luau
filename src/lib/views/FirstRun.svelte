<script lang="ts">
  import { fade, fly } from 'svelte/transition';
  import { Sun, Moon, Monitor, PanelRight, AppWindow } from '@lucide/svelte';
  import { ui } from '$lib/state/ui.svelte';
  import { settings } from '$lib/settings/store.svelte';
  import { setLocale, LOCALES, t, type Locale } from '$lib/i18n/index.svelte';
  import { PRESET_IDS } from '$lib/keybindings/defaults';
  import { runCommand } from '$lib/commands/registry.svelte';
  import { pickFolder } from '$lib/app/helpers';
  import Segmented from '$lib/components/Segmented.svelte';
  import Toggle from '$lib/components/Toggle.svelte';
  import { DEFAULT_SWATCHES } from '$lib/components/ColorPicker.svelte';
  import { trapFocus } from '$lib/components/focusTrap';

  let step = $state(0);
  const steps = 5;
  let lang = $state((settings.get<string>('general.language') === 'auto' ? 'en' : settings.get<string>('general.language')) as Locale);
  let roots = $state<string[]>(settings.get<string[]>('discovery.roots') ?? []);

  function finish(then?: string) {
    settings.set('general.firstRunDone', true);
    settings.set('discovery.roots', roots);
    ui.firstRun = false;
    // Saving these settings starts the scan (the core rescans when discovery
    // settings change), so it sees the chosen roots and folders.
    if (then) void runCommand(then);
  }

  async function addRoot() {
    const p = await pickFolder(t('firstRun.addFolder'));
    if (p && !roots.includes(p)) roots = [...roots, p];
  }
</script>

{#if ui.firstRun}
  <div class="scrim" transition:fade={{ duration: 200 }}></div>
  <div
    class="fr card-surface"
    data-overlay
    role="dialog"
    aria-modal="true"
    aria-label={t('firstRun.welcome')}
    tabindex="-1"
    use:trapFocus={{ initial: 'footer .btn.primary' }}
    onkeydown={(e) => e.key === 'Escape' && (e.preventDefault(), finish())}
    transition:fly={{ y: 16, duration: 260 }}
  >
    <div class="dots">
      {#each Array(steps) as _, i (i)}<span class:on={i === step}></span>{/each}
    </div>
    {#key step}
      <div class="body" in:fly={{ x: 24, duration: 220 }}>
        {#if step === 0}
          <h2>{t('firstRun.welcome')}</h2>
          <p>{t('firstRun.welcomeText')}</p>
          <Segmented
            options={LOCALES.map((l) => ({ value: l.id, label: l.name }))}
            bind:value={lang}
            onchange={(v) => {
              settings.set('general.language', v);
              void setLocale(v as Locale);
            }}
          />
        {:else if step === 1}
          <h2>{t('firstRun.look')}</h2>
          <Segmented
            value={settings.get<string>('appearance.theme')}
            options={[
              { value: 'system', icon: Monitor, label: t('themes.system') },
              { value: 'light', icon: Sun, label: t('themes.light') },
              { value: 'dark', icon: Moon, label: t('themes.dark') },
            ]}
            onchange={(v) => settings.set('appearance.theme', v)}
          />
          <p class="small muted">{t('colors.primary')}</p>
          <div class="sw">
            {#each DEFAULT_SWATCHES.slice(0, 12) as c (c)}
              <button
                style:background={c}
                class:on={settings.get('appearance.primaryColor') === c}
                aria-label={c}
                onclick={() => settings.set('appearance.primaryColor', c)}
              ></button>
            {/each}
          </div>
        {:else if step === 2}
          <h2>{t('firstRun.keys')}</h2>
          <div class="choices">
            {#each PRESET_IDS as p (p)}
              <button class="choice" class:on={settings.get('keyboard.preset') === p} onclick={() => settings.set('keyboard.preset', p)}>
                <strong>{t(`presets.${p}.name`)}</strong>
                <span class="small muted">{t(`presets.${p}.desc`)}</span>
              </button>
            {/each}
          </div>
        {:else if step === 3}
          <h2>{t('firstRun.editor')}</h2>
          <div class="choices two">
            <button class="choice" class:on={settings.get('editor.openMode') === 'modal'} onclick={() => settings.set('editor.openMode', 'modal')}>
              <AppWindow size={20} /><strong>{t('firstRun.modal')}</strong>
            </button>
            <button class="choice" class:on={settings.get('editor.openMode') === 'sidebar'} onclick={() => settings.set('editor.openMode', 'sidebar')}>
              <PanelRight size={20} /><strong>{t('firstRun.sidebar')}</strong>
            </button>
          </div>
        {:else}
          <h2>{t('firstRun.boards')}</h2>
          <p>{t('firstRun.boardsText')}</p>
          <div class="roots">
            {#each roots as r (r)}<span class="chip">{r} <button class="icon-btn sm" onclick={() => (roots = roots.filter((x) => x !== r))}>×</button></span
              >{/each}
            <button class="btn sm" onclick={addRoot}>{t('firstRun.addFolder')}</button>
          </div>
          <label class="opt"
            ><Toggle checked={settings.get<boolean>('discovery.protectedFolders')} onchange={(v) => settings.set('discovery.protectedFolders', v)} />
            {t('settings.keys.discovery.protectedFolders.label')}</label
          >
          <label class="opt"
            ><Toggle checked={settings.get<boolean>('app.runInBackground')} onchange={(v) => settings.set('app.runInBackground', v)} />
            {t('settings.keys.app.runInBackground.label')}</label
          >
        {/if}
      </div>
    {/key}
    <footer>
      <button class="btn ghost" onclick={() => finish()}>{t('firstRun.skip')}</button>
      <span class="grow"></span>
      {#if step > 0}<button class="btn" onclick={() => step--}>{t('common.back')}</button>{/if}
      {#if step < steps - 1}
        <button class="btn primary" onclick={() => step++}>{t('firstRun.next')}</button>
      {:else}
        <button class="btn" onclick={() => finish('board.open')}>{t('boards.openFolder')}</button>
        <button class="btn primary" onclick={() => finish('board.new')}>{t('firstRun.createBoard')}</button>
      {/if}
    </footer>
  </div>
{/if}

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 1400;
    background: var(--bg-overlay);
    backdrop-filter: blur(6px);
  }
  .fr {
    position: fixed;
    z-index: 1401;
    top: 50%;
    left: 50%;
    translate: -50% -50%;
    width: min(560px, calc(100vw - 48px));
    padding: 28px;
    border-radius: var(--r-xl);
  }
  .dots {
    display: flex;
    gap: 6px;
    margin-bottom: 18px;
  }
  .dots span {
    width: 6px;
    height: 6px;
    border-radius: 3px;
    background: var(--line-strong);
    transition: width var(--dur) var(--ease);
  }
  .dots span.on {
    width: 20px;
    background: var(--primary);
  }
  .body {
    min-height: 230px;
    display: flex;
    flex-direction: column;
    gap: 14px;
    align-items: flex-start;
  }
  h2 {
    margin: 0;
    font-size: var(--fs-2xl);
    letter-spacing: -0.01em;
  }
  p {
    margin: 0;
    color: var(--ink-2);
    line-height: 1.55;
  }
  .small {
    font-size: var(--fs-sm);
  }
  .sw {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .sw button {
    width: 28px;
    height: 28px;
    border: none;
    border-radius: 50%;
  }
  .sw button.on {
    box-shadow:
      0 0 0 2px var(--bg-elev),
      0 0 0 4px var(--primary);
  }
  .choices {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
    width: 100%;
  }
  .choice {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 4px;
    padding: 14px;
    border: none;
    border-radius: var(--r-md);
    background: var(--bg-sunken);
    text-align: left;
  }
  .choice.on {
    background: var(--primary-soft);
    box-shadow: inset 0 0 0 1.5px var(--primary);
  }
  .roots {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .opt {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: var(--fs-sm);
  }
  footer {
    display: flex;
    gap: 8px;
    margin-top: 22px;
  }
  .grow {
    flex: 1;
  }
</style>
