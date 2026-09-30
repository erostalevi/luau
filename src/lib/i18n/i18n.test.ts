import { afterEach, describe, expect, it, vi } from 'vitest';
import { detectLocale, fmtDate, i18n, normalizeLocale, relTime, setLocale, t } from './index.svelte';

afterEach(async () => {
  vi.restoreAllMocks();
  await setLocale('en');
});

describe('locale selection', () => {
  it('normalizes regional and unknown values', () => {
    expect(normalizeLocale('es-CL')).toBe('es');
    expect(normalizeLocale('pt-BR')).toBe('pt');
    expect(normalizeLocale('fr')).toBe('en');
    expect(normalizeLocale(undefined)).toBe('en');
  });

  it('falls back to English instead of crashing on an unknown stored locale', async () => {
    await setLocale('xx' as never);
    expect(i18n.locale).toBe('en');
  });

  it('picks the first supported language from the preference list', () => {
    vi.spyOn(navigator, 'languages', 'get').mockReturnValue(['de-DE', 'es-CL', 'en-US']);
    expect(detectLocale()).toBe('es');
    vi.spyOn(navigator, 'languages', 'get').mockReturnValue(['fr-FR']);
    expect(detectLocale()).toBe('en');
  });
});

describe('translations', () => {
  it('translates with plurals and placeholders per locale', async () => {
    await setLocale('es');
    expect(t('status.cards', { count: 1 })).toBe('1 tarjeta');
    expect(t('status.cards', { count: 3 })).toBe('3 tarjetas');
    expect(t('boards.created', { name: 'Hogar' })).toBe('Se creó “Hogar”');
    await setLocale('pt');
    expect(t('status.cards', { count: 2 })).toBe('2 cartões');
    expect(t('editor.saveFailed', { message: 'x' })).toBe('Não foi possível salvar: x');
  });
});

describe('date formatting', () => {
  it('returns an empty string for missing or invalid timestamps', () => {
    expect(relTime('not a date')).toBe('');
    expect(relTime(Number.NaN)).toBe('');
  });

  it('keeps an unparseable date string as-is', () => {
    expect(fmtDate('someday')).toBe('someday');
  });

  it('formats relative time in the active locale', async () => {
    await setLocale('es');
    expect(relTime(Date.now() - 2 * 86400_000)).toBe('anteayer');
    await setLocale('pt');
    expect(relTime(Date.now() - 3 * 3600_000)).toBe('há 3 horas');
  });
});
