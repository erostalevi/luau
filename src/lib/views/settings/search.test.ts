import { describe, it, expect } from 'vitest';
import { CATEGORIES, CORE_SETTINGS, type SettingDef } from '$lib/settings/schema';
import { categoryOrder, filterSettings, humanizeKey, isFiltering, parseSettingsQuery } from './search';
import { dayPart } from '../start/greeting';

const text = (d: SettingDef) => ({ label: humanizeKey(d.key), desc: `Description of ${d.key}` });
const none = () => false;

describe('parseSettingsQuery', () => {
  it('extracts filters and words', () => {
    const q = parseSettingsQuery('@modified  Width @category:Editor @ext:jira @id:editor.width', CATEGORIES);
    expect(q).toEqual({ words: ['width'], modified: true, categories: ['editor'], sources: ['jira'], ids: ['editor.width'] });
  });
  it('supports @ai and @<category> shorthands', () => {
    expect(parseSettingsQuery('@ai').categories).toEqual(['ai']);
    expect(parseSettingsQuery('@board', CATEGORIES).categories).toEqual(['board']);
    expect(parseSettingsQuery('@unknown', CATEGORIES).words).toEqual(['@unknown']);
  });
  it('isFiltering', () => {
    expect(isFiltering(parseSettingsQuery('  '))).toBe(false);
    expect(isFiltering(parseSettingsQuery('@modified'))).toBe(true);
  });
});

describe('filterSettings', () => {
  it('never shows hidden settings', () => {
    const all = filterSettings(CORE_SETTINGS, parseSettingsQuery(''), text, none);
    expect(all.some((d) => d.hidden)).toBe(false);
    expect(all.length).toBe(CORE_SETTINGS.filter((d) => !d.hidden).length);
  });
  it('matches label, description, key and options (all words)', () => {
    expect(filterSettings(CORE_SETTINGS, parseSettingsQuery('auto pair'), text, none).map((d) => d.key)).toEqual(['editor.autoPair']);
    expect(filterSettings(CORE_SETTINGS, parseSettingsQuery('ollama'), text, none).map((d) => d.key)).toEqual(['ai.provider']);
    expect(filterSettings(CORE_SETTINGS, parseSettingsQuery('description of editor.width'), text, none).map((d) => d.key)).toEqual(['editor.width']);
  });
  it('filters modified, category, source and id', () => {
    const mod = (k: string) => k === 'editor.vim';
    expect(filterSettings(CORE_SETTINGS, parseSettingsQuery('@modified'), text, mod).map((d) => d.key)).toEqual(['editor.vim']);
    const ai = filterSettings(CORE_SETTINGS, parseSettingsQuery('@ai'), text, none);
    expect(ai.length).toBeGreaterThan(0);
    expect(ai.every((d) => d.category === 'ai')).toBe(true);
    const ext: SettingDef[] = [...CORE_SETTINGS, { key: 'jira.sync', type: 'boolean', default: true, category: 'integrations', source: 'jira' }];
    expect(filterSettings(ext, parseSettingsQuery('@ext:jira'), text, none).map((d) => d.key)).toEqual(['jira.sync']);
    expect(filterSettings(CORE_SETTINGS, parseSettingsQuery('@id:board.face.*'), text, none).every((d) => d.key.startsWith('board.face.'))).toBe(true);
  });
});

describe('categoryOrder', () => {
  it('appends feature categories after the known ones', () => {
    const defs = [...CORE_SETTINGS, { key: 'x.y', type: 'boolean', default: false, category: 'custom' } as unknown as SettingDef];
    const order = categoryOrder(defs, CATEGORIES);
    expect(order.slice(0, CATEGORIES.length)).toEqual(CATEGORIES);
    expect(order[order.length - 1]).toBe('custom');
  });
});

describe('humanizeKey', () => {
  it('turns the last key segment into words', () => {
    expect(humanizeKey('editor.autoPair')).toBe('Auto pair');
    expect(humanizeKey('jira.sync_interval')).toBe('Sync interval');
  });
});

describe('dayPart', () => {
  it('buckets hours', () => {
    expect(dayPart(6)).toBe('morning');
    expect(dayPart(13)).toBe('afternoon');
    expect(dayPart(19)).toBe('evening');
    expect(dayPart(2)).toBe('night');
    expect(dayPart(23)).toBe('night');
  });
});
