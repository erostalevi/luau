import { describe, expect, it } from 'vitest';
import { editContent, type PlannedAction } from './agent.svelte';

const a = (type: PlannedAction['type'], extra: Partial<PlannedAction>): PlannedAction => ({
  type,
  board: 'b',
  boardName: 'B',
  card: 'c1',
  cardTitle: 'T',
  lane: '',
  laneName: '',
  title: '',
  text: '',
  key: '',
  value: '',
  channel: '',
  because: 'x',
  risky: false,
  ...extra,
});

describe('assistant card edits', () => {
  const doc = '# Old\n\nBody.\n\n---\npriority: low\n';
  it('rename, property, tag and text, keeping the footer last', () => {
    const out = editContent(doc, [
      a('rename_card', { title: 'New' }),
      a('set_property', { key: 'priority', value: 'urgent' }),
      a('add_tag', { value: 'release' }),
      a('append_text', { text: '- [ ] follow up' }),
    ]);
    expect(out).toBe('# New\n\nBody.\n\n#release\n\n- [ ] follow up\n\n---\npriority: urgent\n');
  });
  it('assignees get @ and new fields go to a new footer', () => {
    expect(editContent('# T\n\nBody\n', [a('set_property', { key: 'assignees', value: 'ana, @eros' })])).toBe('# T\n\nBody\n\n---\nassignees: @ana, @eros\n');
  });
});
