import { describe, expect, it } from 'vitest';
import { withField } from './propertyMenu';

describe('property chips', () => {
  const doc = '# T\n\nbody\n\n---\npriority: high\ndue: 2026-10-08\nassignees: @eros\n';
  it('changes one field in place', () => {
    expect(withField(doc, 'priority', 'urgent')).toBe('# T\n\nbody\n\n---\npriority: urgent\ndue: 2026-10-08\nassignees: @eros\n');
  });
  it('removes a field when cleared and adds new ones at the end', () => {
    expect(withField(doc, 'due', '')).toBe('# T\n\nbody\n\n---\npriority: high\nassignees: @eros\n');
    expect(withField('# T\n\nbody\n', 'start', '2026-10-01')).toBe('# T\n\nbody\n\n---\nstart: 2026-10-01\n');
  });
});
