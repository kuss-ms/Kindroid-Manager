import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';
import { defaultSelected, FieldChecklist } from './FieldChecklist';
import type { Character } from '../lib/types';

const full: Character = {
  id: 'id',
  name: 'Test',
  ai_name: 'Aria',
  ai_gender: 'Female',
  ai_backstory: 'b',
  ai_memory: null,
  ai_directive: null,
  ai_example_message: null,
  ai_additional_context: null,
  current_scene: null,
  greeting: null,
  notes: null,
  created_at: new Date().toISOString(),
  updated_at: new Date().toISOString(),
};

describe('FieldChecklist', () => {
  it('groups fields under "Kindroid" and disables empty ones', () => {
    const selected = defaultSelected(full);
    render(
      <FieldChecklist
        character={full}
        selected={selected}
        onChange={() => {}}
      />,
    );
    expect(screen.getByText(/Kindroid/)).toBeInTheDocument();
    expect(screen.getByTestId('field-ai_name')).not.toBeDisabled();
    expect(screen.getByTestId('field-ai_memory')).toBeDisabled();
  });
});
