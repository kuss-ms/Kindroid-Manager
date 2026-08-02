import { describe, expect, it } from 'vitest';
import { escapeFtsQuery } from './api';

describe('escapeFtsQuery', () => {
  it('unquoted tokens become prefix-matched and joined with AND', () => {
    expect(escapeFtsQuery('hello world')).toBe('"hello"* AND "world"*');
  });

  it('quoted text becomes an exact phrase match', () => {
    expect(escapeFtsQuery('"hello world"')).toBe('"hello world"');
  });

  it('mixes quoted phrases and unquoted tokens with AND', () => {
    expect(escapeFtsQuery('hello "world peace"')).toBe('"hello"* AND "world peace"');
  });

  it('joins multiple phrases with AND', () => {
    expect(escapeFtsQuery('"foo bar" "baz qux"')).toBe('"foo bar" AND "baz qux"');
  });

  it('strips FTS5 metacharacters from both tokens and phrases', () => {
    expect(escapeFtsQuery('he*y (wor)ld :foo^bar')).toBe('"hey"* AND "world"* AND "foobar"*');
    expect(escapeFtsQuery('"he*y :foo"')).toBe('"hey foo"');
  });

  it('matches FTS5 tokenisation of an embedded quote', () => {
    // FTS5 treats each standalone `"` as a phrase boundary, so an input
    // like `"he said "hi"` parses the same way FTS5 itself would:
    // phrase `he said ` AND token `hi`. This matches the reference
    // behaviour of the upstream `chat_messages_fts` tokenizer; trying
    // to be cleverer would diverge from it.
    expect(escapeFtsQuery('"he said "hi"')).toBe('"he said " AND "hi"*');
  });

  it('preserves unicode, digits, and hyphens', () => {
    expect(escapeFtsQuery('hello-world 2024 café')).toBe('"hello-world"* AND "2024"* AND "café"*');
  });

  it('treats an unmatched opening quote as a plain token', () => {
    expect(escapeFtsQuery('hello "world')).toBe('"hello"* AND "world"*');
  });

  it('returns an empty string for empty or whitespace input', () => {
    expect(escapeFtsQuery('')).toBe('');
    expect(escapeFtsQuery('   ')).toBe('');
    expect(escapeFtsQuery('\t\n')).toBe('');
  });

  it('drops tokens that consist only of metacharacters', () => {
    expect(escapeFtsQuery('*** hello')).toBe('"hello"*');
    expect(escapeFtsQuery('***')).toBe('');
    expect(escapeFtsQuery('"***" hello')).toBe('"hello"*');
  });

  it('keeps a quoted single-word phrase exact (no wildcard)', () => {
    expect(escapeFtsQuery('"hello" world')).toBe('"hello" AND "world"*');
  });

  it('drops an empty phrase', () => {
    expect(escapeFtsQuery('"" hello')).toBe('"hello"*');
  });

  it('handles tabs and newlines between tokens', () => {
    expect(escapeFtsQuery('hello\tworld\nfoo')).toBe('"hello"* AND "world"* AND "foo"*');
  });
});
