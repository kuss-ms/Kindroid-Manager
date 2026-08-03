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
  user_name?: string | null;
  user_gender?: string | null;
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
  create_new_ai_status?: number;
  create_new_ai_body?: string;
  chat_break_status?: number | null;
  chat_break_body?: string | null;
  journal_entry_ids?: string[] | null;
}

export interface StepResult {
  status: number;
  ok: boolean;
  message: string;
}

export interface JournalEntryStep {
  id: string;
  status: number;
  ok: boolean;
  message: string;
}

export interface JournalEntry {
  id: string;
  character_id: Uuid;
  entry: string;
  keyphrases: string[];
  created_at: string;
  updated_at: string;
}

export interface JournalEntryInput {
  id?: string | null;
  entry: string;
  keyphrases: string[];
}

export interface PushResult {
  update_info: StepResult;
  journal_entries: JournalEntryStep[];
  chat_break?: StepResult | null;
  log_id: Uuid;
}

export interface CreateNewKinResult {
  create_new_ai: StepResult;
  update_info?: StepResult | null;
  journal_entries: JournalEntryStep[];
  log_id: Uuid;
  target: Target;
}

export interface PushRequest {
  character_id: Uuid;
  target_id: Uuid;
  fields: string[];
  chat_break?: { greeting: string; wipe_cascaded: boolean } | null;
  journal_entry_ids?: string[] | null;
}

export interface SettingsDto {
  base_url: string;
  token_configured: boolean;
  /**
   * When true, the chat-automation cycle captures the raw AI provider
   * response in process memory and the AutomationPanel renders it for
   * debugging. Lives in the `settings` table; defaults to false.
   */
  debug_show_automation_response: boolean;
}

export interface TestTokenResult {
  ok: boolean;
  rate_limited: boolean;
  message: string;
  status: number;
}

export interface AiSettingsDto {
  base_url: string;
  model: string;
  token_configured: boolean;
}

export interface TestAiResult {
  ok: boolean;
  status: number;
  message: string;
}

export interface AiChatCompletionResponse {
  content: string;
  model: string | null;
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

export type SummaryBackend = 'additional_context' | 'key_memories';

export const SUMMARY_BACKEND_LABELS: Record<SummaryBackend, string> = {
  additional_context: 'Additional context',
  key_memories: 'Key memories',
};

export const SUMMARY_BACKEND_LIMIT: Record<SummaryBackend, number> = {
  additional_context: 2500,
  key_memories: 1000,
};

export type SummaryBootstrapMode = 'full_history' | 'incremental_only';

export const BOOTSTRAP_MODE_LABELS: Record<SummaryBootstrapMode, string> = {
  full_history: 'Bootstrap from existing history',
  incremental_only: 'Incremental only (wait for new messages)',
};

export type AutoJournalRunStatus = 'pending' | 'running' | 'completed' | 'failed';
export type AutoJournalEntryStatus = 'pending' | 'sent' | 'error';

export interface StableMessageCursor {
  timestamp: number;
  kindroid_msg_id: string;
}

export interface ChatAutomationState {
  ai_id: string;
  auto_journal_enabled: boolean;
  auto_summary_enabled: boolean;
  interval: number;
  journal_cap: number;
  summary_backend: SummaryBackend;
  bootstrap_mode: SummaryBootstrapMode;
  journal_instructions_override: string | null;
  summary_instructions_override: string | null;
  journal_cursor: StableMessageCursor | null;
  summary_cursor: StableMessageCursor | null;
  journal_initialised: boolean;
  summary: string | null;
  summary_backend_stored: SummaryBackend;
  pending_summary_candidate: string | null;
  pending_summary_backend: SummaryBackend | null;
  pending_summary_created_at: string | null;
  pending_summary_cursor: StableMessageCursor | null;
  pending_reformat: boolean;
  journal_last_error: string | null;
  summary_last_error: string | null;
  journal_last_run_at: string | null;
  summary_last_run_at: string | null;
}

export interface AutoJournalRun {
  id: string;
  ai_id: string;
  start_cursor: StableMessageCursor | null;
  end_cursor: StableMessageCursor | null;
  status: AutoJournalRunStatus;
  attempts: number;
  completed_at: string | null;
  last_error: string | null;
  created_at: string;
}

export interface AutoJournalEntry {
  id: string;
  run_id: string;
  ai_id: string;
  entry: string;
  keyphrases: string[];
  source_start: StableMessageCursor | null;
  source_end: StableMessageCursor | null;
  status: AutoJournalEntryStatus;
  response_status: number | null;
  response_message: string | null;
  created_at: string;
  updated_at: string;
}

export interface ChatAutomationDto {
  state: ChatAutomationState;
  journal_instructions: string;
  summary_instructions: string;
  recent_journal_entries: AutoJournalEntry[];
  automation_in_progress: boolean;
  /**
   * Raw AI provider response from the most recent journal cycle. Populated
   * only when the SettingsPage debug toggle `debug_show_automation_response`
   * is ON; otherwise `undefined`. Lives in process memory only — never
   * written to the database.
   */
  journal_last_response_debug?: string;
  /** Same semantics as `journal_last_response_debug` but for summary. */
  summary_last_response_debug?: string;
}

export interface SetChatAutomationSettingsInput {
  ai_id: string;
  auto_journal_enabled: boolean;
  auto_summary_enabled: boolean;
  interval: number;
  journal_cap: number;
  summary_backend: SummaryBackend;
  bootstrap_mode: SummaryBootstrapMode;
  journal_instructions_override: string | null;
  summary_instructions_override: string | null;
}

export interface ResetChatSummaryInput {
  ai_id: string;
}

export interface ClearStuckAutoJournalRunsInput {
  ai_id: string;
}

export interface ClearStuckAutoJournalRunsResult {
  removed: number;
}

export interface RunSummaryNowInput {
  ai_id: string;
}

export interface RunSummaryNowResult {
  ran: boolean;
  message: string;
}

export interface SetAutomationInstructionsInput {
  journal: string;
  summary: string;
}

export interface AutomationInstructionsDefaults {
  journal: string;
  summary: string;
}
