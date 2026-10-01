import { describe, expect, it } from 'vitest';
import en from '$lib/i18n/parts/integrations.en';
import es from '$lib/i18n/parts/integrations.es';
import { changeMessage, fieldLabel, joinList, serviceName, type Tr } from './message';
import type { Prepared } from './types';

function trFor(dict: Record<string, unknown>): Tr {
  return (key, params) => {
    let v: unknown = dict;
    for (const k of key.split('.')) v = v && typeof v === 'object' ? (v as Record<string, unknown>)[k] : undefined;
    if (typeof v !== 'string') return key;
    return v.replace(/\{(\w+)\}/g, (_, k) => String(params?.[k] ?? `{${k}}`));
  };
}

const push: Prepared = {
  token: 'w1',
  direction: 'push',
  service: 'jira',
  site: 'acme.atlassian.net',
  target: 'PROJ-123 “Fix login bug”',
  fields: ['Status', 'Assignee'],
  changes: [
    { field: 'Status', before: 'In Progress', after: 'Done' },
    { field: 'Assignee', before: 'Ana Pérez', after: null },
  ],
  empty: false,
};

describe('WriteGate confirmation text', () => {
  it('builds "This will update X and Y on Z" (SPEC §9.6)', () => {
    expect(changeMessage(push, 'en', trFor(en))).toBe('This will update Status and Assignee on Jira (acme.atlassian.net) for PROJ-123 “Fix login bug”.');
  });

  it('is localized, including the list conjunction', () => {
    expect(changeMessage(push, 'es', trFor(es))).toBe('Esto actualizará Estado y Responsable en Jira (acme.atlassian.net) para PROJ-123 “Fix login bug”.');
  });

  it('describes pulls as updates of the local card', () => {
    const pull: Prepared = { ...push, direction: 'pull', target: 'Fix login', fields: ['Title', 'Description'] };
    expect(changeMessage(pull, 'en', trFor(en))).toBe('This will update Title and Description on “Fix login” from Jira.');
  });

  it('collapses per-issue mirror rows into field kinds', () => {
    const m: Prepared = { ...push, direction: 'pull', target: 'Team board', fields: ['Status K-1', 'Status K-2', 'Issue', 'Column'] };
    expect(changeMessage(m, 'en', trFor(en))).toBe('This will update Status, Issues, and Columns on “Team board” from Jira.');
  });

  it('labels fields and keeps unknown ones', () => {
    const tr = trFor(en);
    expect(fieldLabel('Description', tr)).toBe('Description');
    expect(fieldLabel('Status K-1', tr)).toBe('Status K-1');
    expect(fieldLabel('Sprint', tr)).toBe('Sprint');
    expect(fieldLabel('Summary', trFor(es))).toBe('Resumen');
  });

  it('joins lists and names services', () => {
    expect(joinList([], 'en')).toBe('');
    expect(joinList(['A'], 'en')).toBe('A');
    expect(joinList(['A', 'B', 'C'], 'en')).toBe('A, B, and C');
    expect(serviceName('trello')).toBe('Trello');
    expect(serviceName('linear')).toBe('linear');
  });
});
