import { invoke } from '@tauri-apps/api/core';
import type {
  AutomationInstructionsDefaults,
  Character,
  CharacterRevision,
  CharacterRevisionSummary,
  ChatAutomationDto,
  ChatMessage,
  ChatSyncState,
  JournalEntry,
  JournalEntryInput,
  PushLogEntry,
  PushRequest,
  PushResult,
  CreateNewKinResult,
  ResetChatSummaryInput,
  ClearStuckAutoJournalRunsInput,
  ClearStuckAutoJournalRunsResult,
  RunSummaryNowInput,
  RunSummaryNowResult,
  SetAutomationInstructionsInput,
  SetChatAutomationSettingsInput,
  SettingsDto,
  AiSettingsDto,
  AiChatCompletionResponse,
  Target,
  TargetKind,
  TestTokenResult,
  TestAiResult,
  Uuid,
} from './types';

export interface CharacterInput {
  id?: Uuid;
  name: string;
  ai_name?: string | null;
  ai_gender?: string | null;
  ai_backstory?: string | null;
  ai_memory?: string | null;
  ai_directive?: string | null;
  ai_example_message?: string | null;
  ai_additional_context?: string | null;
  current_scene?: string | null;
  greeting?: string | null;
  notes?: string | null;
  ai_avatar_description?: string | null;
  cover_image?: string | null;
  default_target_id?: Uuid | null;
}

export interface TargetInput {
  id?: Uuid;
  ai_id: string;
  label: string;
  kind?: TargetKind;
}

export interface SettingsInput {
  base_url: string;
}

/**
 * Tauri 2 wraps every `invoke` rejection as
 * `{ message: '<serialized error>' }`. For an `AppError` the inner string
 * is the JSON-encoded tagged enum from `error.rs`:
 *   `{"kind":"…", …payload}`
 * For a plain `Result<_, String>` command the inner is a raw string.
 *
 * This helper extracts a human-readable string for every known variant,
 * including the nested `KindroidError` / `AiError` / `SecretStoreError`
 * payloads. The nested enums are tagged with `code` (not `kind`) so the
 * outer `AppError.kind` and the inner `…code` discriminator do not
 * collide in the JSON. Unknown shapes fall back to the raw `e.message`
 * string.
 */
export function errorMessage(e: unknown): string {
  if (typeof e === 'string') return e;
  if (!e || typeof e !== 'object') return String(e);

  const any = e as Record<string, unknown>;
  const raw = any.message;
  if (typeof raw !== 'string') return String(e);

  let inner: Record<string, unknown> | null = null;
  try {
    const parsed = JSON.parse(raw);
    if (parsed && typeof parsed === 'object') inner = parsed as Record<string, unknown>;
  } catch {
    return raw;
  }
  if (!inner) return raw;

  const kind = inner.kind as string | undefined;
  switch (kind) {
    case 'not_found':
      return 'Not found.';
    case 'invalid':
      return `Invalid input: ${stringField(inner, 'message') ?? 'unknown'}`;
    case 'nothing_to_push':
      return 'Nothing to push — pick a character and a target with persona fields selected.';
    case 'missing_greeting':
      return 'A greeting is required when chat-break is enabled.';
    case 'token_missing':
      return 'No API token configured — open Settings and set a token first.';
    case 'share_code':
      return `Invalid share image: ${stringField(inner, 'message') ?? 'malformed'}`;
    case 'database':
      return `Storage error: ${stringField(inner, 'message') ?? 'unknown'}`;
    case 'internal':
      return `Internal error: ${stringField(inner, 'message') ?? 'unknown'}`;
    case 'sync_conflict':
      return `A sync is already running for ${
        stringField(inner, 'ai_id') ?? 'another target'
      }. Cancel it first.`;
    case 'secret':
      return secretMessage(inner);
    case 'kindroid':
      return kindroidMessage(inner);
    case 'ai':
      return aiMessage(inner);
    default:
      return raw;
  }
}

function stringField(o: Record<string, unknown>, k: string): string | null {
  const v = o[k];
  return typeof v === 'string' ? v : null;
}

function secretMessage(o: Record<string, unknown>): string {
  const code = o.code as string | undefined;
  const body = stringField(o, 'body') ?? stringField(o, 'message');
  switch (code) {
    case 'unavailable':
      return 'OS keychain is not available — the token cannot be stored. Check that a Secret Service / Credential Manager / Keychain is running.';
    case 'access_denied':
      return 'The keychain denied access to the token. Re-enter it in Settings.';
    case 'not_found':
      return 'No token stored — open Settings and set one.';
    case 'other':
      return body ? `Keychain error: ${body}` : 'Keychain error.';
    default:
      return body ?? 'Keychain error.';
  }
}

