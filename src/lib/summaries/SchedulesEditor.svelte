<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { Plus, Play, Trash2, CalendarClock, Bell, MessageSquare } from '@lucide/svelte';
  import Segmented from '$lib/components/Segmented.svelte';
  import Toggle from '$lib/components/Toggle.svelte';
  import { ai, newSchedule, onAiEvent, type CadenceKind, type Engine, type PeriodKind, type Schedule } from './api';
  import { registry } from '$lib/state/registry.svelte';
  import { toast } from '$lib/state/toasts.svelte';
  import { confirm } from '$lib/state/dialogs.svelte';
  import { t, i18n, LOCALES } from '$lib/i18n/index.svelte';

  let { onran }: { onran?: (summaryId: string | null) => void } = $props();

  const PERIODS: PeriodKind[] = ['lastDay', 'lastWorkday', 'today', 'lastWeek', 'previousWeek', 'sinceLastRun', 'days'];
  const WEEKDAYS = [1, 2, 3, 4, 5, 6, 7];

  let list = $state<Schedule[]>([]);
  let edit = $state<Schedule | null>(null);
  let busy = $state(false);
  let running = $state<string | null>(null);
  let slack = $state(false);
  const boardsList = $derived(registry.data.boards.filter((b) => !b.missing && !b.hidden));
  let off: (() => void) | null = null;

  async function refresh() {
    list = await ai.schedules().catch(() => []);
    if (edit?.id) {
      const cur = list.find((s) => s.id === edit!.id);
      if (cur) {
        edit.lastRun = cur.lastRun;
        edit.lastStatus = cur.lastStatus;
        edit.nextRun = cur.nextRun;
      }
    }
  }

  onMount(() => {
    void refresh();
    void ai.slackConnected().then((v) => (slack = v));
    off = onAiEvent('schedules.ran', () => void refresh());
  });
  onDestroy(() => off?.());

  function select(s: Schedule) {
    edit = structuredClone($state.snapshot(s)) as Schedule;
  }

  function add() {
    edit = newSchedule(t('schedules.defaultName'));
  }

  async function save() {
    if (!edit) return;
    busy = true;
    try {
      const s = await ai.saveSchedule($state.snapshot(edit) as Schedule);
      toast.success(t('schedules.saved'));
      await refresh();
      const fresh = list.find((x) => x.id === s.id);
      if (fresh) select(fresh);
    } catch (e) {
      toast.error((e as Error).message);
    } finally {
      busy = false;
    }
  }

  async function remove(s: Schedule) {
    if (!(await confirm({ title: t('schedules.deleteConfirm', { name: s.name }), confirmLabel: t('schedules.delete'), danger: true }))) return;
    await ai.deleteSchedule(s.id).catch((e: Error) => toast.error(e.message));
    if (edit?.id === s.id) edit = null;
    await refresh();
  }

  async function runNow(s: Schedule) {
    running = s.id;
    try {
      const r = await ai.runNow(s.id);
      toast.success(t('schedules.ran'));
      await refresh();
      onran?.(r.id);
    } catch (e) {
      toast.error(t('summaries.failed', { message: (e as Error).message }));
    } finally {
      running = null;
    }
  }

  function toggleDay(d: number) {
    if (!edit) return;
    const w = edit.cadence.weekdays;
    edit.cadence.weekdays = w.includes(d) ? w.filter((x) => x !== d) : [...w, d].sort();
  }

  function toggleBoard(id: string) {
    if (!edit) return;
    edit.boards = edit.boards.includes(id) ? edit.boards.filter((x) => x !== id) : [...edit.boards, id];
  }

  function fmt(iso?: string | null) {
    if (!iso) return '';
    const d = new Date(iso);
    return Number.isNaN(d.getTime())
      ? iso
      : d.toLocaleString(i18n.locale, { weekday: 'short', month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' });
  }

  function cadenceLabel(s: Schedule) {
    const base = t(`schedules.cadence.${s.cadence.kind}`);
    const days =
      s.cadence.kind === 'weekly'
        ? ' · ' + s.cadence.weekdays.map((d) => t(`schedules.weekdays.${d}`)).join(' ')
        : s.cadence.kind === 'monthly'
          ? ` · ${s.cadence.day ?? 1}`
          : '';
    return `${base}${days} · ${s.cadence.time}`;
  }
</script>

<div class="schedules">
  <section class="list" aria-label={t('schedules.title')}>
    <button class="btn soft add" onclick={add}><Plus size={14} strokeWidth={1.8} /> {t('schedules.add')}</button>
    {#if list.length === 0}
      <div class="empty">
        <CalendarClock size={22} strokeWidth={1.6} />
        <p>{t('schedules.empty')}</p>
      </div>
    {:else}
      <ul>
        {#each list as s (s.id)}
          <li class="row item" class:active={edit?.id === s.id}>
            <button class="grow open" onclick={() => select(s)}>
              <span class="name" class:off={!s.enabled}>{s.name}</span>
              <span class="meta">{cadenceLabel(s)}</span>
              <span class="meta">{s.enabled && s.nextRun ? t('schedules.next', { when: fmt(s.nextRun) }) : s.lastRun ? '' : t('schedules.never')}</span>
            </button>
            <button class="icon-btn sm" aria-label={t('schedules.runNow')} disabled={running === s.id} onclick={() => runNow(s)}>
              <Play size={14} strokeWidth={1.8} class={running === s.id ? 'spin' : ''} />
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </section>

  {#if edit}
    <section class="editor card-surface" aria-label={edit.name}>
      <div class="line">
        <input class="field name-field" maxlength="80" bind:value={edit.name} aria-label={t('schedules.name')} placeholder={t('schedules.name')} />
        <Toggle bind:checked={edit.enabled} label={t('schedules.enabled')} />
      </div>

      <div class="group">
        <span class="section-title">{t('schedules.when')}</span>
        <div class="line">
          <Segmented
            size="sm"
            bind:value={edit.cadence.kind}
            options={(['daily', 'weekly', 'monthly'] as CadenceKind[]).map((k) => ({ value: k, label: t(`schedules.cadence.${k}`) }))}
          />
          <span class="muted">{t('schedules.at')}</span>
          <input class="field time" type="time" bind:value={edit.cadence.time} aria-label={t('schedules.at')} />
        </div>
        {#if edit.cadence.kind === 'weekly'}
          <div class="chips">
            {#each WEEKDAYS as d (d)}
              <button class="chip" class:on={edit.cadence.weekdays.includes(d)} aria-pressed={edit.cadence.weekdays.includes(d)} onclick={() => toggleDay(d)}
                >{t(`schedules.weekdays.${d}`)}</button
              >
            {/each}
          </div>
        {:else if edit.cadence.kind === 'monthly'}
          <label class="line muted">{t('schedules.dayOfMonth')} <input class="field num" type="number" min="1" max="31" bind:value={edit.cadence.day} /></label>
        {/if}
      </div>

      <div class="group">
        <label class="section-title" for="sch-period">{t('schedules.covers')}</label>
        <div class="line">
          <select id="sch-period" class="field" bind:value={edit.period.kind}>
            {#each PERIODS as p (p)}
              <option value={p}>{t(`schedules.periods.${p}`)}</option>
            {/each}
          </select>
          {#if edit.period.kind === 'days'}
            <input class="field num" type="number" min="1" max="90" bind:value={edit.period.days} aria-label={t('schedules.days')} />
          {/if}
        </div>
      </div>

      <div class="group">
        <span class="section-title">{t('summaries.boards')}</span>
        <div class="chips">
          <button class="chip" class:on={edit.boards.length === 0} onclick={() => edit && (edit.boards = [])}>{t('summaries.allBoards')}</button>
          {#each boardsList as b (b.id)}
            <button class="chip" class:on={edit.boards.includes(b.id)} aria-pressed={edit.boards.includes(b.id)} onclick={() => toggleBoard(b.id)}
              >{b.name}</button
            >
          {/each}
        </div>
      </div>

      <div class="group">
        <label class="section-title" for="sch-detail">{t('summaries.detail')} · <span class="muted">{t(`summaries.detailLevels.${edit.detail}`)}</span></label>
        <input id="sch-detail" class="slider" type="range" min="1" max="5" step="1" bind:value={edit.detail} />
      </div>

      <div class="group">
        <label class="section-title" for="sch-prompt">{t('summaries.prompt')}</label>
        <textarea id="sch-prompt" class="field" rows="2" maxlength="4000" placeholder={t('summaries.promptPlaceholder')} bind:value={edit.prompt}></textarea>
      </div>

      <div class="group two">
        <div>
          <span class="section-title">{t('summaries.engine')}</span>
          <Segmented
            size="sm"
            bind:value={edit.engine}
            options={(['auto', 'ai', 'basic'] as Engine[]).map((k) => ({ value: k, label: t(`summaries.engines.${k}`) }))}
          />
        </div>
        <div>
          <label class="section-title" for="sch-lang">{t('schedules.language')}</label>
          <select id="sch-lang" class="field" bind:value={edit.locale}>
            <option value="">{t('schedules.appLanguage')}</option>
            {#each LOCALES as l (l.id)}
              <option value={l.id}>{l.name}</option>
            {/each}
          </select>
        </div>
      </div>

      <div class="group">
        <span class="section-title">{t('schedules.deliver')}</span>
        <div class="line">
          <Bell size={14} strokeWidth={1.8} />
          <span class="grow">{t('schedules.notification')}</span>
          <Toggle bind:checked={edit.deliver.notification} label={t('schedules.notification')} />
        </div>
        <div class="line">
          <MessageSquare size={14} strokeWidth={1.8} />
          <span class="grow">{t('schedules.slack')}</span>
          <Toggle bind:checked={edit.deliver.slack} label={t('schedules.slack')} disabled={!slack && !edit.deliver.slack} />
        </div>
        {#if edit.deliver.slack}
          <input
            class="field"
            maxlength="80"
            placeholder={t('schedules.slackChannel')}
            bind:value={edit.deliver.slackChannel}
            aria-label={t('schedules.slackChannel')}
          />
        {/if}
        {#if !slack}
          <span class="muted">{t('schedules.slackOff')}</span>
        {/if}
      </div>

      {#if edit.lastRun}
        <p class="muted">{t('schedules.last', { when: fmt(edit.lastRun), status: edit.lastStatus ?? '' })}</p>
      {/if}

      <div class="actions">
        {#if edit.id}
          <button class="btn ghost danger" onclick={() => edit && remove(edit)}><Trash2 size={14} strokeWidth={1.8} /> {t('schedules.delete')}</button>
          <button class="btn soft" disabled={running === edit.id} onclick={() => edit && runNow(edit)}
            ><Play size={14} strokeWidth={1.8} /> {t('schedules.runNow')}</button
          >
        {/if}
        <button class="btn primary" disabled={busy || !edit.name.trim()} onclick={save}>{t('schedules.save')}</button>
      </div>
    </section>
  {/if}
</div>

<style>
  .schedules {
    display: grid;
    grid-template-columns: minmax(240px, 300px) minmax(0, 620px);
    gap: var(--sp-5);
    align-items: start;
  }
  @media (max-width: 900px) {
    .schedules {
      grid-template-columns: 1fr;
    }
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: var(--sp-2);
  }
  .add {
    align-self: flex-start;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .open {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 1px;
    background: none;
    border: 0;
    padding: 0;
    color: inherit;
    cursor: pointer;
    text-align: left;
    min-width: 0;
  }
  .name {
    font-size: var(--fs-md);
    font-weight: var(--fw-medium);
  }
  .name.off {
    color: var(--ink-4);
  }
  .meta,
  .muted {
    font-size: var(--fs-xs);
    color: var(--ink-3);
  }
  .editor {
    padding: var(--sp-5);
    display: flex;
    flex-direction: column;
    gap: var(--sp-4);
  }
  .group {
    display: flex;
    flex-direction: column;
    gap: var(--sp-2);
  }
  .group.two {
    flex-direction: row;
    gap: var(--sp-5);
    flex-wrap: wrap;
  }
  .group.two > div {
    display: flex;
    flex-direction: column;
    gap: var(--sp-2);
  }
  .line {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
  }
  .grow {
    flex: 1;
  }
  .name-field {
    flex: 1;
    font-size: var(--fs-lg);
  }
  .time {
    width: 110px;
  }
  .num {
    width: 72px;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: var(--sp-1);
  }
  .chip {
    cursor: pointer;
    border: 1px solid transparent;
  }
  .chip.on {
    background: var(--primary-soft);
    color: var(--primary-strong);
    border-color: var(--primary-ring);
  }
  .slider {
    width: 100%;
    accent-color: var(--primary);
  }
  textarea {
    resize: vertical;
    font-family: inherit;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--sp-2);
  }
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--sp-2);
    padding: var(--sp-8) 0;
    color: var(--ink-3);
  }
</style>
