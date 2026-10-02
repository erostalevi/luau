import { describe, expect, it } from 'vitest';
import { mentionText } from './slackTask';
import { appendToBody } from '$lib/markdown/meta';

describe('task from Slack', () => {
  it('keeps who said it and where', () => {
    expect(mentionText({ channel: 'C1', channelName: 'launch', ts: '1', author: 'ana', text: ' Ship it ', permalink: null })).toBe(
      'Ship it\n\n— @ana in #launch',
    );
  });
  it('source links go before the property footer', () => {
    expect(appendToBody('# T\n\nBody\n\n---\ndue: 2026-10-09\n', 'Source: x')).toBe('# T\n\nBody\n\nSource: x\n\n---\ndue: 2026-10-09\n');
    expect(appendToBody('# T\n', 'Source: x')).toBe('# T\n\nSource: x\n');
  });
});
