import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useEffect, useMemo, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { api, escapeFtsQuery, errorMessage } from '../lib/api';
import type { ChatMessage, ChatSyncState, SyncStatusKind, Target } from '../lib/types';
import { ConfirmDialog } from '../components/ConfirmDialog';
import { toast } from '../components/Toaster';

const PAGE_SIZE = 50;
const SEARCH_LIMIT = 200;

interface LiveProgress {
  ai_id: string;
  total: number;
  requests: number;
  last_batch_size: number;
  last_batch_had_messages: boolean;
  last_timestamp: number;
  status_kind: SyncStatusKind;
  status_message: string | null;
  full_sync_done: boolean;
  received_at: number;
}

function relativeTime(iso: string | null | undefined): string {
  if (!iso) return 'never';
  const then = new Date(iso).getTime();
  const now = Date.now();
  const diff = Math.max(0, now - then);
  const s = Math.floor(diff / 1000);
  if (s < 60) return `${s}s ago`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m ago`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ago`;
  const d = Math.floor(h / 24);
  return `${d}d ago`;
}

function localTime(iso: string | null | undefined): string {
  if (!iso) return '';
  return new Date(iso).toLocaleString();
}

function formatCountdown(iso: string | null | undefined): string {
  if (!iso) return '';
  const until = new Date(iso).getTime();
  const diff = Math.max(0, until - Date.now());
  const m = Math.floor(diff / 60000);
  const s = Math.floor((diff % 60000) / 1000);
  return `${m}m ${s.toString().padStart(2, '0')}s`;
}

function tsToLocal(ts: number): string {
  if (!ts) return '—';
  return new Date(ts).toLocaleString();
}

