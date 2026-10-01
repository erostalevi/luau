// Import / export, board templates, schema upgrade and settings bundles (SPEC §15, §16).

import { Download, Upload, FileDown, FolderInput, LayoutTemplate, ArrowUpCircle, FileCode2, FileText, FileArchive, Printer, Braces, Package, Copy, FolderOpen, Replace, Wrench } from '@lucide/svelte';
import type { Component } from 'svelte';
import type { Command } from '$lib/commands/registry.svelte';
import { rpc, isTauri } from '$lib/backend/rpc';
import type { BoardSnapshot } from '$lib/backend/types';
import { pickOne, inputBox, steps, BACK, type QuickItem } from '$lib/quickinput/qi.svelte';
import { confirm } from '$lib/state/dialogs.svelte';
import { toast } from '$lib/state/toasts.svelte';
import { boards, openBoard } from '$lib/state/boards.svelte';
import { openBoardTab } from '$lib/state/workspace.svelte';
import { registry } from '$lib/state/registry.svelte';
import { settings } from '$lib/settings/store.svelte';
import { kb, saveUserKeybindings } from '$lib/keybindings/resolver.svelte';
import { activeCard, tabBoard, activeBoard, pickFolder, pickFile, pickSavePath, reveal, joinPath } from '$lib/app/helpers';
import { t } from '$lib/i18n/index.svelte';
import {
  exportFormats,
  exportFileName,
  extensionOf,
  importModes,
  isNewerSchema,
  buildSettingsBundle,
  templateSpec,
  formatBytes,
  NEW_BOARD_TEMPLATES,
  type ExportFormat,
  type ExportTarget,
  type ExportResult,
  type Inspect,
  type ImportMode,
  type ImportResult,
  type NewBoardTemplate,
} from './io';

const FORMAT_ICONS: Record<ExportFormat, Component<any>> = { html: FileCode2, pdf: Printer, md: FileText, mdBundle: Package, zip: FileArchive, json: Braces };

function errMessage(e: unknown): string {
  return (e as Error)?.message ?? String(e);
}

/** Print HTML through the system print dialog (PDF = "Save as PDF"). */
export function printHtml(html: string): Promise<void> {
  return new Promise((resolve) => {
    const frame = document.createElement('iframe');
    frame.setAttribute('aria-hidden', 'true');
    frame.setAttribute('sandbox', 'allow-modals allow-same-origin');
    frame.style.cssText = 'position:fixed;right:0;bottom:0;width:0;height:0;border:0;visibility:hidden';
    frame.onload = () => {
      const w = frame.contentWindow;
      const done = () => {
        setTimeout(() => frame.remove(), 500);
        resolve();
      };
      if (!w) return done();
      w.addEventListener('afterprint', done, { once: true });
      w.focus();
      w.print();
      // Some webviews never fire `afterprint`.
      setTimeout(done, 60_000);
    };
    frame.srcdoc = html;
    document.body.appendChild(frame);
  });
}

async function runExport(target: ExportTarget, boardId: string, title: string, cardId?: string) {
  const preferred = settings.get<string>('export.defaultFormat');
  const formats = exportFormats(target).sort((a, b) => Number(b === preferred) - Number(a === preferred));
  const fmt = await pickOne<ExportFormat>(
    formats.map((f) => ({ label: t(`io.formats.${f}.label`), description: t(`io.formats.${f}.desc`), icon: FORMAT_ICONS[f], value: f })),
    { title: t(target === 'board' ? 'io.export.boardTitle' : 'io.export.cardTitle', { name: title }), placeholder: t('io.export.pickFormat') },
  );
  if (!fmt || fmt === BACK) return;
  try {
    if (fmt === 'pdf') {
      const html = await rpc<string>('io.renderHtml', { board: boardId, card: cardId ?? null });
      toast.info(t('io.export.pdfHint'));
      await printHtml(html);
      return;
    }
    const name = exportFileName(title, fmt);
    const dest = isTauri ? await pickSavePath(t('io.export.saveTitle'), name, [{ name: t(`io.formats.${fmt}.label`), extensions: [extensionOf(fmt)] }]) : name;
    if (!dest) return;
    const includeHistory = fmt === 'zip' ? settings.get<boolean>('export.includeHistory') === true : false;
    const r = await rpc<ExportResult>('io.export', { board: boardId, card: cardId ?? null, format: fmt, dest, includeHistory });
    toast.success(t('io.export.done', { size: formatBytes(r.bytes) }), isTauri ? { action: { label: t('common.reveal'), run: () => reveal(r.path) } } : undefined);
  } catch (e) {
    toast.error(t('io.export.failed', { message: errMessage(e) }));
  }
}

