// Import / export helpers shared by io.commands.ts and the browser mock.
// Pure functions only (unit-tested in io.test.ts).

export type ExportFormat = 'html' | 'pdf' | 'md' | 'mdBundle' | 'zip' | 'json';
export type ExportTarget = 'board' | 'card';
export type ImportMode = 'copy' | 'overwrite' | 'inPlace';
export type SourceKind = 'board' | 'boardZip' | 'zip' | 'folder' | 'interchange';

export interface ExportResult {
  path: string;
  files: number;
  bytes: number;
}

export interface Inspect {
  kind: SourceKind;
  name: string;
  boardId: string | null;
  registered: boolean;
  schema: number | null;
  boardKind: 'kanban' | 'files' | null;
  lanes: number;
  notes: number;
  truncated: boolean;
}

export interface ImportResult {
  boardId: string;
  cards: number;
  lanes: number;
  attachments: number;
  warnings: string[];
}

/** Formats offered per target, in menu order. */
export function exportFormats(target: ExportTarget): ExportFormat[] {
  return target === 'board' ? ['html', 'pdf', 'md', 'mdBundle', 'zip', 'json'] : ['html', 'pdf', 'md', 'json'];
}

const EXT: Record<ExportFormat, string> = { html: 'html', pdf: 'pdf', md: 'md', mdBundle: 'zip', zip: 'zip', json: 'json' };

export function extensionOf(format: ExportFormat): string {
  return EXT[format];
}

/** Portable file name for an export (no path separators or reserved characters). */
export function exportFileName(title: string, format: ExportFormat): string {
  let stem = title
    .replace(/[\\/:*?"<>|#[\]\u0000-\u001f]/g, '-')
    .replace(/\s+/g, ' ')
    .trim()
    .replace(/^\.+|\.+$/g, '')
    .slice(0, 80)
    .trim();
  if (!stem) stem = 'Untitled';
  if (format === 'mdBundle') stem += ' (Markdown)';
  return `${stem}.${EXT[format]}`;
}

/** Import modes available for a source, default first. */
export function importModes(info: Pick<Inspect, 'kind'>, hasBoards: boolean): ImportMode[] {
  const modes: ImportMode[] = ['copy'];
  if (info.kind === 'board' || info.kind === 'folder') modes.push('inPlace');
  if (hasBoards) modes.push('overwrite');
  return modes;
}

/** True when a read-only reason is a newer on-disk schema (offers "Convert"). */
export function isNewerSchema(readOnly: string | null | undefined): boolean {
  return !!readOnly && readOnly.startsWith('newer_schema');
}

export function isLoose(readOnly: string | null | undefined): boolean {
  return readOnly === 'loose';
}

// ── Settings bundle ─────────────────────────────────────────────────────────

export const SETTINGS_FORMAT = 'lull-settings';

/** Keys that never leave the machine (secrets live in the OS keychain anyway). */
export function isSecretKey(k: string): boolean {
  return /token|secret|password|apikey|api_key|credential|privatekey/i.test(k);
}

export function buildSettingsBundle(settings: Record<string, unknown>, keybindings: unknown[], explorer?: unknown) {
  return {
    format: SETTINGS_FORMAT,
    version: 1,
    settings: Object.fromEntries(Object.entries(settings).filter(([k]) => !isSecretKey(k))),
    keybindings,
    ...(explorer ? { explorer } : {}),
  };
}

// ── Board templates ─────────────────────────────────────────────────────────

export interface TemplateSpec {
  kind: 'kanban' | 'files';
  lanes: { name: string; cards: string[] }[];
  notes: string[];
}

export const NEW_BOARD_TEMPLATES = ['kanbanBasic', 'sprint', 'personal', 'notes'] as const;
export type NewBoardTemplate = (typeof NEW_BOARD_TEMPLATES)[number];

/** Template content; `tr(key)` returns localized text under `io.tpl.<id>.*`. */
export function templateSpec(id: NewBoardTemplate, tr: (key: string) => string): TemplateSpec {
  const k = (s: string) => tr(`io.tpl.${id}.${s}`);
  const card = (title: string, body = '') => `# ${title}\n${body ? `\n${body}\n` : ''}`;
  switch (id) {
    case 'kanbanBasic':
      return {
        kind: 'kanban',
        lanes: [
          { name: k('todo'), cards: [card(k('welcome'), k('welcomeBody'))] },
          { name: k('doing'), cards: [] },
          { name: k('done'), cards: [] },
        ],
        notes: [],
      };
    case 'sprint':
      return {
        kind: 'kanban',
        lanes: [
          { name: k('backlog'), cards: [card(k('goal'), `${k('goalBody')}\n\n#sprint`)] },
          { name: k('sprint'), cards: [] },
          { name: k('inProgress'), cards: [] },
          { name: k('review'), cards: [] },
          { name: k('done'), cards: [card(k('retro'), `## ${k('wentWell')}\n\n- \n\n## ${k('improve')}\n\n- [ ] `)] },
        ],
        notes: [],
      };
    case 'personal':
      return {
        kind: 'kanban',
        lanes: [
          { name: k('inbox'), cards: [card(k('capture'), k('captureBody'))] },
          { name: k('today'), cards: [] },
          { name: k('thisWeek'), cards: [] },
          { name: k('someday'), cards: [] },
          { name: k('done'), cards: [] },
        ],
        notes: [],
      };
    case 'notes':
      return { kind: 'files', lanes: [], notes: [card(k('welcome'), k('welcomeBody')), card(k('ideas'), '- ')] };
  }
}

/** `n` B / KB / MB for result toasts. */
export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}