function kindroidMessage(o: Record<string, unknown>): string {
  const code = o.code as string | undefined;
  const body = stringField(o, 'body') ?? '';
  switch (code) {
    case 'auth':
      return 'Invalid or missing API key — check the token in Settings.';
    case 'rate_limited':
      return body
        ? `Rate limited by Kindroid. ${body}`
        : 'Rate limited by Kindroid. Try again in a moment.';
    case 'bad_request':
      return body ? `Kindroid rejected the request: ${body}` : 'Kindroid rejected the request.';
    case 'not_found':
      return 'Kindroid returned 404 — the target may have been deleted on the server.';
    case 'server':
      return body ? `Kindroid server error: ${body}` : 'Kindroid server error.';
    case 'network':
      return body ? `(network) ${body}` : '(network) request failed.';
    default:
      return body || 'Kindroid request failed.';
  }
}

function aiMessage(o: Record<string, unknown>): string {
  const code = o.code as string | undefined;
  const body = stringField(o, 'body') ?? '';
  switch (code) {
    case 'auth':
      return body
        ? `AI provider rejected the credentials: ${body}`
        : 'AI provider rejected the credentials — check the bearer token in Settings.';
    case 'rate_limited':
      return body ? `AI provider rate limited: ${body}` : 'AI provider rate limited.';
    case 'bad_request':
      return body
        ? `AI provider rejected the request: ${body}`
        : 'AI provider rejected the request.';
    case 'server':
      return body ? `AI provider server error: ${body}` : 'AI provider server error.';
    case 'network':
      return body ? `(network) ${body}` : '(network) AI request failed.';
    case 'decode':
      return body
        ? `AI provider returned an unparseable response: ${body}`
        : 'AI provider returned an unparseable response.';
    default:
      return body || 'AI request failed.';
  }
}