async function exportBoard() {
  const b = tabBoard() ?? activeBoard();
  if (!b) return toast.info(t('io.noBoard'));
  await runExport('board', b.id, b.header.name);
}

async function exportCard(filesOnly: boolean) {
  const c = activeCard();
  if (!c || (filesOnly && c.board.header.kind !== 'files')) return toast.info(t(filesOnly ? 'io.noFile' : 'io.noCard'));
  const title = c.board.node(c.id)?.title || t('common.untitled');
  await runExport('card', c.board.id, title, c.id);
}

// ── Import ──────────────────────────────────────────────────────────────────

async function pickSource(): Promise<string | null> {
  if (!isTauri) return pickFolder(t('io.import.pickFolder'));
  const what = await pickOne(
    [
      { label: t('io.import.folder'), description: t('io.import.folderDesc'), icon: FolderOpen, value: 'folder' },
      { label: t('io.import.file'), description: t('io.import.fileDesc'), icon: FileArchive, value: 'file' },
    ],
    { title: t('io.import.title') },
  );
  if (what === 'folder') return pickFolder(t('io.import.pickFolder'));
  if (what === 'file') return (await pickFile(t('io.import.pickFile'), [{ name: t('io.import.fileFilter'), extensions: ['zip', 'json'] }]))?.[0] ?? null;
  return null;
}

const MODE_ICONS: Record<ImportMode, Component<any>> = { copy: Copy, inPlace: FolderOpen, overwrite: Replace };

function modeItem(m: ImportMode, info: Inspect): QuickItem<ImportMode> {
  const luau = info.kind === 'board';
  const key = m === 'inPlace' ? (luau ? 'inPlaceBoard' : 'inPlaceFolder') : m;
  return { label: t(`io.modes.${key}.label`), description: t(`io.modes.${key}.desc`), icon: MODE_ICONS[m], value: m, picked: m === 'copy' };
}

async function importFlow(args?: { path?: string }) {
  const path = args?.path ?? (await pickSource());
  if (!path) return;
  let info: Inspect;
  try {
    info = await rpc<Inspect>('io.inspect', { path });
  } catch (e) {
    return toast.error(t('io.import.failed', { message: errMessage(e) }));
  }
  const targets = registry.data.boards.filter((b) => !b.mirror && !b.missing && !b.loose);
  const summary = t('io.import.summary', { name: info.name, cards: info.notes, lanes: info.lanes });
  const res = await steps<[ImportMode, string]>([
    () => pickOne(importModes(info, targets.length > 0).map((m) => modeItem(m, info)), { title: summary, step: 1, totalSteps: 2 }),
    async ([mode]) => {
      if (mode === 'inPlace') return '';
      if (mode === 'overwrite') {
        return pickOne(
          targets.map((b) => ({ label: b.name, description: b.path, value: b.id })),
          { title: t('io.import.pickTarget'), step: 2, totalSteps: 2 },
        );
      }
      const parent = await pickFolder(t('io.import.pickLocation'));
      if (!parent) return BACK;
      const name = await inputBox({ title: t('boards.name'), value: info.name, step: 2, totalSteps: 2, validate: (v) => (v.trim() && !/[\\/]/.test(v) ? null : t('validation.required')) });
      if (typeof name !== 'string') return name;
      return JSON.stringify({ dest: joinPath(parent, name.trim()), name: name.trim() });
    },
  ]);
  if (!res) return;
  const [mode, extra] = res;
  const req: Record<string, unknown> = { path, mode, inboxLane: t('templates.lanes.inbox') };
  if (mode === 'copy') Object.assign(req, JSON.parse(extra));
  if (mode === 'overwrite') {
    const target = targets.find((b) => b.id === extra);
    const ok = await confirm({
      title: t('io.import.overwriteTitle', { name: target?.name ?? '' }),
      message: t('io.import.overwriteMessage'),
      confirmLabel: t('io.import.overwriteConfirm'),
      danger: true,
      cancelFocused: true,
    });
    if (!ok) return;
    req.target = extra;
  }
  try {
    const r = await rpc<ImportResult>('io.import', req);
    await openBoard({ id: r.boardId });
    await openBoardTab(r.boardId);
    if (mode === 'inPlace') toast.success(t(info.kind === 'board' ? 'io.import.opened' : 'io.import.openedAsIs', { name: info.name }));
    else toast.success(t('io.import.done', { cards: r.cards, files: r.attachments }));
    if (r.warnings.length) toast.warn(t('io.import.warnings', { count: r.warnings.length }));
  } catch (e) {
    toast.error(t('io.import.failed', { message: errMessage(e) }));
  }
}

