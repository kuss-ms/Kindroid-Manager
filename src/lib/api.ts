import { invoke } from '@tauri-apps/api/core';
import type {
  Character,
  ChatMessage,
  ChatSyncState,
  PushLogEntry,
  PushRequest,
  PushResult,
  SettingsDto,
  Target,
  TestTokenResult,
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

  // Settings / token
  getSettings: () => invoke<SettingsDto>('get_settings'),
  setSettings: (input: SettingsInput) => invoke<void>('set_settings', { input }),
  tokenStatus: () => invoke<{ configured: boolean }>('token_status'),
  setToken: (token: string) => invoke<void>('set_token', { token }),
  clearToken: () => invoke<void>('clear_token'),
  testToken: () => invoke<TestTokenResult>('test_token'),

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
  getChatSyncState: (aiId: string) =>
    invoke<ChatSyncState | null>('get_chat_sync_state', { aiId }),
  getCurrentSync: () => invoke<string | null>('get_current_sync'),
  startChatSync: (aiId: string) => invoke<void>('start_chat_sync', { aiId }),
  cancelChatSync: () => invoke<void>('cancel_chat_sync'),
  resetChatHistory: (aiId: string) => invoke<number>('reset_chat_history', { aiId }),
  setChatMessageFavourite: (aiId: string, kindroidMsgId: string) =>
    invoke<boolean>('toggle_chat_message_favourite', { aiId, kindroidMsgId }),
};

/**
 * Escape an arbitrary user query into a safe FTS5 prefix-match expression.
 *
 * Each whitespace-separated token is double-quoted (so FTS5 treats it as
 * a literal phrase), stripped of FTS5 metacharacters, with internal `"`
 * doubled, and suffixed with `*` for prefix matching. The result is
 * `token1* OR token2* OR token3*`. The Porter stemmer in
 * `chat_messages_fts` collapses inflectional variants automatically.
 */
export function escapeFtsQuery(query: string): string {
  const FTS_META = /[*()^:]/g;
  const tokens = query
    .split(/\s+/)
    .map((t) => t.replace(FTS_META, ''))
    .filter((t) => t.length > 0);
  if (tokens.length === 0) return '';
  return tokens.map((t) => `"${t.replace(/"/g, '""')}"*`).join(' OR ');
}
