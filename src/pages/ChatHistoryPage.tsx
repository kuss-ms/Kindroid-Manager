import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useEffect, useMemo, useRef, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { api, escapeFtsQuery, errorMessage } from '../lib/api';
import type {
  ChatAutomationDto,
  ChatMessage,
  ChatSyncState,
  SyncStatusKind,
  Target,
  TargetKind,
} from '../lib/types';
import { TARGET_KIND_LABEL } from '../lib/types';
import { ConfirmDialog } from '../components/ConfirmDialog';
import { AutomationPanel } from '../components/AutomationPanel';
import { toast } from '../components/Toaster';
import { renderChatMarkdown } from '../lib/chatMarkdown';
import {
  applyChatTheme,
  CUSTOM_PRESET_ID,
  PRESET_THEMES,
  useChatTheme,
  type ChatTheme,
  type ChatThemePalette,
} from '../lib/chatThemes';

const PAGE_SIZE = 50;
const SEARCH_LIMIT = 200;

type ViewMode = 'chat' | 'history';

function parseViewMode(raw: string | null): ViewMode {
  if (raw === 'chat' || raw === 'history') return raw;
  return 'chat';
}

interface ActiveSyncInfo {
  ai_id: string;
  kind: TargetKind;
}

interface LiveProgress {
  ai_id: string;
  kind: TargetKind;
  total: number;
  requests: number;
  last_batch_size: number;
  last_batch_had_messages: boolean;
  last_deleted_count: number;
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
  const current = useQuery<ActiveSyncInfo | null>({
    queryKey: ['current-sync'],
    queryFn: api.getCurrentSync,
    refetchInterval: 5000,
  });

  const targetsList = useMemo(() => targets.data ?? [], [targets.data]);
  const urlAiId = params.get('ai_id');
  const urlKind = params.get('kind') as TargetKind | null;
  // `selectedKey` is the (ai_id, kind) pair that uniquely identifies a
  // target — Kindroid lets an AI and a Group share the same identifier
  // string, so keying the UI on `ai_id` alone would conflate them.
  const selectedKey = useMemo<{ ai_id: string; kind: TargetKind } | null>(() => {
    if (!targets.isLoading && targetsList.length === 0) return null;
    if (urlAiId && urlKind && targetsList.some((t) => t.ai_id === urlAiId && t.kind === urlKind)) {
      return { ai_id: urlAiId, kind: urlKind };
    }
    return null;
  }, [urlAiId, urlKind, targetsList, targets.isLoading]);
  const selectedAiId = selectedKey?.ai_id ?? null;
  const selectedKind: TargetKind | null = selectedKey?.kind ?? null;
  const isGroup = selectedKind === 'group';

  // View mode (URL `?view=`). Default is "chat" for single-AI targets;
  // group targets force "history" below so the toggle isn't shown.
  const urlView = parseViewMode(params.get('view'));
  // If the target is a group, the chat view is unavailable.
  const view: ViewMode = isGroup ? 'history' : urlView;
  function setView(next: ViewMode) {
    const newParams = new URLSearchParams(params);
    newParams.set('view', next);
    setParams(newParams, { replace: true });
  }

  const selectedTarget = useMemo(
    () =>
      selectedKey
        ? (targetsList.find((t) => t.ai_id === selectedKey.ai_id && t.kind === selectedKey.kind) ??
          null)
        : null,
    [targetsList, selectedKey],
  );

  function setSelectedTarget(ai_id: string, kind: TargetKind) {
    const next = new URLSearchParams(params);
    if (ai_id) {
      next.set('ai_id', ai_id);
      next.set('kind', kind);
    } else {
      next.delete('ai_id');
      next.delete('kind');
    }
    setParams(next, { replace: true });
  }

  // Search state.
  const [searchInput, setSearchInput] = useState('');
  const [debouncedQuery, setDebouncedQuery] = useState('');
  useEffect(() => {
    const handle = setTimeout(() => setDebouncedQuery(searchInput), 200);
    return () => clearTimeout(handle);
  }, [searchInput]);

  // Favourites-only filter (applied to browse + search).
  const [favouritesOnly, setFavouritesOnly] = useState(false);

  // Browse pagination: stack of `before_ts` cursors, each one is the
  // oldest message's timestamp on the corresponding page. The top of
  // the stack is the page currently shown; pushing a new cursor
  // advances to the next older page, popping goes back to a newer one.
  // The root of the stack is `null` (newest page, no `before_ts`).
  const [browseCursorStack, setBrowseCursorStack] = useState<Array<number | null>>([null]);
  const browseCursor = browseCursorStack[browseCursorStack.length - 1] ?? null;
  useEffect(() => {
    setBrowseCursorStack([null]);
  }, [selectedAiId, debouncedQuery, favouritesOnly]);

  // Search keeps a plain numeric offset (search_chat uses SQL OFFSET).
  const [searchOffset, setSearchOffset] = useState(0);
  useEffect(() => {
    setSearchOffset(0);
  }, [selectedAiId, debouncedQuery, favouritesOnly]);

  const trimmedQuery = debouncedQuery.trim();
  // The search box only applies in History view (chat is single-AI
  // composer only). When the user types into History, we run a
  // FTS search; when they clear it, we fall back to the regular
  // browse list.
  const isSearching = view === 'history' && trimmedQuery.length > 0;

  // Live progress payload from the backend (refreshed via events).
  const [liveProgress, setLiveProgress] = useState<LiveProgress | null>(null);
  useEffect(() => {
    // Reset when target changes so we don't show stale progress from a
    // previous sync.
    setLiveProgress(null);
  }, [selectedAiId]);

  // Sync state (5 s polling even when navigated away).
  const syncState = useQuery<ChatSyncState | null>({
    queryKey: ['chat-sync-state', selectedAiId, selectedKind],
    queryFn: () =>
      selectedAiId && selectedKind
        ? api.getChatSyncState(selectedAiId, selectedKind)
        : Promise.resolve(null),
    enabled: !!selectedAiId && !!selectedKind,
    refetchInterval: 5000,
  });
  const messageCount = useQuery<number | null>({
    queryKey: ['chat-message-count', selectedAiId, selectedKind],
    queryFn: () =>
      selectedAiId && selectedKind
        ? api.chatMessageCount(selectedAiId, selectedKind)
        : Promise.resolve(null),
    enabled: !!selectedAiId && !!selectedKind,
    refetchInterval: 5000,
  });