// ── Templates ───────────────────────────────────────────────────────────────

async function newFromTemplate() {
  const res = await steps<[NewBoardTemplate, string, string]>([
    () =>
      pickOne(
        NEW_BOARD_TEMPLATES.map((id) => ({ label: t(`io.tpl.${id}.name`), description: t(`io.tpl.${id}.desc`), value: id })),
        { title: t('io.tpl.pick'), step: 1, totalSteps: 3 },
      ),
    async () => (await pickFolder(t('boards.pickLocation'))) ?? BACK,
    ([id]) => inputBox({ title: t('boards.name'), value: t(`io.tpl.${id}.name`), step: 3, totalSteps: 3, prompt: t('boards.namePrompt'), validate: (v) => (v.trim() && !/[\\/]/.test(v) ? null : t('validation.required')) }),
  ]);
  if (!res) return;
  const [id, folder, name] = res;
  try {
    const snap = await rpc<BoardSnapshot>('board.createFromTemplate', {
      path: joinPath(folder, name.trim()),
      name: name.trim(),
      template: templateSpec(id, (k) => t(k)),
      git: settings.get<boolean>('files.gitInit') === true,
    });
    await openBoardTab(snap.header.id);
    toast.success(t('boards.created', { name: snap.header.name }));
  } catch (e) {
    const msg = errMessage(e);
    toast.error(msg.includes('nested_board') ? t('errors.nestedBoard') : t('errors.createBoard', { message: msg }));
  }
}

// ── Schema upgrade ──────────────────────────────────────────────────────────

async function upgradeSchema() {
  const b = tabBoard();
  if (!b) return toast.info(t('io.noBoard'));
  if (!isNewerSchema(b.header.readOnly)) return toast.info(t('io.upgrade.current'));
  const ok = await confirm({ title: t('board.upgradeTitle'), message: `${t('board.upgradeMessage')}\n\n${t('io.upgrade.backupHint')}`, confirmLabel: t('board.upgradeConfirm'), danger: true, cancelFocused: true });
  if (!ok) return;
  try {
    const r = await rpc<{ report: { from: number; to: number; files: number }; snapshot: BoardSnapshot }>('board.upgrade', { board: b.id });
    boards.get(b.id)?.load(r.snapshot);
    await rpc('board.claim', { board: b.id }).catch(() => null);
    toast.success(t('io.upgrade.done', { from: r.report.from, to: r.report.to }));
  } catch (e) {
    toast.error(t('io.upgrade.failed', { message: errMessage(e) }));
  }
}

async function repairBoard() {
  const b = tabBoard();
  if (!b) return toast.info(t('io.noBoard'));
  if (b.header.readOnly !== 'corrupt_manifest') return toast.info(t('io.repair.notNeeded'));
  const ok = await confirm({ title: t('io.repair.title'), message: t('io.repair.message'), confirmLabel: t('io.repair.confirm'), cancelFocused: true });
  if (!ok) return;
  try {
    const snap = await rpc<BoardSnapshot>('board.repair', { board: b.id });
    boards.get(b.id)?.load(snap);
    await rpc('board.claim', { board: b.id }).catch(() => null);
    toast.success(t('io.repair.done'));
  } catch (e) {
    toast.error(t('io.repair.failed', { message: errMessage(e) }));
  }
}