export const api = {
  // Characters
  listCharacters: () => invoke<Character[]>('list_characters'),
  getCharacter: (id: Uuid) => invoke<Character>('get_character', { id }),
  saveCharacter: (input: CharacterInput) => invoke<Character>('save_character', { input }),
  deleteCharacter: (id: Uuid) => invoke<void>('delete_character', { id }),
  duplicateCharacter: (id: Uuid) => invoke<Character>('duplicate_character', { id }),

  // Targets
  listTargets: () => invoke<Target[]>('list_targets'),
  getTarget: (id: Uuid) => invoke<Target>('get_target', { id }),
  saveTarget: (input: TargetInput) => invoke<Target>('save_target', { input }),
  deleteTarget: (id: Uuid) => invoke<void>('delete_target', { id }),

  // Push
  pushToTarget: (req: PushRequest) => invoke<PushResult>('push_to_target', { req }),
  pushCreateNewKin: (characterId: Uuid) =>
    invoke<CreateNewKinResult>('push_create_new_kin', { characterId }),

  // History
  listPushHistory: (limit: number, offset: number) =>
    invoke<PushLogEntry[]>('list_push_history', { limit, offset }),
  getPushLog: (id: Uuid) => invoke<PushLogEntry>('get_push_log', { id }),

  // Share images (PNG with embedded kindroid tEXt chunk)
  importShareImage: (bytes: number[] | Uint8Array) =>
    invoke<Character>('import_share_image', { bytes: Array.from(bytes) }),
  exportShareImage: (id: Uuid) => invoke<number[]>('export_share_image', { id }),
  /**
   * Read and clear the in-app stash of the last exported share image.
   * Returns `null` if the stash is empty (the user pasted a different
   * image, or the app was restarted since the last export). Used by the
   * paste handler so the in-app copy→paste round-trip works even when
   * the OS clipboard transcodes the PNG (which strips the `kindroid`
   * `tEXt` chunk on Windows WebView2 and on some Linux clipboard
   * managers / OEM Android WebViews).
   */
  takeStashedShareImage: () => invoke<number[] | null>('take_stashed_share_image'),
  setCharacterImage: (id: Uuid, bytes: number[] | Uint8Array) =>
    invoke<Character>('set_character_image', { id, bytes: Array.from(bytes) }),
  getCharacterImage: (id: Uuid) => invoke<number[] | null>('get_character_image', { id }),

  /**
   * Copy the encoded share image for `id` to the system clipboard as PNG.
   *
   * Used in place of the `<a download>` flow on Android, where Tauri 2's
   * WebView has no `DownloadListener` and silently ignores anchor download
   * clicks. Throws if the running environment lacks the async Clipboard API
   * with image support.
   */
  copyShareImageToClipboard: async (id: Uuid): Promise<void> => {
    const bytes = await invoke<number[]>('export_share_image', { id });
    const blob = new Blob([new Uint8Array(bytes)], { type: 'image/png' });
    if (
      typeof ClipboardItem === 'undefined' ||
      typeof navigator === 'undefined' ||
      !navigator.clipboard?.write
    ) {
      throw new Error('Image clipboard write not supported in this environment');
    }
    await navigator.clipboard.write([new ClipboardItem({ 'image/png': blob })]);
  },

  // Settings / token
  getSettings: () => invoke<SettingsDto>('get_settings'),
  setSettings: (input: SettingsInput) => invoke<void>('set_settings', { input }),
  tokenStatus: () => invoke<{ configured: boolean }>('token_status'),
  setToken: (token: string) => invoke<void>('set_token', { token }),
  clearToken: () => invoke<void>('clear_token'),
  testToken: () => invoke<TestTokenResult>('test_token'),
  /**
   * Persist SettingsPage debug toggles (currently just the
   * `debug_show_automation_response` flag). When ON, the chat-automation
   * cycle captures the raw AI provider response into process memory and
   * the AutomationPanel renders it for debugging. The values never
   * touch the database.
   */
  setDebugFlags: (input: { debug_show_automation_response: boolean }) =>
    invoke<void>('set_debug_flags', { input }),

  // AI provider
  getAiSettings: () => invoke<AiSettingsDto>('get_ai_settings'),
  setAiSettings: (input: { base_url: string; model: string }) =>
    invoke<void>('set_ai_settings', { input }),
  setAiToken: (token: string) => invoke<void>('set_ai_token', { token }),
  clearAiToken: () => invoke<void>('clear_ai_token'),
  testAiConnection: (input: { base_url: string; model: string; bearer_token: string | null }) =>
    invoke<TestAiResult>('test_ai_connection', { input }),
  aiChatCompletion: (input: {
    base_url: string;
    model: string;
    system: string | null;
    user: string;
    json_mode: boolean;
    bearer_token: string | null;
  }) => invoke<AiChatCompletionResponse>('ai_chat_completion', { input }),

  // Chat history. The Rust commands take separate `aiId` + `kind`
  // parameters; callers always have a `Target` (or `TargetKind`) in
  // hand when invoking these, so threading both through is deliberate
  // — forgetting the kind would silently read the wrong chat-history
  // partition when two targets share an ai_id string (one AI + one
  // Group).
  listChatMessages: (
    aiId: string,
    kind: TargetKind,
    beforeTs: number | null,
    limit: number,
    favouritesOnly: boolean,
  ) =>
    invoke<ChatMessage[]>('list_chat_messages', {
      aiId,
      kind,
      beforeTs,
      limit,
      favouritesOnly,
    }),
  searchChat: (
    aiId: string,
    kind: TargetKind,
    query: string,
    limit: number,
    offset: number,
    favouritesOnly: boolean,
  ) =>
    invoke<ChatMessage[]>('search_chat', {
      aiId,
      kind,
      query,
      limit,
      offset,
      favouritesOnly,
    }),
  chatMessageCount: (aiId: string, kind: TargetKind) =>
    invoke<number>('chat_message_count', { aiId, kind }),
  getChatSyncState: (aiId: string, kind: TargetKind) =>
    invoke<ChatSyncState | null>('get_chat_sync_state', { aiId, kind }),
  getCurrentSync: () =>
    invoke<{ ai_id: string; kind: TargetKind } | null>('get_current_sync'),
  startChatSync: (aiId: string, kind: TargetKind) =>
    invoke<void>('start_chat_sync', { aiId, kind }),
  cancelChatSync: () => invoke<void>('cancel_chat_sync'),
  resetChatHistory: (aiId: string, kind: TargetKind) =>
    invoke<number>('reset_chat_history', { aiId, kind }),
  setChatMessageFavourite: (aiId: string, kind: TargetKind, kindroidMsgId: string) =>
    invoke<boolean>('toggle_chat_message_favourite', { aiId, kind, kindroidMsgId }),

  // Journal entries (character-scoped)
  listJournalEntries: (characterId: string) =>
    invoke<JournalEntry[]>('list_journal_entries', { characterId }),
  saveJournalEntry: (characterId: string, input: JournalEntryInput) =>
    invoke<JournalEntry>('save_journal_entry', { characterId, input }),
  deleteJournalEntry: (characterId: string, entryId: string) =>
    invoke<void>('delete_journal_entry', { characterId, entryId }),

  // Character revision history (rollback)
  listCharacterRevisions: (characterId: string) =>
    invoke<CharacterRevisionSummary[]>('list_character_revisions', { characterId }),
  getCharacterRevision: (id: string) => invoke<CharacterRevision>('get_character_revision', { id }),
  restoreCharacterRevision: (characterId: string, revisionId: string) =>
    invoke<Character>('restore_character_revision', { characterId, revisionId }),

  // Chat automation (auto-journal / auto-summary)
  getChatAutomationState: (aiId: string) =>
    invoke<ChatAutomationDto>('get_chat_automation_state', { aiId }),
  setChatAutomationSettings: (input: SetChatAutomationSettingsInput) =>
    invoke<ChatAutomationDto>('set_chat_automation_settings', { input }),
  resetChatSummary: (input: ResetChatSummaryInput) =>
    invoke<ChatAutomationDto>('reset_chat_summary', { input }),
  clearStuckAutoJournalRuns: (input: ClearStuckAutoJournalRunsInput) =>
    invoke<ClearStuckAutoJournalRunsResult>('clear_stuck_auto_journal_runs', { input }),
  runSummaryNow: (input: RunSummaryNowInput) =>
    invoke<RunSummaryNowResult>('run_summary_now', { input }),
  getAutomationInstructionsDefaults: () =>
    invoke<AutomationInstructionsDefaults>('get_automation_instructions_defaults'),
  setAutomationInstructions: (input: SetAutomationInstructionsInput) =>
    invoke<void>('set_automation_instructions', { input }),
};

