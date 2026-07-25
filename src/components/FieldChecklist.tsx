import { useState } from 'react';
import { AI_FIELDS, PERSONA_FIELD_LABELS, type PersonaField } from '../lib/types';

interface FieldChecklistProps {
  character: import('../lib/types').Character;
  selected: Set<string>;
  onChange: (next: Set<string>) => void;
}

export function FieldChecklist({ character, selected, onChange }: FieldChecklistProps) {
  const toggle = (f: string) => {
    const next = new Set(selected);
    if (next.has(f)) next.delete(f);
    else next.add(f);
    onChange(next);
  };

  return (
    <Group
      title="Kindroid"
      fields={AI_FIELDS}
      character={character}
      selected={selected}
      onToggle={toggle}
    />
  );
}

function Group({
  title,
  fields,
  character,
  selected,
  onToggle,
}: {
  title: string;
  fields: readonly PersonaField[];
  character: import('../lib/types').Character;
  selected: Set<string>;
  onToggle: (f: string) => void;
}) {
  // Reserved for future groups (e.g., per-character custom fields). The
  // identity group was removed per product request — Kindroid doesn't
  // accept user_identity fields over the API anymore.
  void title;
  void useState;
  return (
    <fieldset className="fieldset" style={{ marginBottom: 12 }}>
      <legend>{title}</legend>
      {fields.map((f) => {
        const value = character[f as keyof typeof character] as string | null | undefined;
        const has = !!value && value.trim().length > 0;
        const isSel = selected.has(f);
        return (
          <label key={f} className="fieldset-row">
            <input
              type="checkbox"
              checked={isSel && has}
              disabled={!has}
              onChange={() => onToggle(f)}
              data-testid={`field-${f}`}
            />
            <span>{PERSONA_FIELD_LABELS[f]}</span>
            <span className="code">{f}</span>
            {!has && <span className="empty">(empty)</span>}
          </label>
        );
      })}
    </fieldset>
  );
}

export function defaultSelected(character: import('../lib/types').Character): Set<string> {
  const s = new Set<string>();
  for (const f of AI_FIELDS) {
    const v = character[f as keyof typeof character] as string | null | undefined;
    if (v && v.trim().length > 0) s.add(f);
  }
  return s;
}