export function ChatHistoryPage() {
  const [params, setParams] = useSearchParams();
  const queryClient = useQueryClient();

  const targets = useQuery<Target[]>({
    queryKey: ['targets'],
    queryFn: api.listTargets,
  });
  const current = useQuery<string | null>({
    queryKey: ['current-sync'],
    queryFn: api.getCurrentSync,
    refetchInterval: 5000,
  });

  // Selected ai_id: prefer URL param, else first target's ai_id.
  const targetsList = useMemo(() => targets.data ?? [], [targets.data]);
  const selectedAiId = useMemo(() => {
    const fromUrl = params.get('ai_id');
    if (fromUrl && targetsList.some((t) => t.ai_id === fromUrl)) return fromUrl;
    if (targetsList.length === 1) return targetsList[0].ai_id;
    return fromUrl;
  }, [params, targetsList]);

  function setSelectedAiId(aiId: string) {
    const next = new URLSearchParams(params);
    if (aiId) next.set('ai_id', aiId);
    else next.delete('ai_id');
    setParams(next, { replace: true });
  }

  // Search state.
  const [searchInput, setSearchInput] = useState('');
  const [debouncedQuery, setDebouncedQuery] = useState('');
  useEffect(() => {
    const handle = setTimeout(() => setDebouncedQuery(searchInput), 200);
    return () => clearTimeout(handle);
  }, [searchInput]);

  // Browse pagination: cursor is the smallest timestamp currently shown;
  // next page reads older by setting beforeTs to that.
  const [browseOffset, setBrowseOffset] = useState(0);
  useEffect(() => {
    setBrowseOffset(0);
  }, [selectedAiId, debouncedQuery]);

  const trimmedQuery = debouncedQuery.trim();
  const isSearching = trimmedQuery.length > 0;

  // Live progress payload from the backend (refreshed via events).
  const [liveProgress, setLiveProgress] = useState<LiveProgress | null>(null);
  useEffect(() => {
    // Reset when target changes so we don't show stale progress from a
    // previous sync.
    setLiveProgress(null);
  }, [selectedAiId]);

  // Sync state (5 s polling even when navigated away).
  const syncState = useQuery<ChatSyncState | null>({
    queryKey: ['chat-sync-state', selectedAiId],
    queryFn: () => (selectedAiId ? api.getChatSyncState(selectedAiId) : Promise.resolve(null)),
    enabled: !!selectedAiId,
    refetchInterval: 5000,
  });
  const messageCount = useQuery<number | null>({
    queryKey: ['chat-message-count', selectedAiId],
    queryFn: () => (selectedAiId ? api.chatMessageCount(selectedAiId) : Promise.resolve(null)),
    enabled: !!selectedAiId,
    refetchInterval: 5000,
  });

  // Page of messages (browse mode).
  const browsePage = useQuery<ChatMessage[]>({
    queryKey: ['chat-messages', selectedAiId, browseOffset],
    queryFn: () => {
      if (!selectedAiId) return Promise.resolve([]);
      return api.listChatMessages(
        selectedAiId,
        browseOffset === 0 ? null : browseOffset,
        PAGE_SIZE,
      );
    },
    enabled: !!selectedAiId && !isSearching,
  });

  // Search results.
  const searchPage = useQuery<ChatMessage[]>({
    queryKey: ['chat-search', selectedAiId, trimmedQuery, browseOffset],
    queryFn: () => {
      if (!selectedAiId || !trimmedQuery) return Promise.resolve([]);
      return api.searchChat(
        selectedAiId,
        escapeFtsQuery(trimmedQuery),
        PAGE_SIZE,
        browseOffset,
      );
    },
    enabled: !!selectedAiId && isSearching,
  });

  // Subscribe to backend events. The progress event carries the
  // request count + last batch size so we can render a live indicator.
  // We also invalidate the messages list so newly-fetched rows appear
  // without a manual refresh.
  useEffect(() => {
    const unlistens: Array<Promise<UnlistenFn>> = [];
    unlistens.push(
      listen<{
        ai_id: string;
        total: number;
        last_timestamp: number;
        full_sync_done: boolean;
        status_kind: SyncStatusKind;
        status_message: string | null;
        requests: number;
        last_batch_size: number;
        last_batch_had_messages: boolean;
      }>('chat-sync-progress', (event) => {
        const p = event.payload;
        if (selectedAiId && p.ai_id !== selectedAiId) return;
        setLiveProgress({ ...p, received_at: Date.now() });
        queryClient.invalidateQueries({ queryKey: ['chat-sync-state'] });
        queryClient.invalidateQueries({ queryKey: ['chat-message-count'] });
        queryClient.invalidateQueries({ queryKey: ['current-sync'] });
        // Refresh the visible messages so newly-fetched rows appear.
        queryClient.invalidateQueries({ queryKey: ['chat-messages'] });
        queryClient.invalidateQueries({ queryKey: ['chat-search'] });
      }),
    );
    unlistens.push(
      listen<{
        ai_id: string;
        total: number;
        status_kind: SyncStatusKind;
        status_message: string | null;
        requests: number;
      }>('chat-sync-complete', (event) => {
        const p = event.payload;
        if (selectedAiId && p.ai_id !== selectedAiId) return;
        setLiveProgress({
          ai_id: p.ai_id,
          total: p.total,
          last_timestamp: 0,
          full_sync_done: true,
          status_kind: p.status_kind,
          status_message: p.status_message,
          requests: p.requests,
          last_batch_size: 0,
          last_batch_had_messages: false,
          received_at: Date.now(),
        });
        queryClient.invalidateQueries({ queryKey: ['chat-sync-state'] });
        queryClient.invalidateQueries({ queryKey: ['chat-message-count'] });
        queryClient.invalidateQueries({ queryKey: ['current-sync'] });
        queryClient.invalidateQueries({ queryKey: ['chat-messages'] });
        queryClient.invalidateQueries({ queryKey: ['chat-search'] });
      }),
    );
    return () => {
      unlistens.forEach((p) => p.then((u) => u()).catch(() => {}));
    };
  }, [queryClient, selectedAiId]);

  async function onSync() {
    if (!selectedAiId) return;
    try {
      await api.startChatSync(selectedAiId);
      setLiveProgress({
        ai_id: selectedAiId,
        total: syncState.data?.total ?? 0,
        last_timestamp: 0,
        full_sync_done: false,
        status_kind: 'running',
        status_message: null,
        requests: 0,
        last_batch_size: 0,
        last_batch_had_messages: false,
        received_at: Date.now(),
      });
      queryClient.invalidateQueries({ queryKey: ['chat-sync-state'] });
      queryClient.invalidateQueries({ queryKey: ['current-sync'] });
    } catch (e) {
      toast('error', errorMessage(e));
    }
  }

  async function onCancel() {
    try {
      await api.cancelChatSync();
      queryClient.invalidateQueries({ queryKey: ['chat-sync-state'] });
      queryClient.invalidateQueries({ queryKey: ['current-sync'] });
    } catch (e) {
      toast('error', errorMessage(e));
    }
  }

  // Reset confirmation flow: hold the open flag until the user confirms
  // or dismisses. We also disable the button while a sync is running on
  // this target since the reset would race with the in-flight loop.
  const [resetOpen, setResetOpen] = useState(false);
  const [resetting, setResetting] = useState(false);
  async function onResetConfirm() {
    if (!selectedAiId) return;
    setResetting(true);
    try {
      const deleted = await api.resetChatHistory(selectedAiId);
      // Wipe the live progress hint so the UI doesn't show a stale
      // request count from the previous run.
      setLiveProgress(null);
      queryClient.invalidateQueries({ queryKey: ['chat-sync-state'] });
      queryClient.invalidateQueries({ queryKey: ['chat-message-count'] });
      queryClient.invalidateQueries({ queryKey: ['chat-messages'] });
      queryClient.invalidateQueries({ queryKey: ['chat-search'] });
      queryClient.invalidateQueries({ queryKey: ['current-sync'] });
      const n = deleted === 1 ? '1 message' : `${deleted} messages`;
      toast('success', `Cleared ${n} for ${selectedAiId}.`);
    } catch (e) {
      toast('error', errorMessage(e));
    } finally {
      setResetting(false);
      setResetOpen(false);
    }
  }

  // Modal state for the full-message view.
  const [openMessage, setOpenMessage] = useState<ChatMessage | null>(null);

  if (targets.isLoading) {
    return (
      <div className="page">
        <div className="page-header">
          <h2>Chat History</h2>
        </div>
        <p className="muted">Loading…</p>
      </div>
    );
  }

  // No targets at all.
  if (targetsList.length === 0) {
    return (
      <div className="page">
        <div className="page-header">
          <h2>Chat History</h2>
        </div>
        <div className="empty">
          Add a target on the Targets page to enable chat history.
        </div>
      </div>
    );
  }

  const currentSyncing = current.data;
  const state = syncState.data ?? null;
  const statusKind: SyncStatusKind =
    liveProgress?.status_kind ?? state?.status_kind ?? 'idle';
  const total = liveProgress?.total ?? state?.total ?? messageCount.data ?? 0;

  // Build the progress indicator subtitle. During a sync we combine the
  // request count + last-batch timestamp so the user can see whether the
  // backfill is making progress.
  function progressSubtitle(): string {
    if (currentSyncing === selectedAiId && liveProgress) {
      const reqPart = liveProgress.requests > 0 ? `Request #${liveProgress.requests}` : 'Starting…';
      const cursorPart = liveProgress.last_timestamp
        ? `Last message: ${tsToLocal(liveProgress.last_timestamp)}`
        : 'Awaiting first page…';
      return `Syncing… · ${reqPart} · ${cursorPart}`;
    }
    if (currentSyncing && currentSyncing !== selectedAiId) {
      return `Sync in progress for ${currentSyncing}`;
    }
    if (state == null) return 'Last synced: never';
    if (statusKind === 'backoff') {
      return `Paused until ${formatCountdown(state.backoff_until)} (rate limit)`;
    }
    if (statusKind === 'error') return `Error: ${state.status_message ?? 'unknown'}`;
    if (statusKind === 'cancelled') return `Stopped at ${localTime(state.last_synced_at)}`;
    if (state.full_sync_done && total === 0) {
      return `Last synced: ${relativeTime(state.last_synced_at)}`;
    }
    return `Last synced: ${relativeTime(state.last_synced_at)} · ${total} messages`;
  }

  // Pick header subtitle, action area, body text per the state machine.
  const subtitle = progressSubtitle();
  let showSync = false;
  let showCancel = false;
  let body: string | null = null;
  let syncDisabledReason: string | null = null;

  if (currentSyncing === selectedAiId) {
    showCancel = true;
    if (liveProgress && liveProgress.last_batch_size > 0) {
      body = `Latest page returned ${liveProgress.last_batch_size} message${liveProgress.last_batch_size === 1 ? '' : 's'}.`;
    } else {
      body = `Last updated: ${localTime(state?.last_synced_at) || '—'}`;
    }
  } else if (currentSyncing && currentSyncing !== selectedAiId) {
    syncDisabledReason = `Cancel it before syncing this one.`;
    body = `Cancel it from its target page before syncing this one.`;
  } else if (state == null) {
    showSync = true;
    body = 'Click Sync to fetch history from Kindroid.';
  } else if (statusKind === 'backoff') {
    showCancel = true;
    body = 'Waiting for rate-limit window to reopen.';
  } else if (statusKind === 'error') {
    showSync = true;
    body = 'Last sync failed.';
  } else if (statusKind === 'cancelled') {
    showSync = true;
    body = 'Sync stopped. Cursor preserved — click Sync to resume.';
  } else if (state.full_sync_done && total === 0) {
    showSync = true;
    body = 'No messages on Kindroid for this AI.';
  } else {
    showSync = true;
  }

  const activeList = isSearching ? searchPage : browsePage;
  const messages = activeList.data ?? [];

  return (
    <div className="page">
      <div className="page-header">
        <h2>Chat History</h2>
        <div className="muted">{subtitle}</div>
      </div>

      <div className="flex-row" style={{ gap: 8, flexWrap: 'wrap', alignItems: 'center' }}>
        <label className="muted" htmlFor="target-select">
          Target
        </label>
        <select
          id="target-select"
          value={selectedAiId ?? ''}
          onChange={(e) => setSelectedAiId(e.target.value)}
        >
          {targetsList.map((t) => (
            <option key={t.id} value={t.ai_id}>
              {t.label} ({t.ai_id})
            </option>
          ))}
        </select>
        <div style={{ flex: 1 }} />
        {showSync && (
          <button
            className="btn btn-primary"
            disabled={!!syncDisabledReason}
            title={syncDisabledReason ?? ''}
            onClick={onSync}
          >
            Sync
          </button>
        )}
        {showCancel && (
          <button className="btn" onClick={onCancel}>
            Cancel
          </button>
        )}
        {/* Reset is available whenever a target is selected, except
            while a sync is running for this target (the wipe would race
            with the in-flight loop). */}
        <button
          className="btn btn-danger"
          onClick={() => setResetOpen(true)}
          disabled={resetting || currentSyncing === selectedAiId}
          title={
            currentSyncing === selectedAiId
              ? 'Cancel the sync before resetting.'
              : 'Delete all locally-cached chat history for this target.'
          }
        >
          Reset
        </button>
      </div>

      {body && <p className="muted">{body}</p>}

      <div className="flex-row" style={{ marginTop: 12 }}>
        <input
          type="search"
          placeholder="Search messages…"
          value={searchInput}
          onChange={(e) => setSearchInput(e.target.value)}
          style={{ flex: 1, minWidth: 200 }}
        />
      </div>

      {isSearching && (
        <p className="muted" style={{ marginTop: 8 }}>
          {messages.length === 0
            ? `No matches for "${trimmedQuery}".`
            : `Showing ${messages.length} matches (prefix search, Porter stemmed).`}
        </p>
      )}

      <div style={{ marginTop: 12 }}>
        {messages.map((m) => (
          <MessageRow
            key={m.id}
            message={m}
            query={trimmedQuery}
            onOpen={() => setOpenMessage(m)}
          />
        ))}
        {messages.length === 0 && !activeList.isLoading && (
          <div className="empty">
            {isSearching ? 'No messages match your search.' : 'No messages yet.'}
          </div>
        )}
      </div>

      <div className="flex-row" style={{ marginTop: 12 }}>
        <button
          className="btn"
          disabled={browseOffset === 0}
          onClick={() => setBrowseOffset(Math.max(0, browseOffset - PAGE_SIZE))}
        >
          ← {isSearching ? 'Prev' : 'Newer'}
        </button>
        <button
          className="btn"
          disabled={messages.length < PAGE_SIZE || browseOffset + PAGE_SIZE >= SEARCH_LIMIT}
          onClick={() => setBrowseOffset(browseOffset + PAGE_SIZE)}
        >
          {isSearching ? 'Next' : 'Older'} →
        </button>
      </div>

      <MessageDetailDialog
        message={openMessage}
        onClose={() => setOpenMessage(null)}
      />

      <ConfirmDialog
        open={resetOpen}
        title={`Reset chat history for ${selectedAiId ?? ''}?`}
        body="This deletes every locally-cached message and the sync cursor for this target. The next Sync will re-fetch the full history from Kindroid. Your Kindroid account data is not affected."
        confirmLabel="Reset"
        cancelLabel="Cancel"
        onConfirm={onResetConfirm}
        onCancel={() => setResetOpen(false)}
      />
    </div>
  );
}