  // Lightweight view of automation state so the page can surface a
  // status badge on the Automation… button without opening the modal.
  // Group targets can't have automation, so skip the query entirely
  // for them — the backend would reject, and React Query would log the
  // error in devtools even when the response is ignored.
  const automationBadge = useQuery<ChatAutomationDto | null>({
    queryKey: ['chat-automation', selectedAiId, selectedKind],
    queryFn: () =>
      selectedAiId && selectedKind === 'ai'
        ? api.getChatAutomationState(selectedAiId)
        : Promise.resolve(null),
    enabled: !!selectedAiId && selectedKind === 'ai',
    refetchInterval: 5000,
  });
  const automationHasError =
    !!automationBadge.data &&
    !!(
      automationBadge.data.state.journal_last_error || automationBadge.data.state.summary_last_error
    );
  const automationIsActive =
    !!automationBadge.data &&
    (automationBadge.data.state.auto_journal_enabled ||
      automationBadge.data.state.auto_summary_enabled);

  // Page of messages (browse mode). Runs for both chat and history views.
  const browsePage = useQuery<ChatMessage[]>({
    queryKey: ['chat-messages', selectedAiId, selectedKind, browseCursor, favouritesOnly],
    queryFn: () => {
      if (!selectedAiId || !selectedKind) return Promise.resolve([]);
      return api.listChatMessages(
        selectedAiId,
        selectedKind,
        browseCursor,
        PAGE_SIZE,
        favouritesOnly,
      );
    },
    enabled: !!selectedAiId && !!selectedKind && !isSearching,
  });

