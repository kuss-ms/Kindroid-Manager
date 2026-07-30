export type Uuid = string;

export interface Character {
  id: Uuid;
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
  created_at: string;
  updated_at: string;
}

export interface Target {
  id: Uuid;
  ai_id: string;
  label: string;
  created_at: string;
}

export interface PushLogEntry {
  id: Uuid;
  at: string;
  character_id: Uuid;
  character_name: string;
  target_id: Uuid;
  target_ai_id: string;
  fields_sent: string[];
  did_chat_break: boolean;
  greeting?: string | null;
  wipe_cascaded?: boolean | null;
  update_info_status: number;
  update_info_body: string;
  chat_break_status?: number | null;
  chat_break_body?: string | null;
}

export interface StepResult {
  status: number;
  ok: boolean;
  message: string;
}

export interface PushResult {
  update_info: StepResult;
  chat_break?: StepResult | null;
  log_id: Uuid;
}

export interface PushRequest {
  character_id: Uuid;
  target_id: Uuid;
  fields: string[];
  chat_break?: { greeting: string; wipe_cascaded: boolean } | null;
}

export interface SettingsDto {
  base_url: string;
  token_configured: boolean;
}

export interface TestTokenResult {
  ok: boolean;
  rate_limited: boolean;
  message: string;
  status: number;
}

export interface ChatMessage {
  id: Uuid;
  ai_id: string;
  kindroid_msg_id: string;
  sender: string;
  sender_type: string;
  display_name: string | null;
  timestamp: number;
  message: string;
  image_urls: string[];
  image_description: string | null;
  video_description: string | null;
  internet_response: string | null;
  link_url: string | null;
  link_description: string | null;
  fetched_at: string;
  favourite: boolean;
}

export type SyncStatusKind = 'idle' | 'running' | 'backoff' | 'cancelled' | 'error';

export interface ChatSyncState {
  ai_id: string;
  last_synced_at: string;
  last_timestamp: number;
  full_sync_done: boolean;
  is_syncing: boolean;
  status_kind: SyncStatusKind;
  status_message: string | null;
  backoff_until: string | null;
  total: number;
}

export const PERSONA_FIELDS = [
  'ai_name',
  'ai_gender',
  'ai_backstory',
  'ai_memory',
  'ai_directive',
  'ai_example_message',
  'ai_additional_context',
  'current_scene',
] as const;

export type PersonaField = (typeof PERSONA_FIELDS)[number];

export const PERSONA_FIELD_LABELS: Record<PersonaField, string> = {
  ai_name: 'Name',
  ai_gender: 'Gender',
  ai_backstory: 'Backstory',
  ai_memory: 'Key memories',
  ai_directive: 'Response directive',
  ai_example_message: 'Example message',
  ai_additional_context: 'Additional context',
  current_scene: 'Current scene',
};

export const AI_FIELDS: PersonaField[] = [
  'ai_name',
  'ai_gender',
  'ai_backstory',
  'ai_memory',
  'ai_directive',
  'ai_example_message',
  'ai_additional_context',
  'current_scene',
];

export const GENDER_OPTIONS = [
  { value: '', label: '—' },
  { value: 'Female', label: 'Female' },
  { value: 'Male', label: 'Male' },
] as const;

/** Soft character limits shown in the editor's counter. */
export const FIELD_SOFT_LIMITS: Partial<Record<PersonaField, number>> & {
  ai_avatar_description: number;
} = {
  ai_backstory: 2500,
  ai_memory: 1000,
  ai_directive: 250,
  ai_example_message: 750,
  ai_additional_context: 2500,
  ai_avatar_description: 800,
};
