// Conversation drawer shared by "Quick summary…" (ask: answers from boards,
// cards and history) and "AI…" (agent: answers and proposes changes).

import { rpc, onCoreEvent } from '$lib/backend/rpc';

export type Mode = 'ask' | 'agent';

export interface AskSource {
  board: string;
  boardName: string;
  id: string;
  title: string;
}

export interface Turn {
  id: number;
  role: 'user' | 'ai';
  text: string;
  mode: Mode;
  streaming?: boolean;
  error?: string;
  sources?: AskSource[];
  model?: string;
  /** Agent mode: the plan proposed for this turn (see agent.svelte.ts). */
  plan?: unknown;
}

export const assistant = $state<{ open: boolean; mode: Mode; turns: Turn[]; busy: boolean; draft: string }>({
  open: false,
  mode: 'ask',
  turns: [],
  busy: false,
  draft: '',
});

let nextId = 1;
let seq = 0;

export function openAssistant(mode: Mode, question?: string) {
  assistant.mode = mode;
  assistant.open = true;
  if (question?.trim()) void send(question);
}

export function closeAssistant() {
  assistant.open = false;
}

export function clearConversation() {
  seq++;
  assistant.turns = [];
  assistant.busy = false;
}

/** Stream `ai.chunk` events for `requestId` into `turn`. Returns the unsubscribe. */
export function streamInto(turn: Turn, requestId: string, run: number): () => void {
  return onCoreEvent((e) => {
    if (e.type !== 'custom' || e.name !== 'ai.chunk' || run !== seq) return;
    const p = e.payload as { requestId: string; text: string };
    if (p.requestId === requestId) turn.text += p.text;
  });
}

export const newRequestId = () => 'q' + Math.random().toString(36).slice(2, 12);

/** Push a turn and return the reactive proxy stored in the list. */
export function pushTurn(t: Omit<Turn, 'id'>): Turn {
  assistant.turns.push({ ...t, id: nextId++ });
  return assistant.turns[assistant.turns.length - 1];
}

export async function send(text: string) {
  const question = text.trim();
  if (!question || assistant.busy) return;
  assistant.draft = '';
  pushTurn({ role: 'user', text: question, mode: assistant.mode });
  if (assistant.mode === 'agent') {
    const { runAgentTurn } = await import('./agent.svelte');
    return runAgentTurn(question);
  }
  const run = ++seq;
  const answer = pushTurn({ role: 'ai', text: '', mode: 'ask', streaming: true });
  const requestId = newRequestId();
  assistant.busy = true;
  const off = streamInto(answer, requestId, run);
  try {
    const r = await rpc<{ answer: string; sources: AskSource[]; model: string }>('ai.ask', { question, requestId });
    if (run !== seq) return;
    answer.text = r.answer;
    answer.sources = r.sources;
    answer.model = r.model;
  } catch (e) {
    if (run === seq) answer.error = (e as Error).message;
  } finally {
    off();
    if (run === seq) {
      answer.streaming = false;
      assistant.busy = false;
    }
  }
}

/** Current run counter (agent turns check it to drop stale results). */
export const currentRun = () => seq;
export const nextRun = () => ++seq;
