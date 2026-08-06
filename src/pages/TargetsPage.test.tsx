import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { MemoryRouter } from 'react-router-dom';
import { TargetsPage } from './TargetsPage';
import type { Character, Target } from '../lib/types';

vi.mock('../lib/api', () => ({
  api: {
    listTargets: vi.fn(),
    listCharacters: vi.fn(),
    saveTarget: vi.fn(),
    deleteTarget: vi.fn(),
    pushCreateNewKin: vi.fn(),
  },
  errorMessage: (e: unknown) => (e instanceof Error ? e.message : String(e)),
  isAndroid: () => false,
}));

import { api } from '../lib/api';

function renderTargets() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={queryClient}>
      <MemoryRouter>
        <TargetsPage />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

function target(id: string, label: string, ai_id: string): Target {
  return {
    id,
    ai_id,
    kind: 'ai',
    label,
    created_at: '2024-01-01T00:00:00Z',
  };
}

function character(id: string, default_target_id: string | null): Character {
  return {
    id,
    name: `C-${id}`,
    ai_name: null,
    ai_gender: null,
    ai_backstory: null,
    ai_memory: null,
    ai_directive: null,
    ai_example_message: null,
    ai_additional_context: null,
    current_scene: null,
    greeting: null,
    notes: null,
    default_target_id,
    created_at: '2024-01-01T00:00:00Z',
    updated_at: '2024-01-01T00:00:00Z',
  };
}

describe('TargetsPage default-count caption', () => {
  it('shows the caption when 2 characters point at the same target', async () => {
    vi.mocked(api.listTargets).mockResolvedValue([target('t1', 'Aria', 'ai_1')]);
    vi.mocked(api.listCharacters).mockResolvedValue([
      character('c1', 't1'),
      character('c2', 't1'),
    ]);

    renderTargets();

    await waitFor(() => {
      expect(screen.getByText(/Default for 2 characters/)).toBeInTheDocument();
    });
  });

  it('uses singular when exactly one character points at the target', async () => {
    vi.mocked(api.listTargets).mockResolvedValue([target('t1', 'Aria', 'ai_1')]);
    vi.mocked(api.listCharacters).mockResolvedValue([character('c1', 't1')]);

    renderTargets();

    await waitFor(() => {
      expect(screen.getByText(/Default for 1 character$/)).toBeInTheDocument();
    });
  });

  it('hides the caption when no character points at any target', async () => {
    vi.mocked(api.listTargets).mockResolvedValue([
      target('t1', 'Aria', 'ai_1'),
      target('t2', 'Bob', 'ai_2'),
    ]);
    vi.mocked(api.listCharacters).mockResolvedValue([character('c1', null)]);

    renderTargets();

    await waitFor(() => {
      expect(screen.getByText('Aria')).toBeInTheDocument();
    });
    expect(screen.queryByText(/Default for/)).not.toBeInTheDocument();
  });

  it('does not count a different target', async () => {
    vi.mocked(api.listTargets).mockResolvedValue([
      target('t1', 'Aria', 'ai_1'),
      target('t2', 'Bob', 'ai_2'),
    ]);
    vi.mocked(api.listCharacters).mockResolvedValue([character('c1', 't2')]);

    renderTargets();

    await waitFor(() => {
      expect(screen.getByText('Bob')).toBeInTheDocument();
    });
    expect(screen.getByText(/Default for 1 character$/)).toBeInTheDocument();
    // Aria row has no caption.
    const ariaRow = screen.getByText('Aria').closest('.list-item');
    expect(ariaRow).toBeTruthy();
    expect(ariaRow!.textContent).not.toMatch(/Default for/);
  });
});
