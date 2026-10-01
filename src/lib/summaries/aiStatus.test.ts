import { describe, expect, it } from 'vitest';
import { describeStatus } from './aiStatus';
import type { AiStatus } from './api';

const base: AiStatus = { provider: 'none', endpoint: '', available: false, model: null, models: [], remote: false };

describe('AI status card', () => {
  it('prefers Apple, hints when Apple Intelligence is off, falls back to basic', () => {
    expect(describeStatus({ ...base, provider: 'apple', available: true, model: 'Apple on-device' })).toEqual({
      provider: 'apple',
      ok: true,
      hint: 'ai.hint.apple',
    });
    expect(describeStatus({ ...base, apple: { status: 'appleIntelligenceNotEnabled', contextSize: 0 } })).toEqual({
      provider: 'basic',
      ok: false,
      hint: 'ai.hint.appleIntelligenceNotEnabled',
    });
    expect(
      describeStatus({ ...base, provider: 'ollama', available: true, model: 'llama3', apple: { status: 'appleIntelligenceNotEnabled', contextSize: 0 } }),
    ).toEqual({ provider: 'ollama', ok: true, hint: 'ai.hint.appleIntelligenceNotEnabled' });
    expect(describeStatus({ ...base, provider: 'ollama', available: true })).toMatchObject({ ok: false, hint: 'ai.hint.noModel' });
    expect(describeStatus({ ...base, apple: { status: 'missing', contextSize: 0 } })).toEqual({ provider: 'basic', ok: false, hint: 'ai.hint.basic' });
    expect(describeStatus({ ...base, provider: 'off' })).toEqual({ provider: 'off', ok: false, hint: null });
  });
});
