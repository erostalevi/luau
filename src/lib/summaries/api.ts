// Typed client for the AI / activity-summary / schedules RPCs.
// Contract: src-tauri/src/ai_rpc.rs + crates/luau-core/src/ai/.

import { rpc, onCoreEvent } from '$lib/backend/rpc';

export type Engine = 'auto' | 'ai' | 'basic';

export interface ModelInfo {
  name: string;
  size?: number;
}

export interface AiStatus {
  /** `ollama` | `openai` | `off` | `none` (nothing found). */
  provider: string;
  endpoint: string;
  available: boolean;
  model: string | null;
  models: string[];
  error?: string;
  remote: boolean;
}

export interface FactItem {
  id: string;
  title: string;
  lane?: string;
  due?: string;
  priority?: string;
  tags?: string[];
  at?: string;
}

export interface BoardFacts {
  id: string;
  name: string;
  created: FactItem[];
  completed: FactItem[];
  started: FactItem[];
  moved: { item: FactItem; from: string | null; to: string | null }[];
  edited: { item: FactItem; edits: number; chars: number }[];
  deleted: FactItem[];
  archived: FactItem[];
  external: FactItem[];
  pending: FactItem[];
  overdue: FactItem[];
}

export interface Totals {
  boards: number;
  created: number;
  completed: number;
  started: number;
  moved: number;
  edited: number;
  deleted: number;
  archived: number;
  external: number;
  pending: number;
  overdue: number;
  events: number;
}

export interface ActivityFacts {
  period: { from: string; to: string };
  boards: BoardFacts[];
  totals: Totals;
}

export interface SummarizeParams {
  from: string;
  to: string;
  /** Empty = all boards. */
  boards: string[];
  /** 1 (brief) … 5 (exhaustive). */
  detail: number;
  prompt?: string;
  engine: Engine;
  requestId?: string;
  locale?: string;
  includeRemote?: boolean;
  links?: boolean;
  save?: boolean;
  title?: string;
}

export interface SummaryResult {
  id: string | null;
  title: string;
  markdown: string;
  engine: 'ai' | 'basic';
  model: string | null;
  fallbackReason: string | null;
  created: string;
  facts: ActivityFacts;
  delivery: string[];
}

export interface SavedMeta {
  id: string;
  created: string;
  title: string;
  from: string;
  to: string;
  engine: string;
  model: string | null;
  scheduleId: string | null;
  delivery: string[];
  excerpt: string;
}

export interface SavedSummary extends Omit<SavedMeta, 'excerpt'> {
  boards: string[];
  detail: number;
  prompt: string;
  markdown: string;
}

export type CadenceKind = 'daily' | 'weekly' | 'monthly';
export type PeriodKind = 'lastDay' | 'lastWorkday' | 'today' | 'lastWeek' | 'previousWeek' | 'sinceLastRun' | 'days';

export interface Schedule {
  id: string;
  name: string;
  enabled: boolean;
  cadence: { kind: CadenceKind; time: string; weekdays: number[]; day: number | null };
  period: { kind: PeriodKind; days: number | null };
  boards: string[];
  prompt: string;
  detail: number;
  engine: Engine;
  deliver: { notification: boolean; slack: boolean; slackChannel: string | null };
  locale: string;
  created?: string | null;
  lastRun?: string | null;
  lastStatus?: string | null;
  /** Only in `schedules.list`. */
  nextRun?: string | null;
}

export interface PullProgress {
  name: string;
  status: string;
  completed?: number;
  total?: number;
  done: boolean;
}

export function newSchedule(name: string): Schedule {
  return {
    id: '',
    name,
    enabled: true,
    cadence: { kind: 'daily', time: '09:00', weekdays: [1, 2, 3, 4, 5], day: 1 },
    period: { kind: 'lastWorkday', days: 7 },
    boards: [],
    prompt: '',
    detail: 3,
    engine: 'auto',
    deliver: { notification: true, slack: false, slackChannel: null },
    locale: '',
  };
}

export const ai = {
  status: () => rpc<AiStatus>('ai.status'),
  models: () => rpc<ModelInfo[]>('ai.models'),
  pullModel: (name: string) => rpc<void>('ai.pullModel', { name }),
  facts: (p: { from: string; to: string; boards: string[]; includeRemote?: boolean }) => rpc<ActivityFacts>('activity.facts', p),
  summarize: (p: SummarizeParams) => rpc<SummaryResult>('activity.summarize', p as unknown as Record<string, unknown>),
  cardSummarize: (p: { board: string; id: string; engine?: Engine; detail?: number; requestId?: string }) =>
    rpc<{ text: string; engine: string; model: string | null; cached: boolean }>('card.summarize', p),
  schedules: () => rpc<Schedule[]>('schedules.list'),
  saveSchedule: (schedule: Schedule) => {
    // nextRun is computed server-side; never send it back.
    const { nextRun: _n, ...s } = schedule;
    return rpc<Schedule>('schedules.save', { schedule: s });
  },
  deleteSchedule: (id: string) => rpc<void>('schedules.delete', { id }),
  runNow: (id: string) => rpc<SummaryResult>('schedules.runNow', { id }),
  saved: (limit = 50) => rpc<SavedMeta[]>('summaries.list', { limit }),
  getSaved: (id: string) => rpc<SavedSummary>('summaries.get', { id }),
  deleteSaved: (id: string) => rpc<void>('summaries.delete', { id }),
  slackConnected: () => rpc<boolean>('ai.slackConnected').catch(() => false),
};

/** Subscribe to a feature event (`summary.chunk`, `summary.progress`, `ai.pull`, `schedules.ran`). */
export function onAiEvent<T = Record<string, unknown>>(name: string, fn: (payload: T) => void): () => void {
  return onCoreEvent((e) => {
    if (e.type === 'custom' && e.name === name) fn(e.payload as T);
  });
}

export const requestId = () => 'r' + Math.random().toString(36).slice(2, 12);