/**
 * Escape an arbitrary user query into a safe FTS5 expression.
 *
 * Tokens are whitespace-separated. A token wrapped in `"..."` becomes an
 * **exact phrase** match (no wildcard). An unwrapped token becomes a
 * **prefix** match (suffix `*`). All parts are joined with ` AND ` so that
 * every term (or phrase) must be present in a matching message.
 *
 * Each raw token / phrase is stripped of FTS5 metacharacters
 * (`*`, `(`, `)`, `:`, `^`), any internal `"` is doubled so it survives
 * FTS5 phrase parsing, and the cleaned text is then re-wrapped. Empty
 * parts after cleaning are dropped. An unmatched opening quote is
 * forgiving: the remainder of the input is treated as a plain unquoted
 * token rather than producing an error.
 *
 * The Porter stemmer in `chat_messages_fts` collapses inflectional
 * variants automatically, both for standalone tokens and for the words
 * inside quoted phrases.
 */
export function escapeFtsQuery(query: string): string {
  const FTS_META = /[*()^:]/g;
  const parts: string[] = [];
  const len = query.length;
  let i = 0;

  while (i < len) {
    while (i < len && /\s/.test(query[i]!)) i++;
    if (i >= len) break;

    if (query[i] === '"') {
      i++;
      const start = i;
      while (i < len && query[i] !== '"') i++;
      const raw = query.slice(start, i);
      const closed = i < len;
      if (closed) i++;
      // FTS5 tokenisation matches our linear scan: an unmatched opening
      // quote falls back to being treated as a plain token (so the user
      // still gets prefix-matching feedback instead of a silently
      // different search mode).
      const cleaned = raw.replace(FTS_META, '').replace(/"/g, '""');
      if (cleaned.length > 0) {
        parts.push(closed ? `"${cleaned}"` : `"${cleaned}"*`);
      }
    } else {
      const start = i;
      while (i < len && !/\s/.test(query[i]!) && query[i] !== '"') i++;
      const raw = query.slice(start, i);
      const cleaned = raw.replace(FTS_META, '').replace(/"/g, '""');
      if (cleaned.length > 0) parts.push(`"${cleaned}"*`);
    }
  }

  return parts.length === 0 ? '' : parts.join(' AND ');
}

/**
 * `true` when the app is running inside the Tauri 2 Android WebView.
 *
 * Detection uses `navigator.userAgent` because the WebView's UA always
 * contains "Android" (the WebView inherits Chrome's mobile UA string).
 * Cached after the first call so the work happens at most once per
 * page load.
 */
let cachedIsAndroid: boolean | null = null;
export function isAndroid(): boolean {
  if (cachedIsAndroid !== null) return cachedIsAndroid;
  if (typeof navigator === 'undefined' || !navigator.userAgent) {
    cachedIsAndroid = false;
    return false;
  }
  cachedIsAndroid = navigator.userAgent.includes('Android');
  return cachedIsAndroid;
}
