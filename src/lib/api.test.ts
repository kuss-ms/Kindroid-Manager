import { describe, expect, it } from 'vitest';
import { errorMessage, escapeFtsQuery } from './api';
import type { PushRequest } from './types';

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

// Regression test for the C2 audit finding: selected journal entries
// were silently dropped from pushes because the JS side used
// `journalEntryIds` (camelCase) while the Rust `PushRequest.journal_entry_ids`
// field is snake_case without `rename_all`. The fix renames the JS field
// to `journal_entry_ids` and treats Rust snake_case as the source of
// truth. This test pins the wire shape so a future rename of the JS field
// back to camelCase (or the Rust field to camelCase) cannot silently
// break pushes again.
describe('PushRequest wire shape', () => {
  it('uses snake_case journal_entry_ids (matching Rust)', () => {
    const req: PushRequest = {
      character_id: '00000000-0000-0000-0000-000000000001',
      target_id: '00000000-0000-0000-0000-000000000002',
      fields: ['ai_name'],
      chat_break: null,
      journal_entry_ids: ['je-1', 'je-2'],
    };
    const payload = JSON.parse(JSON.stringify(req));
    expect(payload).toHaveProperty('journal_entry_ids');
    expect(payload).not.toHaveProperty('journalEntryIds');
    expect(payload.journal_entry_ids).toEqual(['je-1', 'je-2']);
  });
});

// Regression tests for the H1 audit finding: every AppError variant
// (incl. the transparent Kindroid / Ai / Secret wrappers) must surface as
// a human-readable string, not raw JSON. Mirrors the variant list in
// src-tauri/src/error.rs (and the inner enums in kindroid/mod.rs,
// kindroid/ai.rs, security/secrets.rs).
describe('errorMessage', () => {
  // Tauri 2 wraps invoke rejections as `{ message: '<json>' }`.
  const wrapped = (json: string) => ({ message: json });

  it('passes through plain string rejections', () => {
    expect(errorMessage('boom')).toBe('boom');
  });

  it('passes through raw `e.message` for non-JSON rejections', () => {
    expect(errorMessage(wrapped('not a json string'))).toBe('not a json string');
  });

  it('falls back to String(e) for non-object values', () => {
    expect(errorMessage(42)).toBe('42');
    expect(errorMessage(null)).toBe('null');
  });

  it('maps AppError::NotFound', () => {
    expect(errorMessage(wrapped('{"kind":"not_found"}'))).toBe('Not found.');
  });

  it('maps AppError::Invalid', () => {
    expect(errorMessage(wrapped('{"kind":"invalid","message":"bad input"}'))).toBe(
      'Invalid input: bad input',
    );
    expect(errorMessage(wrapped('{"kind":"invalid"}'))).toBe('Invalid input: unknown');
  });

  it('maps AppError::NothingToPush / MissingGreeting / TokenMissing', () => {
    expect(errorMessage(wrapped('{"kind":"nothing_to_push"}'))).toContain('Nothing to push');
    expect(errorMessage(wrapped('{"kind":"missing_greeting"}'))).toContain('greeting is required');
    expect(errorMessage(wrapped('{"kind":"token_missing"}'))).toContain('No API token configured');
  });

  it('maps AppError::ShareCode / Database / Internal', () => {
    expect(errorMessage(wrapped('{"kind":"share_code","message":"bad png"}'))).toContain(
      'bad png',
    );
    expect(errorMessage(wrapped('{"kind":"database","message":"locked"}'))).toContain('locked');
    expect(errorMessage(wrapped('{"kind":"internal","message":"oops"}'))).toContain('oops');
  });

  it('maps AppError::SyncConflict (camelCase aiId from Tauri top-level rename)', () => {
    expect(errorMessage(wrapped('{"kind":"sync_conflict","aiId":"ai_x"}'))).toContain('ai_x');
  });

  it('maps SecretStoreError variants (nested inside AppError::Secret)', () => {
    expect(errorMessage(wrapped('{"kind":"secret","code":"unavailable"}'))).toContain(
      'keychain is not available',
    );
    expect(errorMessage(wrapped('{"kind":"secret","code":"not_found"}'))).toContain(
      'No token stored',
    );
    expect(errorMessage(wrapped('{"kind":"secret","code":"other","body":"disk full"}'))).toContain(
      'disk full',
    );
  });

  it('maps KindroidError variants (nested inside AppError::Kindroid)', () => {
    expect(errorMessage(wrapped('{"kind":"kindroid","code":"auth"}'))).toContain('API key');
    expect(
      errorMessage(wrapped('{"kind":"kindroid","code":"rate_limited","body":"slow down"}')),
    ).toContain('slow down');
    expect(
      errorMessage(wrapped('{"kind":"kindroid","code":"bad_request","body":"field x"}')),
    ).toContain('field x');
    expect(errorMessage(wrapped('{"kind":"kindroid","code":"server","body":"500"}'))).toContain(
      '500',
    );
    expect(errorMessage(wrapped('{"kind":"kindroid","code":"network","body":"timeout"}'))).toContain(
      '(network) timeout',
    );
  });

  it('maps AiError variants (nested inside AppError::Ai)', () => {
    expect(errorMessage(wrapped('{"kind":"ai","code":"auth","body":"bad key"}'))).toContain(
      'bad key',
    );
    expect(errorMessage(wrapped('{"kind":"ai","code":"network"}'))).toContain('(network)');
    expect(errorMessage(wrapped('{"kind":"ai","code":"decode","body":"json"}'))).toContain('json');
  });

  it('falls back to the raw message for unknown kinds', () => {
    const raw = '{"kind":"new_variant","detail":"surprise"}';
    expect(errorMessage(wrapped(raw))).toBe(raw);
  });
});
