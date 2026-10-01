import { describe, expect, it } from 'vitest';
import { ws, moveTab } from './workspace.svelte';

describe('moveTab', () => {
  it('drops a tab exactly where the indicator was (same pane)', () => {
    ws.panes = [{ id: 'p1', tabs: ['A', 'B', 'C'].map((id) => ({ id, kind: 'start' as const })), active: 'A', mru: ['A'] }];
    // Indicator on the right half of B → slot 2 (between B and C).
    moveTab('A', 'p1', 2);
    expect(ws.panes[0].tabs.map((t) => t.id)).toEqual(['B', 'A', 'C']);
    // Moving left is unaffected.
    moveTab('C', 'p1', 0);
    expect(ws.panes[0].tabs.map((t) => t.id)).toEqual(['C', 'B', 'A']);
  });
});
