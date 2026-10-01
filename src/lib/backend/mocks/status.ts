// Browser mock for the status light: paired `task` events like the real core
// (discovery, then indexing), a `status.tasks` snapshot, and demo states for QA:
// open the web build with `?status=idle|active|waiting|warn|error`, or call
// `window.luauMockStatus('warn')` from the console.

import type { MockApi } from '../mock';
import type { TaskEvent, TaskIssue } from '../types';

type Demo = 'idle' | 'active' | 'waiting' | 'warn' | 'error';

const ev = (task: string, phase: TaskEvent['phase'], extra: Partial<TaskEvent> = {}): TaskEvent => ({
  task,
  phase,
  done: null,
  total: null,
  detail: null,
  issues: [],
  ...extra,
});
const warn = (code: string, subject: string | null = null): TaskIssue => ({ level: 'warn', code, subject });
const error = (code: string, subject: string | null = null): TaskIssue => ({ level: 'error', code, subject });

export function register(methods: Record<string, (p: Record<string, any>) => unknown>, api: MockApi) {
  const last = new Map<string, TaskEvent>();
  let timers: ReturnType<typeof setTimeout>[] = [];
  const send = (e: TaskEvent) => {
    last.set(e.task, e);
    api.emit({ type: 'task', ...e });
  };
  const play = (steps: [number, TaskEvent][]) => {
    timers.forEach(clearTimeout);
    timers = steps.map(([ms, e]) => setTimeout(() => send(e), ms));
  };
  const boardNames = () => [...api.boards.values()].map((b) => (b as { name: string }).name);

  /** A normal pass: discovery, then indexing with real counts. */
  const pass = () => {
    const names = boardNames();
    const steps: [number, TaskEvent][] = [
      [0, ev('discovery', 'started', { done: 0 })],
      [400, ev('discovery', 'progress', { done: 180 })],
      [900, ev('discovery', 'progress', { done: 620 })],
      [1300, ev('discovery', 'finished')],
      [1320, ev('index', 'started', { done: 0, total: names.length })],
    ];
    names.forEach((n, i) => steps.push([1600 + i * 350, ev('index', 'progress', { done: i + 1, total: names.length, detail: n })]));
    steps.push([1700 + names.length * 350, ev('index', 'finished')]);
    play(steps);
  };

  const demo = (d: Demo) => {
    const at = 0;
    if (d === 'idle')
      play([
        [at, ev('discovery', 'finished')],
        [at, ev('index', 'finished')],
      ]);
    if (d === 'active')
      play([
        [at, ev('index', 'finished')],
        [at, ev('discovery', 'started', { done: 0 })],
        [at + 50, ev('discovery', 'progress', { done: 340 })],
      ]);
    if (d === 'waiting')
      play([
        [at, ev('index', 'finished')],
        [at, ev('discovery', 'started', { done: 0 })],
        [at + 50, ev('discovery', 'waiting', { done: 512, issues: [warn('waitingAccess', 'Documents')] })],
      ]);
    if (d === 'warn')
      play([
        [
          at,
          ev('discovery', 'finished', { issues: [warn('accessPending', 'Desktop'), warn('accessPending', 'Documents'), warn('folderMissing', 'Old boards')] }),
        ],
        [at, ev('index', 'finished', { issues: [warn('indexSkipped', 'Roadmap')] })],
      ]);
    if (d === 'error')
      play([
        [at, ev('discovery', 'finished', { issues: [error('boardDamaged', 'Team notes')] })],
        [at, ev('index', 'failed', { issues: [error('indexFailed')] })],
      ]);
  };

  methods['status.tasks'] = () => [...last.values()];
  methods['discovery.rescan'] = () => (pass(), true);

  const w = window as unknown as { luauMockStatus?: (d: Demo) => void };
  w.luauMockStatus = demo;
  const requested = new URLSearchParams(location.search).get('status') as Demo | null;
  // Like the desktop app: one pass shortly after start (or the requested demo state).
  setTimeout(() => (requested ? demo(requested) : pass()), 1500);
}
