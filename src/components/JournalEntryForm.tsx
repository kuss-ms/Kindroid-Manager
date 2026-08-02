import { useState } from 'react';
import type { JournalEntryInput } from '../lib/types';

interface JournalEntryFormProps {
  initial: JournalEntryInput;
  onSubmit: (v: JournalEntryInput) => void;
  onCancel: () => void;
  submitting: boolean;
}

export function JournalEntryForm({
  initial,
  onSubmit,
  onCancel,
  submitting,
}: JournalEntryFormProps) {
  const [entry, setEntry] = useState(initial.entry);
  const [kpInput, setKpInput] = useState('');
  const [kps, setKps] = useState<string[]>(initial.keyphrases);
  const trimmedLen = entry.trim().length;
  const tooLong = trimmedLen > 500;
  const tooMany = kps.length > 8;

  const addKp = () => {
    const t = kpInput.trim();
    if (!t) return;
    if (kps.length >= 8) return;
    if (kps.some((k) => k.toLowerCase() === t.toLowerCase())) {
      setKpInput('');
      return;
    }
    setKps([...kps, t]);
    setKpInput('');
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
          onChange={(e) => setKpInput(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter' || e.key === ',') {
              e.preventDefault();
              addKp();
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
          onBlur={addKp}
          enterKeyHint="done"
          inputMode="text"
          autoCapitalize="off"
          autoCorrect="off"
          aria-label="Keyphrase"
          data-testid="keyphrase-input"
          disabled={kps.length >= 8}
        />
        <button
          type="button"
          className="btn btn-sm"
          onClick={addKp}
          disabled={kps.length >= 8 || kpInput.trim() === ''}
          aria-label="Add keyphrase"
          data-testid="keyphrase-add"
        >
          Add
        </button>
      </div>
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
        {kps.length} / 8
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
        {trimmedLen} / 500
      </div>

      <div style={{ display: 'flex', gap: 8, marginTop: 12 }}>
        <button
          type="button"
          className="btn btn-primary"
          disabled={submitting || tooLong || tooMany || trimmedLen === 0}
          onClick={() => onSubmit({ id: initial.id, entry, keyphrases: kps })}
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
