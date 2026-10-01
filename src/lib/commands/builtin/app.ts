import { Palette, Keyboard, Settings, Type, Sun, Moon, Monitor, Languages, Sparkles, Download, Upload, RefreshCw, Bug, Wand2, Contrast } from '@lucide/svelte';
import type { Command } from '../registry.svelte';
import { rpc, isTauri } from '$lib/backend/rpc';
import { settings } from '$lib/settings/store.svelte';
import { quickPick, inputBox, pickOne, BACK, type QuickItem } from '$lib/quickinput/qi.svelte';
import { openPalette } from '$lib/quickinput/palette';
import { openSingleton } from '$lib/state/workspace.svelte';
import { ui } from '$lib/state/ui.svelte';
import { saveUserKeybindings, kb } from '$lib/keybindings/resolver.svelte';
import { PRESET_IDS } from '$lib/keybindings/defaults';
import { pickColor } from '$lib/state/colorDialog.svelte';
import { confirm } from '$lib/state/dialogs.svelte';
import { toast } from '$lib/state/toasts.svelte';
import { setLocale, LOCALES, detectLocale, t, type Locale } from '$lib/i18n/index.svelte';
import { pickSavePath, reveal } from '$lib/app/helpers';
import { app } from '$lib/app/bootstrap';
import { registry } from '$lib/state/registry.svelte';

async function toggleSetting(key: string, onKey: string, offKey: string) {
  settings.toggle(key);
  toast.info(t(settings.get<boolean>(key) ? onKey : offKey));
}

