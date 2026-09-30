<script lang="ts">
  import { Search, Keyboard, FileJson, RotateCcw, Plus, Pencil, Trash2, Copy, Filter, Layers } from '@lucide/svelte';
  import { allCommands, commandTitle, commandCategory, getCommand, runCommand } from '$lib/commands/registry.svelte';
  import { effectiveBindings, kb, saveUserKeybindings } from '$lib/keybindings/resolver.svelte';
  import { eventToStroke, isMac } from '$lib/keybindings/keys';
  import { buildRows, conflictCount, changeKey, addKey, removeKey, changeWhen, resetCommand, isCustomized, isSingleKeyBinding, type BindingRow } from '$lib/keybindings/editor/model';
  import { parseKbQuery, filterRows } from '$lib/keybindings/editor/filter';
  import { ChordRecorder } from '$lib/keybindings/editor/recorder';
  import { validateWhen } from '$lib/keybindings/editor/when';
  import { inputBox } from '$lib/quickinput/qi.svelte';
  import { openMenu } from '$lib/state/menu.svelte';
  import { settings } from '$lib/settings/store.svelte';
  import { copyText, reveal } from '$lib/app/helpers';
  import { app } from '$lib/app/bootstrap';
  import { tip } from '$lib/components/tooltip';
  import Kbd from '$lib/components/Kbd.svelte';
  import { t } from '$lib/i18n/index.svelte';

  let query = $state('');
  let recordSearch = $state(false);
  let searchKeys = $state<string | null>(null);
  const searchRec = new ChordRecorder();
  let editing = $state<{ row: BindingRow; mode: 'change' | 'add' } | null>(null);
  let recorded = $state('');
  const rec = new ChordRecorder();

  const rows = $derived.by(() => {
    void kb.version;
    const ids = allCommands().map((c) => c.id);
    return buildRows(ids, effectiveBindings());
  });
  const text = (r: BindingRow) => {
    const c = getCommand(r.command);
    return { title: c ? commandTitle(c) : r.command, category: c ? commandCategory(c) : '' };
  };
  const visible = $derived(
    filterRows(rows, parseKbQuery(query), text, recordSearch ? searchKeys : null).sort((a, b) => {
      const ta = text(a);
      const tb = text(b);
      return (ta.category + ta.title).localeCompare(tb.category + tb.title);
    }),
  );
  const conflicts = $derived(editing && recorded ? conflictCount(recorded, rows, { command: editing.row.command, rowId: editing.row.id }) : 0);

  async function save(list: typeof kb.user) {
    await saveUserKeybindings(list);
  }

  function startEdit(row: BindingRow, mode: 'change' | 'add' = 'change') {
    rec.clear();
    recorded = '';
    editing = { row, mode };
    kb.recording = true;
  }

  function stopEdit() {
    editing = null;
    kb.recording = false;
  }

  async function commitEdit() {
    if (!editing || !recorded) return;
    const { row, mode } = editing;
    const next = mode === 'add' || !row.key ? addKey(kb.user, row, recorded, isMac) : changeKey(kb.user, row, recorded, isMac);
    stopEdit();
    await save(next);
  }

  function onRecordKey(e: KeyboardEvent) {
    if (!editing) return;
    e.preventDefault();
    e.stopPropagation();
    if (e.key === 'Escape' && !e.metaKey && !e.ctrlKey && !e.altKey && !e.shiftKey) return stopEdit();
    if (e.key === 'Enter' && recorded && !e.metaKey && !e.ctrlKey && !e.altKey) return void commitEdit();
    const s = eventToStroke(e);
    if (s) recorded = rec.push(s).join(' ');
  }

  function onSearchKey(e: KeyboardEvent) {
    if (!recordSearch) return;
    e.preventDefault();
    if (e.key === 'Backspace' && !e.metaKey && !e.ctrlKey) {
      searchRec.clear();
      searchKeys = null;
      return;
    }
    const s = eventToStroke(e);
    if (s) searchKeys = searchRec.push(s).join(' ');
  }

  function toggleRecordSearch() {
    recordSearch = !recordSearch;
    kb.recording = recordSearch;
    searchRec.clear();
    searchKeys = null;
  }

  async function editWhen(row: BindingRow) {
    const v = await inputBox({
      title: t('keys.editor.whenTitle'),
      value: row.when ?? '',
      prompt: t('keys.editor.whenPrompt'),
      validate: (s) => {
        const err = validateWhen(s);
        return err ? t('keys.editor.whenError', { pos: err.at + 1 }) : null;
      },
    });
    if (typeof v === 'string') await save(changeWhen(kb.user, row, v.trim(), isMac));
  }

  function rowMenu(e: MouseEvent, row: BindingRow) {
    openMenu(e, [
      { label: t('keys.editor.change'), icon: Pencil, run: () => startEdit(row) },
      { label: t('keys.editor.add'), icon: Plus, run: () => startEdit(row, 'add') },
      { label: t('keys.editor.changeWhen'), icon: Filter, run: () => void editWhen(row), disabled: !row.key },
      { separator: true },
      { label: t('keys.editor.remove'), icon: Trash2, danger: true, run: () => void save(removeKey(kb.user, row, isMac)), disabled: !row.key },
      { label: t('keys.editor.reset'), icon: RotateCcw, run: () => void save(resetCommand(kb.user, row.command)), disabled: !isCustomized(kb.user, row.command) },
      { separator: true },
      { label: t('keys.editor.copyId'), icon: Copy, run: () => void copyText(row.command) },
      { label: t('keys.editor.sameKeys'), icon: Layers, run: () => ((recordSearch = true), (searchKeys = row.key)), disabled: !row.key },
    ]);
  }

  function sourceLabel(r: BindingRow): string {
    if (r.source === 'none') return '';
    if (r.source === 'preset') return t('keys.editor.source.preset', { name: r.sourceDetail ?? '' });
    if (r.source === 'extension') return r.sourceDetail ?? t('keys.editor.source.extension');
    return t(`keys.editor.source.${r.source}`);
  }