function MessageRow({
  message,
  query,
  onOpen,
}: {
  message: ChatMessage;
  query: string;
  onOpen: () => void;
}) {
  const when = new Date(message.timestamp).toLocaleString();
  const who = message.display_name || message.sender;
  const snippet = makeSnippet(message.message, query);
  return (
    <div
      className="chat-row"
      role="button"
      tabIndex={0}
      onClick={onOpen}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          onOpen();
        }
      }}
      style={{
        padding: '8px 0',
        borderBottom: '1px solid var(--border)',
        cursor: 'pointer',
      }}
    >
      <div style={{ display: 'flex', gap: 8, alignItems: 'baseline' }}>
        <strong>{who}</strong>
        <span className="muted" style={{ fontSize: 12 }}>
          {when}
        </span>
      </div>
      <div style={{ marginTop: 2 }}>{snippet}</div>
      {message.image_urls.length > 0 && (
        <div className="muted" style={{ fontSize: 12 }}>
          🖼 {message.image_urls.length} image{message.image_urls.length === 1 ? '' : 's'}
        </div>
      )}
      {message.link_url && (
        <div style={{ fontSize: 12 }}>
          🔗 <a href={message.link_url}>{message.link_description ?? message.link_url}</a>
        </div>
      )}
    </div>
  );
}

