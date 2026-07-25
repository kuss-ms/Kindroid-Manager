import { invoke } from '@tauri-apps/api/core';
import type {
  Character,
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
};
