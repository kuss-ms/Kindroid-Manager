import { invoke } from '@tauri-apps/api/core';
import type {
  Character,
  ChatMessage,
  ChatSyncState,
  JournalEntry,
  JournalEntryInput,
  PushLogEntry,
  PushRequest,
  PushResult,
  CreateNewKinResult,
  SettingsDto,
  AiSettingsDto,
  AiChatCompletionResponse,
  Target,
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
}

export interface TargetInput {
  id?: Uuid;
  ai_id: string;
  label: string;
}

export interface SettingsInput {
  base_url: string;
}

/**
 * Extract a human-readable message from a Tauri invoke rejection.
 * Tauri 2 wraps the JSON-serialized `AppError` as
 * `{ message: '{"kind":"…","message":"…"}' }`. For nested AppError
 * shapes we surface the inner `message` field; for plain strings we
 * pass them through; for anything else we fall back to `String(e)`.
 */
export function errorMessage(e: unknown): string {
  if (typeof e === 'string') return e;
  if (e && typeof e === 'object') {
    const any = e as Record<string, unknown>;
    const raw = any.message;
    if (typeof raw === 'string') {
      try {
        const inner = JSON.parse(raw);
        if (inner && typeof inner === 'object' && 'message' in inner) {
          const m = (inner as Record<string, unknown>).message;
          if (typeof m === 'string') return m;
        }
      } catch {
        // Not JSON — fall through to the raw message string.
      }
      return raw;
    }
  }
  return String(e);
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

  // Chat history
  listChatMessages: (
    aiId: string,
    beforeTs: number | null,
    limit: number,
    favouritesOnly: boolean,
  ) =>
    invoke<ChatMessage[]>('list_chat_messages', {
      aiId,
      beforeTs,
      limit,
      favouritesOnly,
    }),
  searchChat: (
    aiId: string,
    query: string,
    limit: number,
    offset: number,
    favouritesOnly: boolean,
  ) =>
    invoke<ChatMessage[]>('search_chat', {
      aiId,
      query,
      limit,
      offset,
      favouritesOnly,
    }),
  chatMessageCount: (aiId: string) => invoke<number>('chat_message_count', { aiId }),
  getChatSyncState: (aiId: string) => invoke<ChatSyncState | null>('get_chat_sync_state', { aiId }),
  getCurrentSync: () => invoke<string | null>('get_current_sync'),
  startChatSync: (aiId: string) => invoke<void>('start_chat_sync', { aiId }),
  cancelChatSync: () => invoke<void>('cancel_chat_sync'),
  resetChatHistory: (aiId: string) => invoke<number>('reset_chat_history', { aiId }),
  setChatMessageFavourite: (aiId: string, kindroidMsgId: string) =>
    invoke<boolean>('toggle_chat_message_favourite', { aiId, kindroidMsgId }),

  // Journal entries (character-scoped)
  listJournalEntries: (characterId: string) =>
    invoke<JournalEntry[]>('list_journal_entries', { characterId }),
  saveJournalEntry: (characterId: string, input: JournalEntryInput) =>
    invoke<JournalEntry>('save_journal_entry', { characterId, input }),
  deleteJournalEntry: (characterId: string, entryId: string) =>
    invoke<void>('delete_journal_entry', { characterId, entryId }),
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
