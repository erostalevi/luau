// What the AI settings card shows for an `ai.status` answer (pure, i18n keys only).

import type { AiStatus } from './api';

export interface StatusView {
  /** `ai.providerName.*` key of the writer in use. */
  provider: 'apple' | 'ollama' | 'openai' | 'basic' | 'off';
  ok: boolean;
  /** `ai.hint.*` key explaining the state, or null. */
  hint: string | null;
}

export function describeStatus(s: AiStatus): StatusView {
  if (s.provider === 'off') return { provider: 'off', ok: false, hint: null };
  const apple = s.apple?.status;
  // A visible reason to enable Apple's model (only when it could exist on this Mac).
  const appleHint = apple && apple !== 'available' && apple !== 'missing' && apple !== 'unsupportedOs' ? `ai.hint.${apple}` : null;
  if (s.available && (s.provider === 'apple' || s.provider === 'ollama' || s.provider === 'openai')) {
    if (s.provider !== 'apple' && !s.model) return { provider: s.provider, ok: false, hint: 'ai.hint.noModel' };
    return { provider: s.provider, ok: true, hint: s.provider === 'apple' ? 'ai.hint.apple' : appleHint };
  }
  return { provider: 'basic', ok: false, hint: appleHint ?? 'ai.hint.basic' };
}