function MessageDetailDialog({
  message,
  onClose,
}: {
  message: ChatMessage | null;
  onClose: () => void;
}) {
  useEffect(() => {
    if (!message) return;
    function onKey(e: KeyboardEvent) {
      if (e.key === 'Escape') onClose();
    }
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [message, onClose]);

  if (!message) return null;

  const when = new Date(message.timestamp).toLocaleString();
  const who = message.display_name || message.sender;
  const fetched = localTime(message.fetched_at);

  return (
    <div
      className="modal-overlay"
      role="dialog"
      aria-modal="true"
      aria-label={`Message from ${who}`}
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className="modal" style={{ maxWidth: 720, maxHeight: '85vh', overflow: 'auto' }}>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'baseline' }}>
          <h3 style={{ marginBottom: 0 }}>{who}</h3>
          <button className="btn btn-sm" onClick={onClose} aria-label="Close">
            ✕
          </button>
        </div>
        <div className="muted" style={{ fontSize: 12, marginTop: 4 }}>
          {when} · {message.sender_type}
        </div>

        <div style={{ whiteSpace: 'pre-wrap', marginTop: 16, lineHeight: 1.5 }}>
          {message.message || <span className="muted">(empty message)</span>}
        </div>

        {message.image_urls.length > 0 && (
          <section style={{ marginTop: 16 }}>
            <h4 style={{ marginBottom: 4 }}>Images</h4>
            <ul style={{ margin: 0, paddingLeft: 18 }}>
              {message.image_urls.map((url, i) => (
                <li key={i}>
                  <a href={url} target="_blank" rel="noopener noreferrer">
                    {url}
                  </a>
                </li>
              ))}
            </ul>
            {message.image_description && (
              <p className="muted" style={{ marginTop: 4 }}>
                {message.image_description}
              </p>
            )}
          </section>
        )}

        {message.video_description && (
          <section style={{ marginTop: 16 }}>
            <h4 style={{ marginBottom: 4 }}>Video</h4>
            <p className="muted">{message.video_description}</p>
          </section>
        )}

        {message.internet_response && (
          <section style={{ marginTop: 16 }}>
            <h4 style={{ marginBottom: 4 }}>Internet response</h4>
            <p style={{ whiteSpace: 'pre-wrap' }}>{message.internet_response}</p>
          </section>
        )}

        {message.link_url && (
          <section style={{ marginTop: 16 }}>
            <h4 style={{ marginBottom: 4 }}>Link</h4>
            <a href={message.link_url} target="_blank" rel="noopener noreferrer">
              {message.link_description ?? message.link_url}
            </a>
          </section>
        )}

        <hr style={{ margin: '20px 0 12px', border: 0, borderTop: '1px solid var(--border)' }} />
        <div className="muted" style={{ fontSize: 11, lineHeight: 1.4 }}>
          <div>id: {message.kindroid_msg_id}</div>
          <div>fetched_at: {fetched}</div>
        </div>

        <div className="modal-actions">
          <button className="btn" onClick={onClose}>
            Close
          </button>
        </div>
      </div>
    </div>
  );
}

function makeSnippet(text: string, query: string, radius = 30): React.ReactNode {
  if (!query) return text;
  if (!text) return <span className="muted">(empty message)</span>;
  const lc = text.toLowerCase();
  const firstToken = query.toLowerCase().split(/\s+/)[0];
  const idx = lc.indexOf(firstToken);
  if (idx < 0) {
    const short = text.length > 80 ? `${text.slice(0, 80)}…` : text;
    return short;
  }
  const start = Math.max(0, idx - radius);
  const end = Math.min(text.length, idx + firstToken.length + radius);
  const before = start > 0 ? '…' : '';
  const after = end < text.length ? '…' : '';
  return (
    <span>
      {before}
      {text.slice(start, end)}
      {after}
    </span>
  );
}
