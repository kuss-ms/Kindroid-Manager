import { useState } from 'react';
import type { JournalEntryInput } from '../lib/types';

interface JournalEntryFormProps {
  initial: JournalEntryInput;
  onSubmit: (v: JournalEntryInput) => void;
  onCancel: () => void;
  submitting: boolean;
}

// Mirrors the Rust validator in `src-tauri/src/domain/journal_entry.rs`
// so the form rejects invalid input client-side instead of after a
// round-trip to the backend. Any change here must be kept in lock-step
// with `JournalEntry::validate` (and the tests in
// `JournalEntryForm.test.tsx` pin the shape).
export const MAX_ENTRY_CHARS = 500;
export const MAX_KEYPHRASES = 8;
export const MAX_KEYPHRASE_CHARS = 50;
export const MAX_KEYPHRASE_WORDS = 3;

export function validateKeyphrase(kp: string): string | null {
  const t = kp.trim();
  if (t.length === 0) return 'keyphrase must not be empty';
  if ([...t].length > MAX_KEYPHRASE_CHARS) {
    return `keyphrase must be ${MAX_KEYPHRASE_CHARS} characters or fewer`;
  }
  if (t.includes(',') || t.includes(';') || t.includes(':')) {
    return 'keyphrase must not contain separators (no commas, colons, or semicolons)';
  }
  if (t.split(/\s+/).filter(Boolean).length > MAX_KEYPHRASE_WORDS) {
    return `keyphrase must be ${MAX_KEYPHRASE_WORDS} words or fewer`;
  }
  return null;
}

export function JournalEntryForm({
  initial,
  onSubmit,
  onCancel,
  submitting,
}: JournalEntryFormProps) {
  const [entry, setEntry] = useState(initial.entry);
  const [kpInput, setKpInput] = useState('');
  const [kpError, setKpError] = useState<string | null>(null);
  const [kps, setKps] = useState<string[]>(initial.keyphrases);
  const trimmedLen = entry.trim().length;
  const tooLong = trimmedLen > MAX_ENTRY_CHARS;
  const tooMany = kps.length > MAX_KEYPHRASES;

  const tryAddKp = () => {
    const t = kpInput.trim();
    if (!t) return false;
    if (kps.length >= MAX_KEYPHRASES) {
      setKpError(`at most ${MAX_KEYPHRASES} keyphrases`);
      return false;
    }
    const err = validateKeyphrase(t);
    if (err) {
      setKpError(err);
      return false;
    }
    if (kps.some((k) => k.toLowerCase() === t.toLowerCase())) {
      setKpInput('');
      setKpError(null);
      return false;
    }
    setKps([...kps, t]);
    setKpInput('');
    setKpError(null);
    return true;
  };

  const submit = () => {
    if (tooLong || tooMany || trimmedLen === 0) return;
    onSubmit({ id: initial.id, entry, keyphrases: kps });
  };

  return (
    <div
      style={{
        border: '1px solid var(--border)',
        padding: 12,
        borderRadius: 6,
        marginTop: 12,
      }}
    >
      <div className="keyphrase-input-row">
        <input
          className="input"
          placeholder="Type a keyphrase and tap Add (or press Enter)"
          value={kpInput}
          onChange={(e) => {
            setKpInput(e.target.value);
            if (kpError) setKpError(null);
          }}
          onKeyDown={(e) => {
            if (e.key === 'Enter' || e.key === ',') {
              e.preventDefault();
              tryAddKp();
            } else if (e.key === 'Backspace' && kpInput === '' && kps.length > 0) {
              setKps(kps.slice(0, -1));
            }
          }}
          // The Android virtual keyboard's "Next" arrow steals focus to
          // the next field without firing `keydown` for `Enter`, which
          // silently drops the pending keyphrase. `onBlur` flushes the
          // buffer in that case. `enterKeyHint="done"` makes the
          // keyboard's action button read "Done" instead of "Next" on
          // Android/iOS — a better affordance for the input's intent.
          onBlur={() => {
            if (kpInput.trim() !== '') tryAddKp();
          }}
          enterKeyHint="done"
          inputMode="text"
          autoCapitalize="off"
          autoCorrect="off"
          aria-label="Keyphrase"
          aria-invalid={kpError !== null}
          data-testid="keyphrase-input"
          disabled={kps.length >= MAX_KEYPHRASES}
          style={kpError ? { borderColor: 'var(--danger)' } : undefined}
        />
        <button
          type="button"
          className="btn btn-sm"
          onClick={tryAddKp}
          disabled={kps.length >= MAX_KEYPHRASES || kpInput.trim() === ''}
          aria-label="Add keyphrase"
          data-testid="keyphrase-add"
        >
          Add
        </button>
      </div>
      {kpError && (
        <div
          className="keyphrase-error"
          role="alert"
          data-testid="keyphrase-error"
          style={{ color: 'var(--danger)', fontSize: 12, marginTop: 4 }}
        >
          {kpError}
        </div>
      )}
      <div style={{ display: 'flex', flexWrap: 'wrap', gap: 4, marginTop: 6 }}>
        {kps.map((k, i) => (
          <span key={`${k}-${i}`} className="badge badge-muted">
            {k}
            <button
              type="button"
              className="btn btn-sm"
              style={{ marginLeft: 4, padding: 0, border: 'none', background: 'none' }}
              onClick={() => setKps(kps.filter((_, idx) => idx !== i))}
              aria-label={`Remove ${k}`}
            >
              ×
            </button>
          </span>
        ))}
      </div>
      <div
        className={`soft-counter ${tooMany ? 'warn' : ''}`}
        style={tooMany ? { color: 'var(--danger)' } : undefined}
      >
        {kps.length} / {MAX_KEYPHRASES}
      </div>

      <textarea
        className="textarea"
        rows={4}
        value={entry}
        onChange={(e) => setEntry(e.target.value)}
        placeholder="Write the journal entry…"
        style={tooLong ? { borderColor: 'var(--danger)' } : undefined}
      />
      <div
        className={`soft-counter ${tooLong ? 'warn' : ''}`}
        style={tooLong ? { color: 'var(--danger)' } : undefined}
      >
        {trimmedLen} / {MAX_ENTRY_CHARS}
      </div>

      <div style={{ display: 'flex', gap: 8, marginTop: 12 }}>
        <button
          type="button"
          className="btn btn-primary"
          disabled={submitting || tooLong || tooMany || trimmedLen === 0}
          onClick={submit}
        >
          {submitting ? 'Saving…' : initial.id ? 'Update entry' : 'Save entry'}
        </button>
        <button type="button" className="btn" onClick={onCancel}>
          Cancel
        </button>
      </div>
    </div>
  );
}