// ── Settings bundle ─────────────────────────────────────────────────────────

async function exportSettings() {
  const dest = isTauri ? await pickSavePath(t('commands.io.exportSettings'), 'luau-settings.json', [{ name: 'JSON', extensions: ['json'] }]) : 'luau-settings.json';
  if (!dest) return;
  try {
    const bundle = buildSettingsBundle(settings.all(), $state.snapshot(kb.user), { order: registry.data.order, mirrorOrder: registry.data.mirrorOrder });
    await rpc('settings.export', { path: dest, bundle });
    toast.success(t('toasts.exported'), isTauri ? { action: { label: t('common.reveal'), run: () => reveal(dest) } } : undefined);
  } catch (e) {
    toast.error(t('io.export.failed', { message: errMessage(e) }));
  }
}

/** Mirrors `PROTECTED_KEYS` in crates/luau-core/src/io/settings.rs. */
const PROTECTED_SETTINGS = ['editor.python', 'ai.endpoint', 'ai.provider', 'discovery.roots', 'integrations.allowPush', 'integrations.allowPull', 'integrations.confirmPush', 'integrations.confirmPull', 'integrations.allowInsecure'];

async function importSettings() {
  const files = isTauri ? await pickFile(t('commands.io.importSettings'), [{ name: 'JSON', extensions: ['json'] }]) : ['luau-settings.json'];
  if (!files?.length) return;
  let bundle: { settings: Record<string, unknown>; keybindings: any[]; templates: unknown[]; skipped?: string[] };
  try {
    bundle = await rpc('settings.import', { path: files[0] });
  } catch (e) {
    return toast.error(t('io.settings.invalid', { message: errMessage(e) }));
  }
  const ok = await confirm({ title: t('import.settingsTitle'), message: t('import.settingsMessage'), confirmLabel: t('common.import') });
  if (!ok) return;
  // Security-sensitive keys (interpreter path, AI endpoint, discovery roots,
  // push/pull guards) are never imported: keep this computer's values.
  const keep = Object.fromEntries(PROTECTED_SETTINGS.map((k) => [k, settings.all()[k]]).filter(([, v]) => v !== undefined));
  settings.replaceAll({ ...bundle.settings, ...keep });
  await saveUserKeybindings(bundle.keybindings);
  toast.success(t('toasts.settingsImported'));
  if (bundle.skipped?.length) toast.info(t('io.settings.skipped', { count: bundle.skipped.length }));
}

export const commands: Command[] = [
  { id: 'board.export', title: 'commands.board.export', category: 'export', icon: Download, run: exportBoard },
  { id: 'card.export', title: 'commands.io.exportCard', category: 'export', icon: FileDown, run: () => exportCard(false) },
  { id: 'file.export', title: 'commands.io.exportFile', category: 'export', icon: FileDown, when: "boardType == 'files'", run: () => exportCard(true) },
  { id: 'board.import', title: 'commands.io.import', category: 'import', icon: FolderInput, run: (args?: { path?: string }) => importFlow(args) },
  { id: 'board.newFromTemplate', title: 'commands.io.newFromTemplate', category: 'board', icon: LayoutTemplate, run: newFromTemplate },
  { id: 'board.upgradeSchema', title: 'commands.io.upgradeSchema', category: 'board', icon: ArrowUpCircle, run: upgradeSchema },
  { id: 'board.repair', title: 'commands.io.repair', category: 'board', icon: Wrench, run: repairBoard },
  { id: 'settings.export', title: 'commands.io.exportSettings', category: 'preferences', icon: Upload, hidden: true, run: exportSettings },
  { id: 'settings.import', title: 'commands.io.importSettings', category: 'preferences', icon: Download, hidden: true, run: importSettings },
];
