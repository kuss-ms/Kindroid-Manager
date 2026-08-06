import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import { CharacterEditorPage, PushFieldButton } from './CharacterEditorPage';
import type { Character, Target } from '../lib/types';

vi.mock('../lib/api', () => ({
  api: {
    getCharacter: vi.fn(),
    listTargets: vi.fn(),
    saveCharacter: vi.fn(),
    deleteCharacter: vi.fn(),
    duplicateCharacter: vi.fn(),
    setCharacterImage: vi.fn(),
    getCharacterImage: vi.fn(),
    exportShareImage: vi.fn(),
    copyShareImageToClipboard: vi.fn(),
    listJournalEntries: vi.fn(),
    saveJournalEntry: vi.fn(),
    deleteJournalEntry: vi.fn(),
    listCharacterRevisions: vi.fn(),
    pushToTarget: vi.fn(),
  },
  errorMessage: (e: unknown) => (e instanceof Error ? e.message : String(e)),
  isAndroid: () => false,
}));

import { api } from '../lib/api';

function renderEditor(id: string | 'new') {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  const path = id === 'new' ? '/characters/new' : `/characters/${id}`;
  return render(
    <QueryClientProvider client={queryClient}>
      <MemoryRouter initialEntries={[path]}>
        <Routes>
          <Route path="/characters/:id" element={<CharacterEditorPage />} />
          <Route path="/characters/new" element={<CharacterEditorPage />} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

function target(id: string, label: string, ai_id: string, kind: 'ai' | 'group' = 'ai'): Target {
  return { id, ai_id, kind, label, created_at: '2024-01-01T00:00:00Z' };
}

function character(id: string, default_target_id: string | null): Character {
  return {
    id,
    name: 'Test',
    ai_name: 'Aria',
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

describe('CharacterEditorPage default-target select', () => {
  it('falls back to "— none —" before targets load', async () => {
    const cid = '00000000-0000-0000-0000-000000000001';
    const tid = '00000000-0000-0000-0000-000000000010';
    // Resolve character immediately; keep targets pending forever.
    vi.mocked(api.getCharacter).mockResolvedValue(character(cid, tid));
    let resolveTargets: (v: Target[]) => void = () => {};
    vi.mocked(api.listTargets).mockReturnValue(new Promise<Target[]>((res) => { resolveTargets = res; }));
    vi.mocked(api.listJournalEntries).mockResolvedValue([]);

    renderEditor(cid);

    await waitFor(() => {
      expect(screen.getByTestId('default-target-select')).toBeInTheDocument();
    });
    // Targets still loading → select is forced to "" (not the tid, which isn't in the option list yet).
    const select = screen.getByTestId('default-target-select') as HTMLSelectElement;
    expect(select.value).toBe('');

    // Once targets resolve, the select snaps to the default target.
    resolveTargets([target(tid, 'Aria', 'ai_1')]);
    await waitFor(() => {
      expect((screen.getByTestId('default-target-select') as HTMLSelectElement).value).toBe(tid);
    });
  });

  it('does not list group targets as defaults', async () => {
    const cid = '00000000-0000-0000-0000-000000000001';
    const aiId = '00000000-0000-0000-0000-000000000010';
    const groupId = '00000000-0000-0000-0000-000000000020';
    vi.mocked(api.getCharacter).mockResolvedValue(character(cid, null));
    vi.mocked(api.listTargets).mockResolvedValue([
      target(aiId, 'Aria', 'ai_1', 'ai'),
      target(groupId, 'Group', 'grp_1', 'group'),
    ]);
    vi.mocked(api.listJournalEntries).mockResolvedValue([]);

    renderEditor(cid);

    const select = (await waitFor(() => screen.getByTestId('default-target-select'))) as HTMLSelectElement;
    // Targets must resolve before the AI option appears.
    await waitFor(() => {
      const values = Array.from((screen.getByTestId('default-target-select') as HTMLSelectElement).options).map(
        (o) => o.value,
      );
      expect(values).toContain(aiId);
    });
    const optionValues = Array.from(select.options).map((o) => o.value);
    expect(optionValues).toContain(aiId);
    expect(optionValues).not.toContain(groupId);
  });});

describe('PushFieldButton', () => {
  it('is enabled and invokes onPush when value + target are set', async () => {
    const onPush = vi.fn();
    render(
      <PushFieldButton
        field="ai_name"
        value="Aria"
        defaultTargetLabel="Aria"
        busy={false}
        onPush={onPush}
      />,
    );
    const btn = screen.getByTestId('push-field-ai_name');
    expect(btn).not.toBeDisabled();
    fireEvent.click(btn);
    expect(onPush).toHaveBeenCalledTimes(1);
  });

  it('is disabled with a helpful tooltip when the field is empty', () => {
    render(
      <PushFieldButton
        field="ai_name"
        value=""
        defaultTargetLabel="Aria"
        busy={false}
        onPush={() => {}}
      />,
    );
    const btn = screen.getByTestId('push-field-ai_name');
    expect(btn).toBeDisabled();
    expect(btn.getAttribute('title')).toMatch(/empty/);
  });

  it('is disabled with the default-target tooltip when no target is set', () => {
    render(
      <PushFieldButton
        field="ai_name"
        value="Aria"
        defaultTargetLabel={null}
        busy={false}
        onPush={() => {}}
      />,
    );
    const btn = screen.getByTestId('push-field-ai_name');
    expect(btn).toBeDisabled();
    expect(btn.getAttribute('title')).toMatch(/default push target/);
  });

  it('is disabled while busy, regardless of other props', () => {
    const onPush = vi.fn();
    render(
      <PushFieldButton
        field="ai_name"
        value="Aria"
        defaultTargetLabel="Aria"
        busy={true}
        onPush={onPush}
      />,
    );
    const btn = screen.getByTestId('push-field-ai_name');
    expect(btn).toBeDisabled();
  });
});