export const appCommands: Command[] = [
  { id: 'palette.commands', title: 'commands.palette.commands', category: 'view', hidden: true, run: () => openPalette('>') },
  { id: 'palette.quickOpen', title: 'commands.palette.quickOpen', category: 'view', hidden: true, run: () => openPalette('') },
  { id: 'app.openSettings', title: 'commands.app.openSettings', category: 'preferences', icon: Settings, run: () => openSingleton('settings') },
  { id: 'app.openKeybindings', title: 'commands.app.openKeybindings', category: 'preferences', icon: Keyboard, run: () => openSingleton('keybindings') },
  {
    id: 'app.openSettingsJson',
    title: 'commands.app.openSettingsJson',
    category: 'preferences',
    run: async () => {
      if (!app.info) return;
      await rpc('config.open', { file: 'settings' }).catch(() => reveal(`${app.info!.configDir}/settings.json`));
    },
  },
  { id: 'app.cheatsheet', title: 'commands.app.cheatsheet', category: 'help', icon: Keyboard, run: () => (ui.cheatsheet = !ui.cheatsheet) },
  { id: 'app.showWelcome', title: 'commands.app.showWelcome', category: 'help', run: () => openSingleton('start') },
  { id: 'app.firstRun', title: 'commands.app.firstRun', category: 'help', icon: Wand2, run: () => (ui.firstRun = true) },
  { id: 'app.reload', title: 'commands.app.reload', category: 'developer', icon: RefreshCw, run: () => location.reload() },
  {
    id: 'app.openLogs',
    title: 'commands.app.openLogs',
    category: 'help',
    icon: Bug,
    run: () => app.info && reveal(app.info.logsDir),
  },
  {
    id: 'app.exportDiagnostics',
    title: 'commands.app.exportDiagnostics',
    category: 'help',
    icon: Bug,
    run: async () => {
      const dest = await pickSavePath(t('commands.app.exportDiagnostics'), `luau-diagnostics-${new Date().toISOString().slice(0, 10)}.zip`, [{ name: 'Zip', extensions: ['zip'] }]);
      if (!dest) return;
      await rpc('logs.export', { dest });
      toast.success(t('toasts.exported'), { action: { label: t('common.reveal'), run: () => reveal(dest) } });
    },
  },
  {
    id: 'app.checkForUpdates',
    title: 'commands.app.checkForUpdates',
    category: 'help',
    icon: Download,
    run: async (args?: { silent?: boolean }) => {
      if (!isTauri) return;
      try {
        const { check } = await import('@tauri-apps/plugin-updater');
        const update = await check();
        if (!update) {
          if (!args?.silent) toast.success(t('updates.upToDate'));
          return;
        }
        const ok = await confirm({ title: t('updates.available', { version: update.version }), message: update.body ?? '', confirmLabel: t('updates.install'), cancelFocused: false });
        if (!ok) return;
        toast.info(t('updates.downloading'));
        await update.downloadAndInstall();
        const { relaunch } = await import('@tauri-apps/plugin-process');
        await relaunch();
      } catch (e) {
        if (!args?.silent) toast.warn(t('updates.failed', { message: String((e as Error)?.message ?? e) }));
      }
    },
  },
  {
    id: 'app.pickTheme',
    title: 'commands.app.pickTheme',
    category: 'preferences',
    icon: Sun,
    run: async () => {
      const cur = settings.get<string>('appearance.theme');
      const items: QuickItem<string>[] = [
        { label: t('themes.system'), value: 'system', icon: Monitor, picked: cur === 'system' },
        { label: t('themes.light'), value: 'light', icon: Sun, picked: cur === 'light' },
        { label: t('themes.dark'), value: 'dark', icon: Moon, picked: cur === 'dark' },
      ];
      const v = await pickOne(items, {
        title: t('commands.app.pickTheme'),
        activeIndex: Math.max(0, items.findIndex((i) => i.value === cur)),
        onActive: (it) => it?.value && settings.set('appearance.theme', it.value),
      });
      settings.set('appearance.theme', typeof v === 'string' ? v : cur);
    },
  },
  {
    id: 'app.toggleTransparency',
    title: 'commands.app.toggleTransparency',
    category: 'preferences',
    icon: Contrast,
    run: () => {
      const reduced = settings.get('appearance.transparency') === 'reduced';
      settings.set('appearance.transparency', reduced ? 'full' : 'reduced');
      toast.info(t(reduced ? 'toasts.transparencyOn' : 'toasts.transparencyOff'));
    },
  },
  {
    id: 'app.reduceMotion',
    title: 'commands.app.reduceMotion',
    category: 'preferences',
    run: async () => {
      const cur = settings.get<string>('appearance.motion');
      const v = await pickOne(
        (['none', 'reduced', 'full'] as const).map((m) => ({ label: t(`motion.${m}`), value: m, picked: cur === m })),
        { title: t('commands.app.reduceMotion'), activeIndex: ['none', 'reduced', 'full'].indexOf(cur) },
      );
      if (typeof v === 'string') settings.set('appearance.motion', v);
    },
  },
  {
    id: 'app.changeFonts',
    title: 'commands.app.changeFonts',
    category: 'preferences',
    icon: Type,
    run: async () => {
      const target = await pickOne(
        [
          { label: t('fonts.ui'), value: 'appearance.uiFont' },
          { label: t('fonts.editor'), value: 'appearance.editorFont' },
          { label: t('fonts.mono'), value: 'appearance.monoFont' },
        ],
        { title: t('commands.app.changeFonts'), step: 1, totalSteps: 2 },
      );
      if (typeof target !== 'string') return;
      const mode = await pickOne(
        [
          { label: t('fonts.default'), value: 'default', description: target === 'appearance.monoFont' ? 'JetBrains Mono' : 'Inter' },
          { label: t('fonts.system'), value: 'system' },
          { label: t('fonts.pick'), value: 'pick' },
        ],
        { title: t('commands.app.changeFonts'), step: 2, totalSteps: 2 },
      );
      if (mode === BACK || mode === undefined) return;
      if (mode !== 'pick') return settings.set(target, mode);
      const fonts = await rpc<string[]>('fonts.list');
      const prev = settings.get<string>(target);
      const family = await pickOne(
        fonts.map((f) => ({ label: f, value: f })),
        { title: t('fonts.pick'), placeholder: t('fonts.search'), onActive: (it) => it?.value && settings.set(target, it.value) },
      );
      settings.set(target, typeof family === 'string' ? family : prev);
    },
  },
  {
    id: 'app.colors',
    title: 'commands.app.colors',
    category: 'preferences',
    icon: Palette,
    run: async () => {
      const which = await pickOne(
        [
          { label: t('colors.primary'), value: 'appearance.primaryColor', description: settings.get<string>('appearance.primaryColor') },
          { label: t('colors.secondary'), value: 'appearance.secondaryColor', description: settings.get<string>('appearance.secondaryColor') },
          { label: t('colors.reset'), value: 'reset' },
        ],
        { title: t('commands.app.colors') },
      );
      if (which === 'reset') {
        settings.reset('appearance.primaryColor');
        settings.reset('appearance.secondaryColor');
        return;
      }
      if (typeof which !== 'string') return;
      const prev = settings.get<string>(which);
      const hex = await pickColor(which.endsWith('primaryColor') ? t('colors.primary') : t('colors.secondary'), prev, (v) => settings.set(which, v));
      settings.set(which, hex ?? prev);
    },
  },
  {
    id: 'app.pickKeybindingPreset',
    title: 'commands.app.pickKeybindingPreset',
    category: 'preferences',
    icon: Keyboard,
    run: async () => {
      const cur = settings.get<string>('keyboard.preset');
      const preset = await pickOne(
        PRESET_IDS.map((p) => ({ label: t(`presets.${p}.name`), description: t(`presets.${p}.desc`), value: p, picked: p === cur })),
        { title: t('commands.app.pickKeybindingPreset'), step: 1, totalSteps: 2 },
      );
      if (typeof preset !== 'string') return;
      if (kb.user.length) {
        const override = await pickOne(
          [
            { label: t('presets.keepCustom'), value: 'no' },
            { label: t('presets.overrideCustom', { count: kb.user.length }), value: 'yes' },
          ],
          { title: t('presets.overrideTitle'), step: 2, totalSteps: 2 },
        );
        if (override === undefined || override === BACK) return;
        if (override === 'yes') await saveUserKeybindings([]);
      }
      settings.set('keyboard.preset', preset);
      toast.success(t('presets.applied', { name: t(`presets.${preset}.name`) }));
    },
  },
  {
    id: 'app.toggleSingleKeys',
    title: 'commands.app.toggleSingleKeys',
    category: 'preferences',
    icon: Keyboard,
    run: () => toggleSetting('board.singleKeyShortcuts', 'toasts.singleKeysOn', 'toasts.singleKeysOff'),
  },
  { id: 'app.toggleConfirmPush', title: 'commands.app.toggleConfirmPush', category: 'integrations', run: () => toggleSetting('integrations.confirmPush', 'toasts.confirmPushOn', 'toasts.confirmPushOff') },
  { id: 'app.toggleConfirmPull', title: 'commands.app.toggleConfirmPull', category: 'integrations', run: () => toggleSetting('integrations.confirmPull', 'toasts.confirmPullOn', 'toasts.confirmPullOff') },
  {
    id: 'app.toggleHiddenBoards',
    title: 'commands.app.toggleHiddenBoards',
    category: 'explorer',
    run: () => toggleSetting('explorer.showHidden', 'toasts.hiddenShown', 'toasts.hiddenHidden'),
  },
  {
    id: 'app.language',
    title: 'commands.app.language',
    category: 'preferences',
    icon: Languages,
    run: async () => {
      const cur = settings.get<string>('general.language');
      const v = await pickOne(
        [{ label: t('languages.auto'), value: 'auto', picked: cur === 'auto' }, ...LOCALES.map((l) => ({ label: l.name, value: l.id, picked: cur === l.id }))],
        { title: t('commands.app.language') },
      );
      if (typeof v !== 'string') return;
      settings.set('general.language', v);
      await setLocale(v === 'auto' ? detectLocale() : (v as Locale));
    },
  },
  {
    id: 'app.exportSettings',
    title: 'commands.app.exportSettings',
    category: 'preferences',
    icon: Upload,
    // Implemented in io.commands.ts (validated bundle, secrets stripped).
    run: async () => (await import('../registry.svelte')).runCommand('settings.export'),
  },
  {
    id: 'app.importSettings',
    title: 'commands.app.importSettings',
    category: 'preferences',
    icon: Download,
    run: async () => (await import('../registry.svelte')).runCommand('settings.import'),
  },
  {
    id: 'app.changeEditorWidth',
    title: 'commands.app.changeEditorWidth',
    category: 'editor',
    run: async () => {
      const v = await inputBox({
        title: t('commands.app.changeEditorWidth'),
        value: String(settings.get<number>('editor.width')),
        prompt: t('settings.keys.editor.width.desc'),
        validate: (s) => (/^\d{3,4}$/.test(s) && +s >= 480 && +s <= 1400 ? null : t('validation.between', { min: 480, max: 1400 })),
      });
      if (typeof v === 'string') {
        settings.set('editor.width', Number(v));
        settings.set('editor.fullWidth', false);
      }
    },
  },
  {
    id: 'app.toggleSpellcheck',
    title: 'commands.app.toggleSpellcheck',
    category: 'editor',
    run: () => toggleSetting('editor.spellcheck', 'toasts.spellcheckOn', 'toasts.spellcheckOff'),
  },
  { id: 'app.toggleVim', title: 'commands.app.toggleVim', category: 'editor', run: () => toggleSetting('editor.vim', 'toasts.vimOn', 'toasts.vimOff') },
  {
    id: 'app.toggleEditorMode',
    title: 'commands.app.toggleEditorMode',
    category: 'editor',
    run: () => settings.set('editor.openMode', settings.get('editor.openMode') === 'modal' ? 'sidebar' : 'modal'),
  },
  {
    id: 'app.launchAtLogin',
    title: 'commands.app.launchAtLogin',
    category: 'preferences',
    run: async () => {
      if (!isTauri) return;
      const al = await import('@tauri-apps/plugin-autostart');
      if (await al.isEnabled()) await al.disable();
      else await al.enable();
      const on = await al.isEnabled();
      settings.set('app.launchAtLogin', on);
      toast.info(t(on ? 'toasts.autostartOn' : 'toasts.autostartOff'));
    },
  },
  { id: 'app.aiSettings', title: 'commands.app.aiSettings', category: 'ai', icon: Sparkles, run: () => openSingleton('settings', { query: '@ai' }) },
];

export { quickPick };
