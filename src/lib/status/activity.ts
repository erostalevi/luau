// Status light model: a pure reducer over the core's `task` events plus a
// summary (light colour, running tasks, issues). No Svelte, no I/O: the store
// in `activity.svelte.ts` feeds it and components only render the summary.

import type { TaskEvent, TaskIssue } from '$lib/backend/types';

export interface TaskState {
  task: string;
  running: boolean;
  done: number | null;
  total: number | null;
  detail: string | null;
  /** Set while the task is blocked (e.g. a privacy prompt is open). */
  waiting: TaskIssue | null;
  /** Issues reported by the last run (replaced when the task starts again). */
  issues: TaskIssue[];
  failed: boolean;
}

export interface ActivityState {
  tasks: Record<string, TaskState>;
}

/** gray = idle, green = active, yellow = something odd, red = a real problem. */
export type Light = 'idle' | 'active' | 'warn' | 'error';

export interface StatusSummary {
  light: Light;
  /** Something is running right now (the light pulses). */
  busy: boolean;
  running: TaskState[];
  issues: TaskIssue[];
}

export const initialActivity = (): ActivityState => ({ tasks: {} });

const blank = (task: string): TaskState => ({
  task,
  running: false,
  done: null,
  total: null,
  detail: null,
  waiting: null,
  issues: [],
  failed: false,
});

/** Apply one `task` event. Returns a new state (the input is not mutated). */
export function reduceTask(s: ActivityState, e: TaskEvent): ActivityState {
  const prev = s.tasks[e.task] ?? blank(e.task);
  let next: TaskState;
  switch (e.phase) {
    case 'started':
      next = { ...blank(e.task), running: true, done: e.done, total: e.total };
      break;
    case 'progress':
      next = { ...prev, running: true, done: e.done ?? prev.done, total: e.total ?? prev.total, detail: e.detail, waiting: null };
      break;
    case 'waiting':
      next = {
        ...prev,
        running: true,
        done: e.done ?? prev.done,
        waiting: e.issues[0] ?? { level: 'warn', code: 'waitingFolder', subject: null },
      };
      break;
    case 'finished':
      next = { ...prev, running: false, waiting: null, detail: null, issues: e.issues, failed: false };
      break;
    case 'failed': {
      const issues = e.issues.some((i) => i.level === 'error') ? e.issues : [...e.issues, { level: 'error' as const, code: 'taskFailed', subject: null }];
      next = { ...prev, running: false, waiting: null, detail: null, issues, failed: true };
      break;
    }
    default:
      return s;
  }
  return { tasks: { ...s.tasks, [e.task]: next } };
}

/**
 * Apply the core's snapshot (last event per task) fetched after subscribing.
 * Live events are newer, so tasks already seen live are left alone.
 */
export function mergeSnapshot(s: ActivityState, snapshot: TaskEvent[]): ActivityState {
  let out = s;
  for (const e of snapshot) {
    if (s.tasks[e.task]) continue;
    // A snapshot of a running task may hold only its latest progress.
    out = reduceTask(out, e);
  }
  return out;
}

const issueKey = (i: TaskIssue) => `${i.level}|${i.code}|${i.subject ?? ''}`;

function dedupe(list: TaskIssue[]): TaskIssue[] {
  const seen = new Set<string>();
  return list.filter((i) => {
    const k = issueKey(i);
    if (seen.has(k)) return false;
    seen.add(k);
    return true;
  });
}

/**
 * Summarize for the status light. `extra` carries issues from other state
 * (e.g. an unreachable integration). Precedence: error > warn > active > idle.
 */
export function summarize(s: ActivityState, extra: TaskIssue[] = []): StatusSummary {
  const tasks = Object.values(s.tasks);
  const running = tasks.filter((t) => t.running);
  const issues = dedupe([...tasks.flatMap((t) => (t.waiting ? [t.waiting, ...t.issues] : t.issues)), ...extra]);
  // Errors first, then warnings, keeping event order within each level.
  issues.sort((a, b) => (a.level === b.level ? 0 : a.level === 'error' ? -1 : 1));
  const light: Light = issues.some((i) => i.level === 'error') ? 'error' : issues.some((i) => i.level === 'warn') ? 'warn' : running.length ? 'active' : 'idle';
  return { light, busy: running.length > 0, running, issues };
}

/** An i18n key with params, resolved by the component. */
export interface Msg {
  key: string;
  params?: Record<string, string | number>;
}

/** What a running task is doing, honestly (no fake 0/N while nothing is counted). */
export function taskMessage(t: TaskState): Msg {
  // Short line here; the full explanation (which folder, why) is in the issues list.
  if (t.waiting) return { key: t.waiting.code === 'waitingAccess' ? 'status.tasks.waitingAccess' : 'status.tasks.waitingFolder' };
  if (t.task === 'index' && t.total) return { key: 'status.indexing', params: { done: t.done ?? 0, total: t.total } };
  if (t.task === 'index') return { key: 'status.tasks.index' };
  if (t.task === 'discovery') return { key: 'status.tasks.discovery' };
  return { key: 'status.tasks.other' };
}

/** Short text beside the light (only while busy); null when nothing runs. */
export function barMessage(sum: StatusSummary): Msg | null {
  if (!sum.busy) return null;
  const waiting = sum.running.find((t) => t.waiting);
  return taskMessage(waiting ?? sum.running[sum.running.length - 1]);
}

export function issueMessage(i: TaskIssue): Msg {
  const known = (ISSUE_CODES as readonly string[]).includes(i.code);
  return { key: `status.issues.${known ? i.code : 'unknown'}`, params: { name: i.subject ?? '' } };
}

/** Issue codes the UI knows how to phrase (others fall back to a generic line). */
export const ISSUE_CODES = [
  'waitingAccess',
  'waitingFolder',
  'folderMissing',
  'accessDenied',
  'accessPending',
  'folderStalled',
  'folderFailed',
  'boardDamaged',
  'indexSkipped',
  'indexFailed',
  'taskCrashed',
  'taskFailed',
  'integrationOffline',
] as const;
