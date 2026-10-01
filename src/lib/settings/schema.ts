// Settings schema: single source of truth for defaults, types and the Settings UI.
// Keys are flat dotted strings (VS Code style). Labels/descriptions are i18n keys
// derived from the key: `settings.keys.<key>.label` / `.desc`.

export type SettingType = 'boolean' | 'number' | 'string' | 'enum' | 'color' | 'font' | 'list' | 'paths';

export interface SettingDef {
  key: string;
  type: SettingType;
  default: unknown;
  category: SettingCategory;
  options?: string[];
  min?: number;
  max?: number;
  step?: number;
  /** Hidden from the UI (managed elsewhere). */
  hidden?: boolean;
  /** Requires restart. */
  restart?: boolean;
  /** Source: core or an extension id. */
  source?: string;
}

export type SettingCategory =
  | 'general'
  | 'appearance'
  | 'board'
  | 'editor'
  | 'tags'
  | 'files'
  | 'discovery'
  | 'search'
  | 'history'
  | 'ai'
  | 'integrations'
  | 'keyboard'
  | 'export'
  | 'advanced';

export const CATEGORIES: SettingCategory[] = [
  'general',
  'appearance',
  'board',
  'editor',
  'tags',
  'files',
  'discovery',
  'search',
  'history',
  'ai',
  'integrations',
  'keyboard',
  'export',
  'advanced',
];

const s = (key: string, type: SettingType, def: unknown, category: SettingCategory, extra: Partial<SettingDef> = {}): SettingDef => ({
  key,
  type,
  default: def,
  category,
  ...extra,
});