</script>

<svelte:window onkeydown={(e) => (editing ? onRecordKey(e) : undefined)} />

<div class="kbv">
  <header>
    <h1>{t('tabs.keybindings')}</h1>
    <div class="tools">
      <button class="btn sm soft" onclick={() => runCommand('app.pickKeybindingPreset')}>
        <Keyboard size={14} />
        {t('keys.editor.preset', { name: t(`presets.${settings.get<string>('keyboard.preset')}.name`) })}
      </button>
      <button class="btn sm" onclick={() => runCommand('app.toggleSingleKeys')}>
        {settings.get<boolean>('board.singleKeyShortcuts') ? t('keys.editor.singleOn') : t('keys.editor.singleOff')}
      </button>
      <button class="icon-btn" onclick={() => app.info && reveal(`${app.info.configDir}/keybindings.json`)} use:tip={t('keys.editor.openJson')}><FileJson size={16} /></button>
    </div>
  </header>
  <div class="search">
    <Search size={15} class="muted" />
    {#if recordSearch}
      <!-- svelte-ignore a11y_autofocus -->
      <input readonly autofocus value={searchKeys ?? ''} placeholder={t('keys.editor.recordSearchPlaceholder')} onkeydown={onSearchKey} class="rec" />
    {:else}
      <input bind:value={query} placeholder={t('keys.editor.searchPlaceholder')} spellcheck="false" />
    {/if}
    <button class="icon-btn sm" class:active={recordSearch} onclick={toggleRecordSearch} use:tip={t('keys.editor.recordSearch')}><Keyboard size={14} /></button>
  </div>
  <div class="table" role="grid">
    <div class="thead" role="row">
      <span>{t('keys.editor.command')}</span><span>{t('keys.editor.keybinding')}</span><span>{t('keys.editor.when')}</span><span>{t('keys.editor.sourceCol')}</span>
    </div>
    {#each visible as row (row.id)}
      {@const tx = text(row)}
      <div
        class="tr"
        class:user={row.source === 'user'}
        role="row"
        tabindex="0"
        ondblclick={() => startEdit(row)}
        onkeydown={(e) => e.key === 'Enter' && !editing && startEdit(row)}
        oncontextmenu={(e) => rowMenu(e, row)}
        title={row.command}
      >
        <span class="cmd">
          {#if tx.category}<span class="cat">{tx.category}:</span>{/if}
          {tx.title}
        </span>
        <span class="key">
          {#if row.key}<Kbd keys={row.key} />{#if isSingleKeyBinding(row)}<span class="badge">{t('keys.editor.single')}</span>{/if}{:else}<span class="none">—</span>{/if}
        </span>
        <span class="when mono">{row.when ?? ''}</span>
        <span class="src">{sourceLabel(row)}</span>
      </div>
    {:else}
      <div class="empty">{t('quickinput.noResults')}</div>
    {/each}
  </div>
</div>

{#if editing}
  <div class="scrim" role="presentation" onpointerdown={stopEdit}></div>
  <div class="overlay card-surface" role="dialog" aria-modal="true">
    <p class="muted">{t('keys.editor.recordPrompt')}</p>
    <div class="recorded">{#if recorded}<Kbd keys={recorded} />{:else}<span class="muted">…</span>{/if}</div>
    {#if conflicts}
      <button class="conflict" onclick={() => ((recordSearch = true), (searchKeys = recorded), stopEdit())}>{t('keys.editor.conflicts', { count: conflicts })}</button>
    {/if}
    <p class="hint muted">{t('keys.editor.recordHint')}</p>
    <div class="actions">
      <button class="btn sm" onclick={() => rec.armChord()}>{t('keys.editor.addChord')}</button>
      <span class="grow"></span>
      <button class="btn sm" onclick={stopEdit}>{t('common.cancel')}</button>
      <button class="btn sm primary" disabled={!recorded} onclick={commitEdit}>{t('common.save')}</button>
    </div>
  </div>
{/if}

<style>
  .kbv {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    padding: 20px 28px 0;
    gap: 12px;
  }
  header {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  h1 {
    margin: 0;
    flex: 1;
    font-size: var(--fs-xl);
    font-weight: var(--fw-semibold);
  }
  .tools {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .search {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 8px 0 12px;
    height: 38px;
    border-radius: var(--r-md);
    background: var(--bg-elev);
    box-shadow: inset 0 0 0 1px var(--line);
  }
  .search input {
    flex: 1;
    border: none;
    outline: none;
    background: transparent;
  }
  .search input.rec {
    font-weight: var(--fw-medium);
    color: var(--primary-strong);
  }
  .table {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding-bottom: 24px;
  }
  .thead,
  .tr {
    display: grid;
    grid-template-columns: minmax(220px, 2.2fr) minmax(140px, 1fr) minmax(120px, 1.4fr) 110px;
    gap: 12px;
    align-items: center;
    padding: 7px 10px;
  }
  .thead {
    position: sticky;
    top: 0;
    background: var(--bg-glass);
    backdrop-filter: blur(12px);
    font-size: var(--fs-xs);
    font-weight: var(--fw-semibold);
    color: var(--ink-3);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    z-index: 1;
  }
  .tr {
    border-radius: var(--r-sm);
    font-size: var(--fs-md);
  }
  .tr:hover,
  .tr:focus {
    background: var(--bg-hover);
    outline: none;
  }
  .tr.user .src {
    color: var(--primary-strong);
  }
  .cat {
    color: var(--ink-3);
    margin-right: 4px;
  }
  .cmd,
  .when {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .when {
    font-size: var(--fs-xs);
    color: var(--ink-3);
  }
  .key {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .none {
    color: var(--ink-4);
  }
  .badge {
    font-size: 10px;
    padding: 0 6px;
    border-radius: 999px;
    background: var(--secondary);
    color: var(--secondary-ink);
  }
  .src {
    font-size: var(--fs-xs);
    color: var(--ink-3);
  }
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 1100;
    background: var(--bg-overlay);
  }
  .overlay {
    position: fixed;
    z-index: 1101;
    top: 30%;
    left: 50%;
    translate: -50% 0;
    width: min(440px, calc(100vw - 48px));
    padding: 22px;
    border-radius: var(--r-xl);
    text-align: center;
    animation: pop-in var(--dur) var(--ease-out);
  }
  .recorded {
    min-height: 44px;
    display: grid;
    place-items: center;
    margin: 8px 0;
    font-size: 16px;
  }
  .recorded :global(.kbd > span) {
    min-width: 30px;
    height: 30px;
    font-size: 15px;
  }
  .conflict {
    border: none;
    background: var(--warn-soft);
    color: var(--warn);
    border-radius: var(--r-pill);
    padding: 4px 12px;
    font-size: var(--fs-sm);
  }
  .hint {
    font-size: var(--fs-xs);
  }
  .actions {
    display: flex;
    gap: 8px;
    margin-top: 12px;
  }
  .grow {
    flex: 1;
  }
</style>
