import { describe, expect, it } from 'vitest';
import { escapeFtsQuery } from './api';

describe('escapeFtsQuery', () => {
  it('wraps simple tokens with quote-star and joins with OR', () => {
    expect(escapeFtsQuery('hello world')).toBe('"hello"* OR "world"*');
  });

  it('strips FTS5 metacharacters before wrapping', () => {
    expect(escapeFtsQuery('he*y (wor)ld :foo^bar')).toBe(
      '"hey"* OR "world"* OR "foobar"*',
    );
  });

  it('doubles internal double-quotes', () => {
    expect(escapeFtsQuery('he said "hi"')).toBe('"he"* OR "said"* OR """hi"""*');
  });

  it('returns empty string for empty or whitespace input', () => {
    expect(escapeFtsQuery('')).toBe('');
    expect(escapeFtsQuery('   ')).toBe('');
  });

  it('drops tokens that are only metacharacters', () => {
    expect(escapeFtsQuery('*** hello')).toBe('"hello"*');
  });
});
