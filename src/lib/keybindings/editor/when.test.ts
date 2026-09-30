import { describe, it, expect } from 'vitest';
import { validateWhen, whenKeys } from './when';
import { BASE, PRESETS } from '../defaults';

describe('validateWhen', () => {
  it('accepts empty and valid expressions', () => {
    for (const e of ['', '   ', 'boardFocus', '!inputFocus', "boardType == 'kanban'", 'a && (b || !c)', 'config.board.singleKeyShortcuts && x != "y"', 'count == 3']) {
      expect(validateWhen(e), e).toBeNull();
    }
  });

  it('accepts every built-in when clause', () => {
    const all = [...BASE, ...Object.values(PRESETS).flat()].map((b) => b.when).filter((w): w is string => !!w);
    for (const w of all) expect(validateWhen(w), w).toBeNull();
  });

  it('reports dangling operators', () => {
    expect(validateWhen('a &&')).toMatchObject({ code: 'unexpectedEnd', at: 4 });
    expect(validateWhen('&& a')).toMatchObject({ code: 'unexpectedToken', at: 0, token: '&&' });
    expect(validateWhen('a ==')).toMatchObject({ code: 'unexpectedEnd' });
    expect(validateWhen('!')).toMatchObject({ code: 'unexpectedEnd' });
  });

  it('reports unbalanced parentheses', () => {
    expect(validateWhen('(a && b')).toMatchObject({ code: 'unclosedParen' });
    expect(validateWhen('a && b)')).toMatchObject({ code: 'unexpectedToken', token: ')' });
    expect(validateWhen('()')).toMatchObject({ code: 'unexpectedToken', token: ')' });
  });

  it('reports invalid characters and strings', () => {
    expect(validateWhen('a & b')).toMatchObject({ code: 'unexpectedChar', at: 2, token: '&' });
    expect(validateWhen("a == 'open")).toMatchObject({ code: 'unterminatedString', at: 5 });
    expect(validateWhen('a = b')).toMatchObject({ code: 'unexpectedChar' });
  });

  it('reports juxtaposed operands', () => {
    expect(validateWhen('a b')).toMatchObject({ code: 'unexpectedToken', at: 2, token: 'b' });
  });
});

describe('whenKeys', () => {
  it('lists referenced context keys', () => {
    expect(whenKeys("boardFocus && !inputFocus && boardType == 'files'")).toEqual(['boardFocus', 'inputFocus', 'boardType']);
    expect(whenKeys('a & b')).toEqual([]);
  });
});
