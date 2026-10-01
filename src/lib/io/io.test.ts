import { describe, expect, it } from 'vitest';
import {
  exportFileName,
  exportFormats,
  importModes,
  isNewerSchema,
  isLoose,
  buildSettingsBundle,
  isSecretKey,
  templateSpec,
  BASIC_BOARD_TEMPLATES,
  formatBytes,
} from './io';

describe('io helpers', () => {
  it('offers formats per target', () => {
    expect(exportFormats('board')).toEqual(['html', 'pdf', 'md', 'mdBundle', 'zip', 'json']);
    expect(exportFormats('card')).not.toContain('zip');
  });

  it('builds safe export file names', () => {
    expect(exportFileName('Plan: Q4 / H1?', 'md')).toBe('Plan- Q4 - H1-.md');
    expect(exportFileName('  ..  ', 'html')).toBe('Untitled.html');
    expect(exportFileName('Board', 'mdBundle')).toBe('Board (Markdown).zip');
    expect(exportFileName('x'.repeat(200), 'json').length).toBe(85);
  });

  it('lists import modes with copy as default', () => {
    expect(importModes({ kind: 'folder' }, true)).toEqual(['copy', 'inPlace', 'overwrite']);
    expect(importModes({ kind: 'boardZip' }, false)).toEqual(['copy']);
    expect(importModes({ kind: 'interchange' }, true)).toEqual(['copy', 'overwrite']);
  });

  it('classifies read-only reasons', () => {
    expect(isNewerSchema('newer_schema:3')).toBe(true);
    expect(isNewerSchema('mirror')).toBe(false);
    expect(isNewerSchema(null)).toBe(false);
    expect(isLoose('loose')).toBe(true);
  });

  it('strips secret-looking keys from settings bundles', () => {
    expect(isSecretKey('jira.apiToken')).toBe(true);
    const b = buildSettingsBundle({ theme: 'dark', 'ai.secretKey': 'x' }, [{ key: 'a', command: 'b' }]);
    expect(b.format).toBe('luau-settings');
    expect(Object.keys(b.settings)).toEqual(['theme']);
    expect(b.keybindings).toHaveLength(1);
  });

  it('builds valid templates for every id', () => {
    for (const id of BASIC_BOARD_TEMPLATES) {
      const s = templateSpec(id, (k) => k.split('.').pop()!);
      if (s.kind === 'files') {
        expect(s.lanes).toEqual([]);
        expect(s.notes.length).toBeGreaterThan(0);
      } else {
        expect(s.notes).toEqual([]);
        expect(s.lanes.length).toBeGreaterThanOrEqual(3);
      }
      for (const c of [...s.notes, ...s.lanes.flatMap((l) => l.cards)]) expect(c.startsWith('# ')).toBe(true);
    }
  });

  it('formats sizes', () => {
    expect(formatBytes(10)).toBe('10 B');
    expect(formatBytes(2048)).toBe('2.0 KB');
    expect(formatBytes(3 * 1024 * 1024)).toBe('3.0 MB');
  });
});
