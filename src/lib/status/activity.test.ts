import { describe, expect, it } from 'vitest';
import type { TaskEvent, TaskIssue } from '$lib/backend/types';
import { ISSUE_CODES, barMessage, initialActivity, issueMessage, mergeSnapshot, reduceTask, summarize, type ActivityState } from './activity';
import en from '$lib/i18n/parts/status.en';
import es from '$lib/i18n/parts/status.es';
import pt from '$lib/i18n/parts/status.pt';

const ev = (task: string, phase: TaskEvent['phase'], extra: Partial<TaskEvent> = {}): TaskEvent => ({
  task,
  phase,
  done: null,
  total: null,
  detail: null,
  issues: [],
  ...extra,
});

const run = (...events: TaskEvent[]): ActivityState => events.reduce(reduceTask, initialActivity());

const warn = (code: string, subject: string | null = null): TaskIssue => ({ level: 'warn', code, subject });
const error = (code: string, subject: string | null = null): TaskIssue => ({ level: 'error', code, subject });

describe('status light reducer', () => {
  it('is idle (gray) with no events', () => {
    expect(summarize(initialActivity())).toMatchObject({ light: 'idle', busy: false, issues: [] });
  });

  it('is active (green) while a task runs and idle once it finishes', () => {
    const s = run(ev('discovery', 'started', { done: 0 }));
    expect(summarize(s)).toMatchObject({ light: 'active', busy: true });
    expect(summarize(reduceTask(s, ev('discovery', 'finished')))).toMatchObject({ light: 'idle', busy: false });
  });

  it('regression: discovery that finds nothing ends idle (no endless "Indexing 0/…")', () => {
    const s = run(ev('discovery', 'started'), ev('discovery', 'finished'), ev('index', 'started', { done: 0, total: 0 }), ev('index', 'finished'));
    const sum = summarize(s);
    expect(sum.light).toBe('idle');
    expect(barMessage(sum)).toBeNull();
  });

  it('reports waiting honestly (yellow, still busy) and clears it on progress', () => {
    let s = run(ev('discovery', 'started'), ev('discovery', 'waiting', { done: 40, issues: [warn('waitingAccess', 'Documents')] }));
    let sum = summarize(s);
    expect(sum).toMatchObject({ light: 'warn', busy: true });
    expect(barMessage(sum)).toEqual({ key: 'status.tasks.waitingAccess' });
    expect(sum.issues).toEqual([warn('waitingAccess', 'Documents')]);
    s = reduceTask(s, ev('discovery', 'progress', { done: 41 }));
    sum = summarize(s);
    expect(sum.light).toBe('active');
    expect(barMessage(sum)).toEqual({ key: 'status.tasks.discovery' });
  });

  it('shows real index counts, never a fake 0/N while counting nothing', () => {
    const s = run(ev('index', 'started', { done: 0, total: 3 }), ev('index', 'progress', { done: 2, total: 3, detail: 'Work' }));
    expect(barMessage(summarize(s))).toEqual({ key: 'status.indexing', params: { done: 2, total: 3 } });
    const empty = run(ev('index', 'started', { done: 0, total: 0 }));
    expect(barMessage(summarize(empty))).toEqual({ key: 'status.tasks.index' });
  });

  it('keeps warnings after finishing (yellow) and errors win (red)', () => {
    const s = run(ev('discovery', 'started'), ev('discovery', 'finished', { issues: [warn('accessPending', 'Desktop')] }));
    expect(summarize(s).light).toBe('warn');
    const red = reduceTask(s, ev('index', 'failed', { issues: [error('indexFailed')] }));
    const sum = summarize(red);
    expect(sum.light).toBe('error');
    expect(sum.issues[0].level).toBe('error');
  });

  it('a failed event without an error issue still turns red', () => {
    const s = run(ev('index', 'started'), ev('index', 'failed'));
    expect(summarize(s).light).toBe('error');
    expect(summarize(s).issues).toEqual([error('taskFailed')]);
  });

  it('a new run clears the issues of the previous run of that task', () => {
    const s = run(ev('discovery', 'started'), ev('discovery', 'finished', { issues: [warn('folderMissing', 'Old')] }), ev('discovery', 'started'));
    expect(summarize(s)).toMatchObject({ light: 'active', issues: [] });
  });

  it('merges extra issues (integrations) and deduplicates', () => {
    const s = run(ev('discovery', 'finished', { issues: [warn('accessDenied', 'Downloads'), warn('accessDenied', 'Downloads')] }));
    const sum = summarize(s, [warn('integrationOffline', 'Jira offline')]);
    expect(sum.issues).toHaveLength(2);
    expect(sum.light).toBe('warn');
  });

  it('snapshot fills tasks not yet seen live, without overriding live ones', () => {
    const live = run(ev('index', 'started', { total: 5 }));
    const merged = mergeSnapshot(live, [ev('index', 'finished'), ev('discovery', 'waiting', { issues: [warn('waitingAccess', 'Desktop')] })]);
    expect(merged.tasks.index.running).toBe(true);
    expect(merged.tasks.discovery.running).toBe(true);
    expect(merged.tasks.discovery.waiting?.subject).toBe('Desktop');
  });

  it('does not mutate the previous state', () => {
    const s0 = run(ev('discovery', 'started'));
    const before = JSON.stringify(s0);
    reduceTask(s0, ev('discovery', 'finished', { issues: [warn('folderMissing')] }));
    expect(JSON.stringify(s0)).toBe(before);
  });

  it('unknown issue codes fall back to a generic message', () => {
    expect(issueMessage(warn('somethingNew', 'X')).key).toBe('status.issues.unknown');
    expect(issueMessage(warn('boardDamaged', 'X')).key).toBe('status.issues.boardDamaged');
  });

  it('a corrected result after the run (late folder answered) replaces the issues', () => {
    const s = run(ev('discovery', 'started'), ev('discovery', 'finished', { issues: [warn('accessPending', 'Desktop')] }), ev('discovery', 'finished'));
    expect(summarize(s)).toMatchObject({ light: 'idle', busy: false, issues: [] });
  });

  it('every issue code and light is phrased in en, es and pt', () => {
    for (const dict of [en, es, pt]) {
      const st = dict.status as { issues: Record<string, string>; light: Record<string, string>; explain: Record<string, string> };
      for (const code of [...ISSUE_CODES, 'unknown']) expect(st.issues[code], code).toBeTruthy();
      for (const l of ['idle', 'active', 'warn', 'error']) expect(st.light[l] && st.explain[l], l).toBeTruthy();
    }
  });
});