export const CORE_SETTINGS: SettingDef[] = [
  // general
  s('general.language', 'enum', 'auto', 'general', { options: ['auto', 'en', 'es', 'pt'] }),
  s('general.restoreTabs', 'boolean', true, 'general'),
  s('general.startPage', 'enum', 'start', 'general', { options: ['start', 'last', 'none'] }),
  s('general.confirmDelete', 'boolean', false, 'general'),
  s('app.runInBackground', 'boolean', false, 'general', { restart: false }),
  s('app.launchAtLogin', 'boolean', false, 'general'),
  s('updates.checkOnStart', 'boolean', false, 'general'),
  s('general.firstRunDone', 'boolean', false, 'general', { hidden: true }),

  // appearance
  s('appearance.theme', 'enum', 'system', 'appearance', { options: ['system', 'light', 'dark'] }),
  s('appearance.primaryColor', 'color', '#ef8a7c', 'appearance'),
  s('appearance.secondaryColor', 'color', '#fdeadc', 'appearance'),
  s('appearance.uiFont', 'font', 'default', 'appearance'),
  s('appearance.editorFont', 'font', 'default', 'appearance'),
  s('appearance.monoFont', 'font', 'default', 'appearance'),
  s('appearance.fontSize', 'number', 13.5, 'appearance', { min: 11, max: 18, step: 0.5 }),
  s('appearance.transparency', 'enum', 'full', 'appearance', { options: ['full', 'reduced'] }),
  s('appearance.motion', 'enum', 'full', 'appearance', { options: ['full', 'reduced', 'none'] }),
  s('appearance.density', 'enum', 'comfortable', 'appearance', { options: ['comfortable', 'compact'] }),
  s('appearance.zoom', 'number', 1, 'appearance', { min: 0.6, max: 1.8, step: 0.05 }),

  // board
  s('board.defaultOrientation', 'enum', 'columns', 'board', { options: ['columns', 'rows'] }),
  s('board.defaultSpacing', 'enum', 'fixedMain', 'board', { options: ['fixedMain', 'fixedCross'] }),
  s('board.laneWidth', 'number', 300, 'board', { min: 220, max: 560, step: 10 }),
  s('board.cardWidth', 'number', 280, 'board', { min: 200, max: 480, step: 10 }),
  s('board.face.preview', 'boolean', true, 'board'),
  s('board.face.summary', 'boolean', true, 'board'),
  s('board.face.maxItems', 'number', 5, 'board', { min: 1, max: 8, step: 1 }),
  s('board.face.tags', 'boolean', true, 'board'),
  s('board.face.indicators', 'boolean', true, 'board'),
  s('board.newCardPosition', 'enum', 'bottom', 'board', { options: ['bottom', 'top'] }),
  s('board.showArchived', 'boolean', false, 'board'),
  s('board.singleKeyShortcuts', 'boolean', true, 'board'),
  s('board.templates', 'list', [], 'board', { hidden: true }),

  // editor
  s('editor.openMode', 'enum', 'modal', 'editor', { options: ['modal', 'sidebar'] }),
  s('editor.livePreview', 'boolean', true, 'editor'),
  s('editor.width', 'number', 720, 'editor', { min: 480, max: 1400, step: 20 }),
  s('editor.fullWidth', 'boolean', false, 'editor'),
  s('editor.fontSize', 'number', 15, 'editor', { min: 12, max: 22, step: 0.5 }),
  s('editor.lineHeight', 'number', 1.65, 'editor', { min: 1.2, max: 2.2, step: 0.05 }),
  s('editor.spellcheck', 'boolean', false, 'editor'),
  s('editor.vim', 'boolean', false, 'editor'),
  s('editor.lineNumbers', 'boolean', false, 'editor'),
  s('editor.autosaveDelay', 'number', 300, 'editor', { min: 100, max: 3000, step: 50 }),
  s('editor.tabSize', 'number', 2, 'editor', { min: 2, max: 8, step: 1 }),
  s('editor.autoPair', 'boolean', true, 'editor'),
  s('editor.pasteRich', 'boolean', true, 'editor'),
  s('editor.sidebarPinned', 'boolean', false, 'editor', { hidden: true }),
  s('editor.sidebarWidth', 'number', 520, 'editor', { hidden: true }),
  s('editor.python', 'string', '', 'editor'),
  s('editor.codeTimeout', 'number', 30, 'editor', { min: 5, max: 600, step: 5 }),

  // tags & links
  s('tags.autoColor', 'boolean', true, 'tags'),
  s('tags.colors', 'list', {}, 'tags', { hidden: true }),
  s('links.showBacklinks', 'boolean', true, 'tags'),
  s('links.chipStyle', 'enum', 'soft', 'tags', { options: ['soft', 'underline'] }),

  // files
  s('files.autoCleanup', 'boolean', true, 'files'),
  s('files.unlinkedTtlDays', 'number', 7, 'files', { min: 1, max: 365, step: 1 }),
  s('trash.ttlDays', 'number', 7, 'files', { min: 1, max: 365, step: 1 }),
  s('files.gitInit', 'boolean', true, 'files'),
  s('files.remoteImages', 'boolean', true, 'files'),

  // discovery
  s('discovery.roots', 'paths', [], 'discovery'),
  s('discovery.exclude', 'list', [], 'discovery'),
  s('discovery.rescanMinutes', 'number', 30, 'discovery', { min: 0, max: 1440, step: 5 }),
  s('explorer.showHidden', 'boolean', false, 'discovery'),

  // search
  s('search.caseSensitive', 'boolean', false, 'search'),
  s('search.includeArchived', 'boolean', false, 'search'),

  // history
  s('history.retentionDays', 'number', 180, 'history', { min: 7, max: 3650, step: 1 }),
  s('history.maxMb', 'number', 50, 'history', { min: 5, max: 2000, step: 5 }),

  // ai
  s('ai.provider', 'enum', 'auto', 'ai', { options: ['auto', 'ollama', 'openai', 'off'] }),
  s('ai.endpoint', 'string', 'http://localhost:11434', 'ai'),
  s('ai.model', 'string', '', 'ai'),
  s('ai.temperature', 'number', 0.3, 'ai', { min: 0, max: 1.5, step: 0.05 }),
  s('summaries.schedules', 'list', [], 'ai', { hidden: true }),

  // integrations
  s('integrations.confirmPush', 'boolean', true, 'integrations'),
  s('integrations.confirmPull', 'boolean', false, 'integrations'),
  s('integrations.watchIntervalSec', 'number', 60, 'integrations', { min: 30, max: 3600, step: 30 }),
  s('integrations.allowInsecure', 'boolean', false, 'integrations', { hidden: true }),

  // keyboard
  s('keyboard.preset', 'enum', 'vscode', 'keyboard', { options: ['vscode', 'trello', 'vim', 'emacs'] }),

  // export
  s('export.defaultFormat', 'enum', 'md', 'export', { options: ['md', 'html', 'pdf'] }),
  s('export.includeHistory', 'boolean', false, 'export'),

  // advanced
  s('advanced.showIds', 'boolean', false, 'advanced'),
  s('advanced.devtools', 'boolean', false, 'advanced'),
];
