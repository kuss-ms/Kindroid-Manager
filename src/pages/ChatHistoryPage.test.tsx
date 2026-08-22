import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import { ChatHistoryPage } from './ChatHistoryPage';
import type { Target } from '../lib/types';

// The Tauri `listen` API doesn't work in jsdom. Stub it with a noop
// returning an unlisten handle that never fires.
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

// jsdom doesn't implement scrollIntoView; the ChatView auto-scroll
// effect would otherwise throw on the sentinel ref.
if (!HTMLElement.prototype.scrollIntoView) {
  HTMLElement.prototype.scrollIntoView = function () {
    /* no-op in jsdom */
  };
}

vi.mock('../lib/api', () => ({
  api: {
    listTargets: vi.fn(),
    listChatMessages: vi.fn(),
    searchChat: vi.fn(),
    chatMessageCount: vi.fn(),
    getChatSyncState: vi.fn(),
    getCurrentSync: vi.fn(),
    getChatAutomationState: vi.fn(),
    startChatSync: vi.fn(),
    cancelChatSync: vi.fn(),
    resetChatHistory: vi.fn(),
    setChatMessageFavourite: vi.fn(),
    sendChatMessage: vi.fn(),
    rewindChat: vi.fn(),
    suggestChatUserMessage: vi.fn(),
  },
  errorMessage: (e: unknown) => (e instanceof Error ? e.message : String(e)),
  isAndroid: () => false,
}));

import { api } from '../lib/api';

function renderPage(initialPath: string) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={queryClient}>
      <MemoryRouter initialEntries={[initialPath]}>
        <Routes>
          <Route path="/chat-history" element={<ChatHistoryPage />} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

function target(aiId: string, kind: 'ai' | 'group'): Target {
  return {
    id: `id-${aiId}-${kind}`,
    ai_id: aiId,
    kind,
    label: `${aiId}-${kind}`,
    created_at: '2024-01-01T00:00:00Z',
  };
}

describe('ChatHistoryPage', () => {
  it('defaults to chat view for an AI target', async () => {
    const t = target('ai_x', 'ai');
    vi.mocked(api.listTargets).mockResolvedValue([t]);
    vi.mocked(api.listChatMessages).mockResolvedValue([]);
    vi.mocked(api.chatMessageCount).mockResolvedValue(0);
    vi.mocked(api.getChatSyncState).mockResolvedValue(null);
    vi.mocked(api.getCurrentSync).mockResolvedValue(null);
    vi.mocked(api.getChatAutomationState).mockResolvedValue(null as never);
    renderPage(`/chat-history?ai_id=${t.ai_id}&kind=${t.kind}`);

    await waitFor(() => {
      expect(screen.getByTestId('view-segmented')).toBeTruthy();
    });
    const chatTab = screen.getByRole('tab', { name: 'Chat' });
    expect(chatTab.getAttribute('aria-selected')).toBe('true');
    // ChatView should render.
    expect(screen.getByTestId('chat-view')).toBeTruthy();
  });

  it('hides the segmented control for a group target', async () => {
    const t = target('gc_1', 'group');
    vi.mocked(api.listTargets).mockResolvedValue([t]);
    vi.mocked(api.listChatMessages).mockResolvedValue([]);
    vi.mocked(api.chatMessageCount).mockResolvedValue(0);
    vi.mocked(api.getChatSyncState).mockResolvedValue(null);
    vi.mocked(api.getCurrentSync).mockResolvedValue(null);
    vi.mocked(api.getChatAutomationState).mockResolvedValue(null as never);
    renderPage(`/chat-history?ai_id=${t.ai_id}&kind=${t.kind}`);

    await waitFor(() => {
      expect(screen.queryByTestId('view-segmented')).toBeNull();
    });
    // ChatView must NOT render for group targets.
    expect(screen.queryByTestId('chat-view')).toBeNull();
  });

  it('Send is disabled when the textarea is empty', async () => {
    const t = target('ai_x', 'ai');
    vi.mocked(api.listTargets).mockResolvedValue([t]);
    vi.mocked(api.listChatMessages).mockResolvedValue([]);
    vi.mocked(api.chatMessageCount).mockResolvedValue(0);
    vi.mocked(api.getChatSyncState).mockResolvedValue(null);
    vi.mocked(api.getCurrentSync).mockResolvedValue(null);
    vi.mocked(api.getChatAutomationState).mockResolvedValue(null as never);
    renderPage(`/chat-history?ai_id=${t.ai_id}&kind=${t.kind}`);

    const send = await waitFor(() => screen.getByTestId('chat-send'));
    expect(send).toBeDisabled();
  });
});