  // Search results.
  const searchPage = useQuery<ChatMessage[]>({
    queryKey: [
      'chat-search',
      selectedAiId,
      selectedKind,
      trimmedQuery,
      searchOffset,
      favouritesOnly,
    ],
    queryFn: () => {
      if (!selectedAiId || !selectedKind || !trimmedQuery) return Promise.resolve([]);
      return api.searchChat(
        selectedAiId,
        selectedKind,
        escapeFtsQuery(trimmedQuery),
        PAGE_SIZE,
        searchOffset,
        favouritesOnly,
      );
    },
    enabled: !!selectedAiId && !!selectedKind && isSearching,
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
        kind: TargetKind;
        total: number;
        last_timestamp: number;
        full_sync_done: boolean;
        status_kind: SyncStatusKind;
        status_message: string | null;
        requests: number;
        last_batch_size: number;
        last_batch_had_messages: boolean;
        last_deleted_count: number;
      }>('chat-sync-progress', (event) => {
        const p = event.payload;
        if (selectedAiId && selectedKind) {
          if (p.ai_id !== selectedAiId || p.kind !== selectedKind) return;
        }
        setLiveProgress({ ...p, received_at: Date.now() });
        queryClient.invalidateQueries({ queryKey: ['chat-sync-state'] });
        queryClient.invalidateQueries({ queryKey: ['chat-message-count'] });
        queryClient.invalidateQueries({ queryKey: ['current-sync'] });
        // Refresh the visible messages so newly-fetched rows appear.
        queryClient.invalidateQueries({ queryKey: ['chat-messages'] });
        queryClient.invalidateQueries({ queryKey: ['chat-search'] });
        // The automation cycle runs after a successful drain; refresh its
        // state too so the panel picks up the new cursor / last-run time.
        queryClient.invalidateQueries({ queryKey: ['chat-automation'] });
      }),
    );
    unlistens.push(
      listen<{
        ai_id: string;
        kind: TargetKind;
        total: number;
        status_kind: SyncStatusKind;
        status_message: string | null;
        requests: number;
      }>('chat-sync-complete', (event) => {
        const p = event.payload;
        if (selectedAiId && selectedKind) {
          if (p.ai_id !== selectedAiId || p.kind !== selectedKind) return;
        }
        setLiveProgress({
          ai_id: p.ai_id,
          kind: p.kind,
          total: p.total,
          last_timestamp: 0,
          full_sync_done: true,
          status_kind: p.status_kind,
          status_message: p.status_message,
          requests: p.requests,
          last_batch_size: 0,
          last_batch_had_messages: false,
          last_deleted_count: 0,
          received_at: Date.now(),
        });
        queryClient.invalidateQueries({ queryKey: ['chat-sync-state'] });
        queryClient.invalidateQueries({ queryKey: ['chat-message-count'] });
        queryClient.invalidateQueries({ queryKey: ['current-sync'] });
        queryClient.invalidateQueries({ queryKey: ['chat-messages'] });
        queryClient.invalidateQueries({ queryKey: ['chat-search'] });
        queryClient.invalidateQueries({ queryKey: ['chat-automation'] });
      }),
    );
    return () => {
      unlistens.forEach((p) => p.then((u) => u()).catch(() => {}));
    };
  }, [queryClient, selectedAiId, selectedKind]);

  // Cancel any sync running on the same (ai_id, kind) while in chat
  // view, so the chat composer doesn't fight the background loop for the
  // token. Syncs on other targets are left alone. The effect re-runs
  // on every change of `current.data` so a sync that starts while we're
  // already in chat view is also cancelled (the latch was removed —
  // it was racy because `current.data` polls every 5s and could be
  // stale on first mount).
  useEffect(() => {
    if (view !== 'chat') return;
    if (!selectedAiId || !selectedKind) return;
    if (isGroup) return;
    const cur = current.data;
    if (cur && cur.ai_id === selectedAiId && cur.kind === selectedKind) {
      api.cancelChatSync().catch(() => {
        // Best-effort: if the cancel fails the user can still chat,
        // and the sync will end on its own.
      });
    }
  }, [view, selectedAiId, selectedKind, isGroup, current.data]);
  // Surface a one-time info toast when a cancel actually fires, so the
  // user knows their sync was paused. The latch is keyed on
  // (ai_id, kind) and is cleared when the user navigates away from
  // chat view, so re-entering chat and triggering another cancel
  // re-shows the toast.
  const cancelToastKeyRef = useRef<string | null>(null);
  useEffect(() => {
    if (view !== 'chat') {
      cancelToastKeyRef.current = null;
      return;
    }
    if (!selectedAiId || !selectedKind || isGroup) return;
    const cur = current.data;
    if (cur && cur.ai_id === selectedAiId && cur.kind === selectedKind) {
      const key = `${cur.ai_id}|${cur.kind}`;
      if (cancelToastKeyRef.current !== key) {
        cancelToastKeyRef.current = key;
        toast('info', 'Sync paused for chat.');
        queryClient.invalidateQueries({ queryKey: ['chat-sync-state'] });
        queryClient.invalidateQueries({ queryKey: ['current-sync'] });
      }
    }
  }, [view, selectedAiId, selectedKind, isGroup, current.data, queryClient]);
  // Toggle the `chat-mode` body class while a chat view is rendered
  // so the global CSS can apply `body.chat-mode .app-main {
  // overflow: clip }`. That suppresses the page-level scrollbar
  // (whose thumb would otherwise be hidden behind the position-fixed
  // composer) without breaking the inner `.chat-scroll`'s own
  // `overflow-y: auto` — `overflow: clip` does not create a scroll
  // container, so wheel events still reach the messages.
  useEffect(() => {
    if (view === 'chat' && selectedAiId && !isGroup) {
      document.body.classList.add('chat-mode');
      return () => document.body.classList.remove('chat-mode');
    }
    document.body.classList.remove('chat-mode');
  }, [view, selectedAiId, isGroup]);

  async function onSync() {
    if (!selectedAiId || !selectedKind) return;
    try {
      await api.startChatSync(selectedAiId, selectedKind);
      setLiveProgress({
        ai_id: selectedAiId,
        kind: selectedKind,
        total: syncState.data?.total ?? 0,
        last_timestamp: 0,
        full_sync_done: false,
        status_kind: 'running',
        status_message: null,
        requests: 0,
        last_batch_size: 0,
        last_batch_had_messages: false,
        last_deleted_count: 0,
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
  // Automation settings open in a modal so the chat-history stream
  // stays focused on the chat itself.
  const [automationOpen, setAutomationOpen] = useState(false);
  async function onResetConfirm() {
    if (!selectedAiId || !selectedKind) return;
    setResetting(true);
    try {
      const deleted = await api.resetChatHistory(selectedAiId, selectedKind);
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

  // Optimistic favourite mutation. The server's response is the canonical
  // value, so we reconcile the cache to it on success. On failure we
  // roll back the optimistic flip and surface a toast.
  const [pendingFavourites, setPendingFavourites] = useState<Set<string>>(new Set());
  const setFavourite = useMutation<
    boolean,
    unknown,
    { kindroidMsgId: string; prevFavourite: boolean },
    { aiId: string; kind: TargetKind; kindroidMsgId: string; prevFavourite: boolean }
  >({
    mutationFn: ({ kindroidMsgId }) => {
      const aiId = selectedAiId;
      const kind = selectedKind;
      if (!aiId || !kind) throw new Error('no target selected');
      return api.setChatMessageFavourite(aiId, kind, kindroidMsgId);
    },
    onMutate: ({ kindroidMsgId, prevFavourite }) => {
      if (!selectedAiId || !selectedKind) {
        return { aiId: '', kind: 'ai' as TargetKind, kindroidMsgId, prevFavourite };
      }
      setPendingFavourites((prev) => {
        const next = new Set(prev);
        next.add(kindroidMsgId);
        return next;
      });
      const aiId = selectedAiId;
      const kind = selectedKind;
      // Optimistically flip the favourite on every cached page for this
      // message. React Query keys include favouritesOnly + browseOffset +
      // kind, so we patch every variant via setQueriesData.
      queryClient.setQueriesData<ChatMessage[]>({ queryKey: ['chat-messages', aiId] }, (old) =>
        old
          ? old.map((m) =>
              m.kindroid_msg_id === kindroidMsgId ? { ...m, favourite: !prevFavourite } : m,
            )
          : old,
      );
      queryClient.setQueriesData<ChatMessage[]>({ queryKey: ['chat-search', aiId] }, (old) =>
        old
          ? old.map((m) =>
              m.kindroid_msg_id === kindroidMsgId ? { ...m, favourite: !prevFavourite } : m,
            )
          : old,
      );
      // If the filter is on and we just unfavourited, drop the row so it
      // disappears from the filtered list immediately.
      if (favouritesOnly && prevFavourite) {
        queryClient.setQueriesData<ChatMessage[]>({ queryKey: ['chat-messages', aiId] }, (old) =>
          old ? old.filter((m) => m.kindroid_msg_id !== kindroidMsgId) : old,
        );
        queryClient.setQueriesData<ChatMessage[]>({ queryKey: ['chat-search', aiId] }, (old) =>
          old ? old.filter((m) => m.kindroid_msg_id !== kindroidMsgId) : old,
        );
      }
      return { aiId, kind, kindroidMsgId, prevFavourite };
    },
    onSuccess: (canonical, { kindroidMsgId }) => {
      if (!selectedAiId || !selectedKind) return;
      // Reconcile every cache to the server's authoritative value. If the
      // filter is on and the server cleared the pin, drop the row.
      const aiId = selectedAiId;
      const reconcile = (old: ChatMessage[] | undefined) => {
        if (!old) return old;
        if (favouritesOnly && !canonical) {
          return old.filter((m) => m.kindroid_msg_id !== kindroidMsgId);
        }
        return old.map((m) =>
          m.kindroid_msg_id === kindroidMsgId ? { ...m, favourite: canonical } : m,
        );
      };
      queryClient.setQueriesData<ChatMessage[]>({ queryKey: ['chat-messages', aiId] }, reconcile);
      queryClient.setQueriesData<ChatMessage[]>({ queryKey: ['chat-search', aiId] }, reconcile);
      // Reflect in the open detail dialog too, if its message id matches.
      setOpenMessage((cur) =>
        cur && cur.kindroid_msg_id === kindroidMsgId ? { ...cur, favourite: canonical } : cur,
      );
      // Total visible count may shift if filter is on.
      queryClient.invalidateQueries({ queryKey: ['chat-message-count', aiId] });
    },
    onError: (e, { kindroidMsgId, prevFavourite }, ctx) => {
      if (!ctx?.aiId) {
        toast('error', errorMessage(e));
        return;
      }
      toast('error', errorMessage(e));
      const aiId = ctx.aiId;
      const restore = (old: ChatMessage[] | undefined) =>
        old
          ? old.map((m) =>
              m.kindroid_msg_id === kindroidMsgId ? { ...m, favourite: prevFavourite } : m,
            )
          : old;
      queryClient.setQueriesData<ChatMessage[]>({ queryKey: ['chat-messages', aiId] }, restore);
      queryClient.setQueriesData<ChatMessage[]>({ queryKey: ['chat-search', aiId] }, restore);
      setOpenMessage((cur) =>
        cur && cur.kindroid_msg_id === kindroidMsgId ? { ...cur, favourite: prevFavourite } : cur,
      );
    },
    onSettled: (_data, _err, { kindroidMsgId }) => {
      setPendingFavourites((prev) => {
        if (!prev.has(kindroidMsgId)) return prev;
        const next = new Set(prev);
        next.delete(kindroidMsgId);
        return next;
      });
    },
  });

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
        <div className="empty">Add a target on the Targets page to enable chat history.</div>
      </div>
    );
  }

  // Targets loaded but the user hasn't picked one yet (or the URL had a
  // stale ai_id/kind). Show a helpful empty state instead of rendering the
  // full chat-history UI with no data.
  if (!selectedAiId || !selectedKind) {
    return (
      <div className="page">
        <div className="page-header">
          <h2>Chat History</h2>
        </div>
        <div
          className="form-row"
          style={{
            flexDirection: 'row',
            gap: 8,
            alignItems: 'center',
            flexWrap: 'wrap',
          }}
        >
          <label className="form-label" htmlFor="target-select" style={{ flexShrink: 0 }}>
            Target
          </label>
          <select
            id="target-select"
            className="select"
            value=""
            onChange={(e) => {
              const value = e.target.value;
              // Find the matching target so we can carry both ai_id
              // and kind into the URL.
              const match = targetsList.find((t) => t.id === value);
              if (match) setSelectedTarget(match.ai_id, match.kind);
            }}
          >
            <option value="">— select a target —</option>
            {targetsList.map((t) => (
              <option key={t.id} value={t.id}>
                {t.label} ({t.ai_id}) — {TARGET_KIND_LABEL[t.kind]}
              </option>
            ))}
          </select>
        </div>
        <div className="empty">Select a target to view its chat history.</div>
      </div>
    );
  }

  const currentSyncing = current.data;
  const state = syncState.data ?? null;
  const statusKind: SyncStatusKind = liveProgress?.status_kind ?? state?.status_kind ?? 'idle';
  const total = liveProgress?.total ?? state?.total ?? messageCount.data ?? 0;

  // `activeSyncMatches` is true when the registry's currently-running
  // sync is exactly this (ai_id, kind). A sync for a different kind on
  // the same ai_id is treated as "another sync in progress" — group +
  // AI share the singleton slot.
  const activeSyncMatches =
    currentSyncing !== null &&
    currentSyncing !== undefined &&
    currentSyncing.ai_id === selectedAiId &&
    currentSyncing.kind === selectedKind;

  // In chat-view we hide Sync / Reset / Automation so the user can't
  // trigger destructive ops while composing a message. The chat mode
  // also cancels any in-flight sync on entry — see the `cancelledOnceRef`
  // effect — and the Sync button stays disabled until the user switches
  // back to History and re-syncs.
  const showHistoryActions = view === 'history';

  // Build the progress indicator subtitle. During a sync we combine the
  // request count + last-batch timestamp so the user can see whether the
  // backfill is making progress.
  function progressSubtitle(): string {
    if (activeSyncMatches && liveProgress) {
      const reqPart = liveProgress.requests > 0 ? `Request #${liveProgress.requests}` : 'Starting…';
      const cursorPart = liveProgress.last_timestamp
        ? `Last message: ${tsToLocal(liveProgress.last_timestamp)}`
        : 'Awaiting first page…';
      return `Syncing… · ${reqPart} · ${cursorPart}`;
    }
    if (currentSyncing && !activeSyncMatches) {
      return `Sync in progress for ${currentSyncing.ai_id} (${TARGET_KIND_LABEL[currentSyncing.kind]})`;
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

  if (activeSyncMatches) {
    showCancel = true;
    const lastDel = liveProgress?.last_deleted_count ?? 0;
    const lastBatch = liveProgress?.last_batch_size ?? 0;
    if (lastDel > 0) {
      body = `Latest page returned ${lastBatch} message${lastBatch === 1 ? '' : 's'}; removed ${lastDel} deleted on server.`;
    } else if (lastBatch > 0) {
      body = `Latest page returned ${lastBatch} message${lastBatch === 1 ? '' : 's'}.`;
    } else {
      body = `Last updated: ${localTime(state?.last_synced_at) || '—'}`;
    }
  } else if (currentSyncing && !activeSyncMatches) {
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
    body = isGroup
      ? 'No messages on Kindroid for this group.'
      : 'No messages on Kindroid for this AI.';
  } else {
    showSync = true;
  }

  const activeList = isSearching ? searchPage : browsePage;
  const messages = activeList.data ?? [];

  return (
    <div
      className="page"
      // `flex: 1` on the page wrapper lets the chat pane (which has
      // `flex: 1` in CSS) fill the remaining vertical space in
      // `.app-main`. Without this, `.page` sizes to its content and
      // `.chat-view` has nothing left to expand into, leaving a
      // large empty area below the composer. We only stretch when
      // the chat pane is actually rendered — in history mode the
      // page wraps its content normally so the message list sits at
      // its natural height near the top.
      style={{ flex: view === 'chat' && selectedAiId && !isGroup ? 1 : undefined }}
    >
      <div className="page-header">
        <h2>Chat History</h2>
        <div className="muted">{subtitle}</div>
      </div>

      <div
        className="form-row"
        style={{ flexDirection: 'row', gap: 8, alignItems: 'center', flexWrap: 'wrap' }}
      >
        <label className="form-label" htmlFor="target-select" style={{ flexShrink: 0 }}>
          Target
        </label>
        <select
          id="target-select"
          className="select"
          value={selectedTarget?.id ?? ''}
          onChange={(e) => {
            const value = e.target.value;
            const match = targetsList.find((t) => t.id === value);
            if (match) setSelectedTarget(match.ai_id, match.kind);
          }}
          style={{
            // The base `.select` rule (global.css) sets `width: 100%`
            // for form-row stacks. In this toolbar row we want the
            // select to size to its content so the spacer + action
            // buttons + segmented control can sit on the same line.
            // Without this override the select fills the entire row,
            // wrapping everything else below.
            width: 'auto',
            flex: '0 1 auto',
            minWidth: 180,
            maxWidth: 360,
          }}
        >
          <option value="">— select a target —</option>
          {targetsList.map((t) => (
            <option key={t.id} value={t.id}>
              {t.label} ({t.ai_id}) — {TARGET_KIND_LABEL[t.kind]}
            </option>
          ))}
        </select>
        {/* Spacer absorbs all available free space so the action buttons
            (Sync / Cancel / Automation / Reset) and the chat/history
            segmented control sit on the right side of the toolbar row.
            Without this they'd pack immediately to the right of the
            select and the row would look crowded. The wider action
            buttons wrap to a second line on narrow viewports. */}
        <div style={{ flex: 1 }} />
        {showHistoryActions && showSync && (
          <button
            className="btn btn-primary"
            disabled={!!syncDisabledReason}
            title={syncDisabledReason ?? ''}
            onClick={onSync}
          >
            Sync
          </button>
        )}
        {showHistoryActions && showCancel && (
          <button className="btn" onClick={onCancel}>
            Cancel
          </button>
        )}
        {/* Reset is available whenever a target is selected, except
            while a sync is running on this target (the wipe would race
            with the in-flight loop). */}
        {showHistoryActions && (
          <button
            className="btn"
            onClick={() => setAutomationOpen(true)}
            disabled={isGroup}
            title={
              isGroup
                ? 'Automation is not available for group chats.'
                : automationHasError
                  ? 'Automation recorded an error — open to clear or reset.'
                  : automationIsActive
                    ? 'Auto-journal or auto-summary is enabled for this target.'
                    : 'Configure auto-journal and auto-summary for this target.'
            }
            data-testid="automation-button"
          >
            Automation…
            {!isGroup && automationHasError && (
              <span
                className="badge badge-danger"
                style={{ marginLeft: 6 }}
                aria-label="automation error"
              >
                error
              </span>
            )}
            {!isGroup && automationIsActive && !automationHasError && (
              <span
                className="badge badge-success"
                style={{ marginLeft: 6 }}
                aria-label="automation enabled"
              >
                on
              </span>
            )}
          </button>
        )}
        {showHistoryActions && (
          <button
            className="btn btn-danger"
            onClick={() => setResetOpen(true)}
            disabled={resetting || activeSyncMatches}
            title={
              activeSyncMatches
                ? 'Cancel the sync before resetting.'
                : 'Delete all locally-cached chat history for this target.'
            }
          >
            Reset
          </button>
        )}
        {/* Chat / History segmented control. The spacer before the action
            buttons already pushes everything to the right side of the
            toolbar, so the tabs naturally sit at the far right after
            the action buttons. `flex-shrink: 0` keeps them together so
            they don't collapse if the row is tight. Hidden for group
            targets — chat-mode is single-AI only. */}
        {!isGroup && (
          <div
            data-testid="view-segmented"
            style={{
              flexDirection: 'row',
              gap: 0,
              flexShrink: 0,
            }}
            role="tablist"
          >
            <button
              type="button"
              role="tab"
              aria-selected={view === 'chat'}
              className={`btn ${view === 'chat' ? 'btn-primary' : ''}`}
              onClick={() => setView('chat')}
              style={{ borderTopRightRadius: 0, borderBottomRightRadius: 0 }}
            >
              Chat
            </button>
            <button
              type="button"
              role="tab"
              aria-selected={view === 'history'}
              className={`btn ${view === 'history' ? 'btn-primary' : ''}`}
              onClick={() => setView('history')}
              style={{ borderTopLeftRadius: 0, borderBottomLeftRadius: 0 }}
            >
              History
            </button>
          </div>
        )}
      </div>

      {body && view !== 'chat' && <p className="muted">{body}</p>}

      {/* Chat view: chat bubbles + composer. Reuses the same query key as
          the history list so switching tabs is instant. */}
      {view === 'chat' && !isGroup && selectedAiId && selectedKind && (
        <ChatView
          aiId={selectedAiId}
          kind={selectedKind}
          messages={browsePage.data ?? []}
          pendingFavourites={pendingFavourites}
          onToggleFavourite={(id, prev) =>
            setFavourite.mutate({ kindroidMsgId: id, prevFavourite: prev })
          }
          onInvalidate={() => {
            queryClient.invalidateQueries({ queryKey: ['chat-messages'] });
            queryClient.invalidateQueries({ queryKey: ['chat-message-count'] });
          }}
        />
      )}

      {view === 'history' && (
        <>
          <div
            className="form-row"
            style={{ flexDirection: 'row', gap: 8, alignItems: 'center', flexWrap: 'wrap' }}
          >
            <input
              type="search"
              className="input input-search"
              placeholder="Search messages…"
              value={searchInput}
              onChange={(e) => setSearchInput(e.target.value)}
              style={{ flex: 1, minWidth: 200 }}
            />
            <label className="checkbox" title="Show only messages you've favourited (pinned) here">
              <input
                type="checkbox"
                checked={favouritesOnly}
                onChange={(e) => setFavouritesOnly(e.target.checked)}
              />
              Favourites only
            </label>
          </div>

          {isSearching && (
            <p className="muted" style={{ marginTop: 8 }}>
              {messages.length === 0
                ? `No matches for "${trimmedQuery}".`
                : `Showing ${messages.length} matches. All terms required (Porter stemmed); wrap a phrase in "quotes" for an exact match.`}
            </p>
          )}

          <div style={{ marginTop: 12 }}>
            {messages.map((m) => (
              <MessageRow
                key={m.id}
                message={m}
                query={trimmedQuery}
                pending={pendingFavourites.has(m.kindroid_msg_id)}
                onOpen={() => setOpenMessage(m)}
                onToggleFavourite={() =>
                  setFavourite.mutate({
                    kindroidMsgId: m.kindroid_msg_id,
                    prevFavourite: m.favourite,
                  })
                }
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
              disabled={isSearching ? searchOffset === 0 : browseCursorStack.length <= 1}
              onClick={() => {
                if (isSearching) {
                  setSearchOffset(Math.max(0, searchOffset - PAGE_SIZE));
                } else {
                  setBrowseCursorStack((stack) => (stack.length > 1 ? stack.slice(0, -1) : stack));
                }
              }}
            >
              ← {isSearching ? 'Prev' : 'Newer'}
            </button>
            <button
              className="btn"
              disabled={
                messages.length < PAGE_SIZE ||
                (isSearching && searchOffset + PAGE_SIZE >= SEARCH_LIMIT)
              }
              onClick={() => {
                if (isSearching) {
                  setSearchOffset(searchOffset + PAGE_SIZE);
                } else {
                  const oldest = messages[messages.length - 1];
                  if (oldest) {
                    setBrowseCursorStack((stack) => [...stack, oldest.timestamp]);
                  }
                }
              }}
            >
              {isSearching ? 'Next' : 'Older'} →
            </button>
          </div>
        </>
      )}

      <MessageDetailDialog
        message={openMessage}
        pending={openMessage !== null && pendingFavourites.has(openMessage.kindroid_msg_id)}
        onToggleFavourite={() => {
          if (!openMessage) return;
          setFavourite.mutate({
            kindroidMsgId: openMessage.kindroid_msg_id,
            prevFavourite: openMessage.favourite,
          });
        }}
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

      {automationOpen && selectedAiId && (
        <div
          className="modal-overlay"
          role="dialog"
          aria-modal="true"
          aria-label="Automation settings"
          onClick={(e) => {
            if (e.target === e.currentTarget) setAutomationOpen(false);
          }}
        >
          <div className="modal" style={{ maxWidth: 760, maxHeight: '85vh', overflow: 'auto' }}>
            <div
              style={{
                display: 'flex',
                justifyContent: 'space-between',
                alignItems: 'baseline',
                marginBottom: 8,
              }}
            >
              <h3 style={{ margin: 0 }}>Automation</h3>
              <button
                className="btn btn-sm"
                onClick={() => setAutomationOpen(false)}
                aria-label="Close"
              >
                ✕
              </button>
            </div>
            <p className="muted text-sm" style={{ marginTop: 0 }}>
              Configure auto-journal and auto-summary for <code>{selectedAiId}</code>. Changes are
              applied when you click <strong>Save settings</strong>.
            </p>
            <AutomationPanel
              aiId={selectedAiId}
              kind={selectedKind}
              automationInProgress={!!selectedAiId && !!state && state.status_kind === 'running'}
            />
          </div>
        </div>
      )}
    </div>
  );
}

function MessageRow({
  message,
  query,
  pending,
  onOpen,
  onToggleFavourite,
}: {
  message: ChatMessage;
  query: string;
  pending: boolean;
  onOpen: () => void;
  onToggleFavourite: () => void;
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
        display: 'flex',
        gap: 8,
        alignItems: 'flex-start',
      }}
    >
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ display: 'flex', gap: 8, alignItems: 'baseline' }}>
          <strong>{who}</strong>
          <span className="muted text-sm">{when}</span>
        </div>
        <div style={{ marginTop: 2 }}>{snippet}</div>
        {message.image_urls.length > 0 && (
          <div className="muted text-sm">
            🖼 {message.image_urls.length} image{message.image_urls.length === 1 ? '' : 's'}
          </div>
        )}
        {message.link_url && (
          <div className="text-sm">
            🔗{' '}
            <a href={message.link_url} onClick={(e) => e.stopPropagation()}>
              {message.link_description ?? message.link_url}
            </a>
          </div>
        )}
      </div>
      <HeartButton
        active={message.favourite}
        pending={pending}
        onClick={onToggleFavourite}
        label={message.favourite ? 'Unfavourite' : 'Favourite'}
      />
    </div>
  );
}

function HeartButton({
  active,
  pending,
  onClick,
  label,
}: {
  active: boolean;
  pending: boolean;
  onClick: () => void;
  label: string;
}) {
  return (
    <button
      type="button"
      className="btn btn-sm"
      aria-label={label}
      aria-pressed={active}
      title={label}
      disabled={pending}
      onClick={(e) => {
        e.stopPropagation();
        e.preventDefault();
        onClick();
      }}
      style={{
        padding: '4px 6px',
        lineHeight: 1,
        background: 'transparent',
        color: active ? 'var(--primary, #2563eb)' : 'var(--muted)',
        border: '1px solid var(--border)',
        opacity: pending ? 0.6 : 1,
      }}
    >
      {active ? (
        // Filled heart.
        <svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor" aria-hidden>
          <path d="M12 21s-7.5-4.7-9.6-9.2C.6 7.5 3.4 4 7.2 4c2 0 3.6 1 4.8 2.6C13.2 5 14.8 4 16.8 4c3.8 0 6.6 3.5 4.8 7.8C19.5 16.3 12 21 12 21z" />
        </svg>
      ) : (
        // Outline heart.
        <svg
          width="16"
          height="16"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          aria-hidden
        >
          <path d="M12 21s-7.5-4.7-9.6-9.2C.6 7.5 3.4 4 7.2 4c2 0 3.6 1 4.8 2.6C13.2 5 14.8 4 16.8 4c3.8 0 6.6 3.5 4.8 7.8C19.5 16.3 12 21 12 21z" />
        </svg>
      )}
    </button>
  );
}

function MessageDetailDialog({
  message,
  pending,
  onToggleFavourite,
  onClose,
}: {
  message: ChatMessage | null;
  pending: boolean;
  onToggleFavourite: () => void;
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
          <div className="flex-row" style={{ gap: 6, alignItems: 'center' }}>
            <HeartButton
              active={message.favourite}
              pending={pending}
              onClick={onToggleFavourite}
              label={message.favourite ? 'Unfavourite' : 'Favourite'}
            />
            <button className="btn btn-sm" onClick={onClose} aria-label="Close">
              ✕
            </button>
          </div>
        </div>
        <div className="muted text-sm" style={{ marginTop: 4 }}>
          {when} · {message.sender}
        </div>

        <div style={{ whiteSpace: 'pre-wrap', marginTop: 16, lineHeight: 1.5 }}>
          {message.message ? renderChatMarkdown(message.message) : <span className="muted">(empty message)</span>}
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
        <div className="muted text-xs" style={{ lineHeight: 1.4 }}>
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

interface ChatViewProps {
  aiId: string;
  kind: TargetKind;
  messages: ChatMessage[];
  pendingFavourites: Set<string>;
  onToggleFavourite: (kindroidMsgId: string, prev: boolean) => void;
  onInvalidate: () => void;
}

const SUGGEST_COOLDOWN_MS = 1500;

function ChatView({
  aiId,
  messages,
  pendingFavourites,
  onToggleFavourite,
  onInvalidate,
}: ChatViewProps) {
  const wrapperRef = useRef<HTMLDivElement | null>(null);
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const sentinelRef = useRef<HTMLDivElement | null>(null);
  const composerRef = useRef<HTMLTextAreaElement | null>(null);

  const { state: themeState, setState: setThemeState } = useChatTheme();

  const [draft, setDraft] = useState('');
  const [pending, setPending] = useState(false);
  const [suggestCooldownUntil, setSuggestCooldownUntil] = useState<number>(0);
  const [rewindOpen, setRewindOpen] = useState(false);

  // Apply theme palette to the wrapper element. `useChatTheme()` runs
  // every render, but `applyChatTheme` is idempotent (just writes CSS
  // custom properties), so it's safe to call repeatedly.
  useEffect(() => {
    if (wrapperRef.current) {
      applyChatTheme(themeState, aiId, wrapperRef.current);
    }
  }, [themeState, aiId]);

  // Auto-scroll to the sentinel on new messages.
  useEffect(() => {
    sentinelRef.current?.scrollIntoView({ block: 'end' });
  }, [messages.length]);

  const send = useMutation<ChatMessage, unknown, string>({
    mutationFn: (text: string) => api.sendChatMessage({ ai_id: aiId, message: text }),
    onMutate: () => setPending(true),
    onSettled: () => {
      setPending(false);
      onInvalidate();
    },
    onSuccess: () => {
      setDraft('');
      composerRef.current?.focus();
    },
    onError: (e) => toast('error', errorMessage(e)),
  });

  const suggest = useMutation<string, unknown, void>({
    mutationFn: () =>
      api.suggestChatUserMessage({
        ai_id: aiId,
        existing_message: draft,
      }),
    onSuccess: (body) => {
      const trimmed = body.trim();
      if (!trimmed) return;
      setDraft(trimmed);
      // Focus + select all so the user can type-over or hit Enter.
      const ta = composerRef.current;
      if (ta) {
        ta.focus();
        ta.setSelectionRange(0, trimmed.length);
      }
    },
    onError: (e) => toast('error', errorMessage(e)),
    onSettled: () => {
      setSuggestCooldownUntil(Date.now() + SUGGEST_COOLDOWN_MS);
      setTimeout(() => {
        // Trigger a re-render so the button's disabled state refreshes.
        setSuggestCooldownUntil((cur) => cur);
      }, SUGGEST_COOLDOWN_MS);
    },
  });

  const rewind = useMutation<number, unknown, number>({
    mutationFn: (count: number) => api.rewindChat({ ai_id: aiId, count }),
    onSuccess: (deleted) => {
      toast('success', `Rewound ${deleted} message${deleted === 1 ? '' : 's'}.`);
      onInvalidate();
    },
    onError: (e) => toast('error', errorMessage(e)),
    onSettled: () => setRewindOpen(false),
  });

  const trimmedDraft = draft.trim();
  const sendDisabled = pending || suggest.isPending || trimmedDraft.length === 0;
  const suggestDisabled =
    pending || send.isPending || suggest.isPending || Date.now() < suggestCooldownUntil;

  function onComposerKeyDown(e: React.KeyboardEvent<HTMLTextAreaElement>) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      if (!sendDisabled && trimmedDraft) {
        send.mutate(trimmedDraft);
      }
    }
  }

  return (
    <div className="chat-view" ref={wrapperRef} data-testid="chat-view">
      <div className="chat-scroll" ref={scrollRef} data-testid="chat-scroll">
        {/* `browsePage.data` is sorted newest-first (DESC by timestamp).
            Chat UX wants newest at the BOTTOM, so reverse before
            rendering. The order is otherwise stable so React keys +
            memoisation still work. */}
        {[...messages].reverse().map((m) => (
          <Bubble
            key={m.id}
            message={m}
            pending={pendingFavourites.has(m.kindroid_msg_id)}
            onToggleFavourite={() => onToggleFavourite(m.kindroid_msg_id, m.favourite)}
          />
        ))}
        <div ref={sentinelRef} />
      </div>
      <div className="chat-composer">
        <div className="chat-composer-inner">
          <textarea
            ref={composerRef}
            value={draft}
            placeholder="Type a message…"
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={onComposerKeyDown}
            rows={2}
            data-testid="chat-composer"
            disabled={pending}
          />
          <button
            type="button"
            className="btn"
            onClick={() => suggest.mutate()}
            disabled={suggestDisabled}
            title="Suggest a follow-up based on the conversation so far"
            data-testid="chat-suggest"
            aria-label="Suggest a follow-up"
          >
            ✨
          </button>
          <div style={{ position: 'relative' }}>
            <button
              type="button"
              className="btn"
              onClick={() => setRewindOpen((v) => !v)}
              disabled={pending || send.isPending || suggest.isPending}
              title="Rewind the last user/AI pair(s)"
              data-testid="chat-rewind"
              aria-haspopup="true"
              aria-expanded={rewindOpen}
              aria-label="Rewind"
            >
              ↩️
            </button>
            {rewindOpen && (
              <div
                role="menu"
                style={{
                  position: 'absolute',
                  bottom: '100%',
                  right: 0,
                  marginBottom: 4,
                  background: 'var(--surface)',
                  border: '1px solid var(--border)',
                  borderRadius: 'var(--radius)',
                  boxShadow: 'var(--shadow)',
                  padding: 4,
                  zIndex: 5,
                }}
              >
                {[2, 4, 6, 8].map((n) => (
                  <button
                    key={n}
                    type="button"
                    className="btn btn-sm"
                    style={{ display: 'block', width: '100%', textAlign: 'left' }}
                    onClick={() => rewind.mutate(n)}
                    disabled={rewind.isPending}
                    data-testid={`chat-rewind-${n}`}
                  >
                    Rewind {n}
                  </button>
                ))}
              </div>
            )}
          </div>
          <ChatThemePicker aiId={aiId} themeState={themeState} setThemeState={setThemeState} />
          <button
            type="button"
            className="btn btn-primary"
            onClick={() => send.mutate(trimmedDraft)}
            disabled={sendDisabled}
            data-testid="chat-send"
          >
            Send
          </button>
        </div>
      </div>
    </div>
  );
}

interface BubbleProps {
  message: ChatMessage;
  pending: boolean;
  onToggleFavourite: () => void;
}

function Bubble({ message, pending, onToggleFavourite }: BubbleProps) {
  const isUser = message.sender === 'user';
  // `display_name` is populated by Kindroid for AI messages (the
  // character's actual name) and is empty for user messages on the
  // current server API. We fall back to `sender` so the meta line
  // is never blank.
  const author = message.display_name?.trim() || message.sender;
  return (
    <div className={`chat-bubble-row ${isUser ? 'user' : 'ai'}`}>
      <div className={`chat-bubble ${isUser ? 'user' : 'ai'}`}>
        {message.message ? (
          // Chat messages support a tiny inline markdown subset
          // (bold / italic / > line quotes — see chatMarkdown.tsx).
          // Empty messages fall through to the muted placeholder so
          // the renderer never crashes on whitespace.
          renderChatMarkdown(message.message)
        ) : (
          <span style={{ opacity: 0.6 }}>(empty message)</span>
        )}
        <div className="chat-meta">
          <strong style={{ color: 'inherit' }}>{author}</strong>
          <span>·</span>
          <span>{new Date(message.timestamp).toLocaleString()}</span>
          <button
            type="button"
            className="btn btn-sm"
            aria-label={message.favourite ? 'Unfavourite' : 'Favourite'}
            onClick={(e) => {
              e.stopPropagation();
              onToggleFavourite();
            }}
            disabled={pending}
            style={{
              padding: '2px 6px',
              background: 'transparent',
              border: 'none',
              // Inline `color` wins over `.chat-meta`'s `text-muted`,
              // so favourited stars actually show the accent colour.
              color: message.favourite ? 'var(--chat-accent, var(--primary))' : 'inherit',
              opacity: pending ? 0.6 : 1,
              marginLeft: 'auto',
              // Bump the font-size a touch so the glyph reads better
              // at the small meta-row height.
              fontSize: '1rem',
              lineHeight: 1,
            }}
          >
            {message.favourite ? '★' : '☆'}
          </button>
        </div>
        {message.image_urls.length > 0 && (
          <div className="muted text-sm" style={{ marginTop: 4 }}>
            🖼 {message.image_urls.length} image{message.image_urls.length === 1 ? '' : 's'}
          </div>
        )}
        {message.link_url && (
          <div className="text-sm" style={{ marginTop: 4 }}>
            🔗{' '}
            <a href={message.link_url} target="_blank" rel="noopener noreferrer">
              {message.link_description ?? message.link_url}
            </a>
          </div>
        )}
      </div>
    </div>
  );
}

interface ChatThemePickerProps {
  aiId: string;
  themeState: ReturnType<typeof useChatTheme>['state'];
  setThemeState: ReturnType<typeof useChatTheme>['setState'];
}

function ChatThemePicker({ aiId, themeState, setThemeState }: ChatThemePickerProps) {
  const [open, setOpen] = useState(false);
  const [overriding, setOverriding] = useState<boolean>(Boolean(themeState.overridesByAi[aiId]));
  const [custom, setCustom] = useState<ChatThemePalette>(
    themeState.overridesByAi[aiId]?.custom ?? {
      bg: '#ffffff',
      // Light blue user bubble with a darker blue accent — the contrast
      // is what keeps inline marks (italic, bold) and the quote bar
      // visible. Picking equal colours for `userBubble` and `accent`
      // renders the inline marks invisible.
      userBubble: '#dbeafe',
      userText: '#0f172a',
      aiBubble: '#f1f5f9',
      accent: '#2563eb',
      text: '#0f172a',
    },
  );
  const [editingCustom, setEditingCustom] = useState(false);

  const active = themeState.overridesByAi[aiId] ?? themeState.base;

  function commitPreset(presetId: string) {
    const next = {
      ...themeState,
      overridesByAi: { ...themeState.overridesByAi },
    };
    if (overriding) {
      next.overridesByAi[aiId] = { presetId };
    } else {
      next.base = { presetId };
    }
    setThemeState(next);
    setEditingCustom(false);
  }

  function commitCustom(palette: ChatThemePalette) {
    const next = {
      ...themeState,
      overridesByAi: { ...themeState.overridesByAi },
    };
    const theme: ChatTheme = { presetId: CUSTOM_PRESET_ID, custom: palette };
    if (overriding) {
      next.overridesByAi[aiId] = theme;
    } else {
      next.base = theme;
    }
    setThemeState(next);
  }

  return (
    <div style={{ position: 'relative' }}>
      <button
        type="button"
        className="btn"
        onClick={() => setOpen((v) => !v)}
        title="Chat bubble theme"
        data-testid="chat-theme-toggle"
        aria-haspopup="true"
        aria-expanded={open}
      >
        🎨
      </button>
      {open && (
        <div
          role="menu"
          style={{
            position: 'absolute',
            bottom: '100%',
            right: 0,
            marginBottom: 4,
            background: 'var(--surface)',
            border: '1px solid var(--border)',
            borderRadius: 'var(--radius)',
            boxShadow: 'var(--shadow)',
            padding: 8,
            minWidth: 220,
            zIndex: 5,
          }}
          data-testid="chat-theme-menu"
        >
          <label className="checkbox" style={{ marginBottom: 4 }}>
            <input
              type="checkbox"
              checked={overriding}
              onChange={(e) => {
                const next = {
                  ...themeState,
                  overridesByAi: { ...themeState.overridesByAi },
                };
                if (e.target.checked) {
                  next.overridesByAi[aiId] = { ...active };
                  setOverriding(true);
                  setThemeState(next);
                } else {
                  delete next.overridesByAi[aiId];
                  setOverriding(false);
                  setThemeState(next);
                }
              }}
              data-testid="chat-theme-override"
            />
            Overwrite for this target
          </label>
          {PRESET_THEMES.map((p) => (
            <button
              key={p.id}
              type="button"
              className="btn btn-sm"
              style={{
                display: 'block',
                width: '100%',
                textAlign: 'left',
                marginBottom: 2,
                background: active.presetId === p.id ? 'var(--primary-soft)' : undefined,
              }}
              onClick={() => commitPreset(p.id)}
              data-testid={`chat-theme-preset-${p.id}`}
            >
              {p.label}
            </button>
          ))}
          <button
            type="button"
            className="btn btn-sm"
            style={{
              display: 'block',
              width: '100%',
              textAlign: 'left',
              marginBottom: 4,
              background: active.presetId === CUSTOM_PRESET_ID ? 'var(--primary-soft)' : undefined,
            }}
            onClick={() => {
              setEditingCustom(true);
              commitCustom(custom);
            }}
            data-testid="chat-theme-custom"
          >
            Custom…
          </button>
          {editingCustom && (
            <div style={{ marginTop: 4, borderTop: '1px solid var(--border)', paddingTop: 4 }}>
              {(
                [
                  { key: 'bg', label: 'Background' },
                  { key: 'userBubble', label: 'Your bubble' },
                  { key: 'userText', label: 'Your text colour' },
                  { key: 'aiBubble', label: 'AI bubble' },
                  { key: 'accent', label: 'Accent (quotes, stars)' },
                  { key: 'text', label: 'AI text colour' },
                ] as const
              ).map(({ key, label }) => (
                <label
                  key={key}
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    gap: 6,
                    marginBottom: 2,
                    // Inherit the page's text colour so the labels stay
                    // readable in both light and dark mode (the popup
                    // sits on top of `--surface`).
                    color: 'var(--text)',
                  }}
                >
                  <span style={{ flex: 1, fontSize: '0.85rem' }}>{label}</span>
                  <input
                    type="color"
                    value={custom[key]}
                    onChange={(e) => {
                      const next = { ...custom, [key]: e.target.value };
                      setCustom(next);
                      commitCustom(next);
                    }}
                    data-testid={`chat-theme-custom-${key}`}
                  />
                </label>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
