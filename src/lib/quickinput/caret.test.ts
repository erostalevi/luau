import { describe, it, expect } from 'vitest';
import { initialSelection } from './caret';

describe('initialSelection', () => {
  it('selects the whole pre-filled value by default', () => {
    expect(initialSelection('My board', undefined)).toEqual([0, 8]);
    expect(initialSelection('My board', true)).toEqual([0, 8]);
  });

  it('puts the caret after a palette prefix with nothing selected', () => {
    expect(initialSelection('>', false)).toEqual([1, 1]);
    expect(initialSelection('#', false)).toEqual([1, 1]);
    expect(initialSelection('>toggle', false)).toEqual([7, 7]);
  });

  it('handles an empty value', () => {
    expect(initialSelection('', undefined)).toEqual([0, 0]);
    expect(initialSelection('', false)).toEqual([0, 0]);
  });
});
