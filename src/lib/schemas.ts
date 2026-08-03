import { z } from 'zod';
export const optionalString = z.preprocess(
  (v) => (typeof v === 'string' && v.trim() === '' ? undefined : v),
  z.string().trim().min(1).optional(),
);
export const characterInputSchema = z.object({
  name: z.string().trim().min(1, 'Local label is required'),
  ai_name: optionalString,
  ai_gender: optionalString,
  ai_backstory: optionalString,
  ai_memory: optionalString,
  ai_directive: optionalString,
  ai_example_message: optionalString,
  ai_additional_context: optionalString,
  current_scene: optionalString,
  greeting: optionalString,
  notes: z.string().optional(),
  ai_avatar_description: optionalString,
});

export type CharacterFormValues = z.infer<typeof characterInputSchema>;
export const targetInputSchema = z.object({
  ai_id: z.string().trim().min(1, 'AI ID is required'),
  label: z.string().trim().min(1, 'Label is required'),
});
export type TargetFormValues = z.infer<typeof targetInputSchema>;
export const settingsSchema = z.object({ base_url: z.string().trim().url('Must be a valid URL') });
export const aiSettingsSchema = z.object({
  base_url: z
    .string()
    .trim()
    .url('Must be a valid URL')
    .refine((u) => /^https?:\/\//.test(u), {
      message: 'must start with http:// or https://',
    }),
  model: z.string(),
});
export type AiSettingsForm = z.infer<typeof aiSettingsSchema>;

// Zod schemas for the AI provider command payloads. These mirror the
// Rust request structs in `src-tauri/src/kindroid/ai.rs` and are
// optional — the Tauri commands accept a single `input` struct — but
// validating client-side surfaces malformed URLs or empty user text
// before they hit the network.

const httpUrl = z
  .string()
  .trim()
  .min(1, 'URL is required')
  .refine((u) => /^https?:\/\//.test(u), 'must start with http:// or https://');

export const testAiRequestSchema = z.object({
  base_url: httpUrl,
  model: z.string().trim().min(1, 'Model is required'),
  bearer_token: z.string().nullable(),
});
export type TestAiRequestForm = z.infer<typeof testAiRequestSchema>;

export const aiChatCompletionRequestSchema = z.object({
  base_url: httpUrl,
  model: z.string().trim().min(1, 'Model is required'),
  system: z.string().nullable(),
  user: z.string().trim().min(1, 'User message is required'),
  json_mode: z.boolean(),
  bearer_token: z.string().nullable(),
});
export type AiChatCompletionRequestForm = z.infer<typeof aiChatCompletionRequestSchema>;

const noPlaceholderSyntax = (val: string) => !val.includes('{{') && !val.includes('}}');

export const chatAutomationSettingsSchema = z.object({
  // Fields below must mirror every field in
  // `src-tauri/src/commands/chat_automation.rs::SetChatAutomationSettingsInput`
  // — a previous audit (M2) found that this schema silently stripped
  // `ai_id` / `auto_journal_enabled` / `auto_summary_enabled`, so the
  // Tauri command received an incomplete payload. Re-adding them
  // here is the safe half of the fix; the unsafe half was already
  // mitigated on the Rust side (the inner `code` tag, see H1).
  ai_id: z.string().trim().min(1, 'AI id is required'),
  auto_journal_enabled: z.boolean(),
  auto_summary_enabled: z.boolean(),
  interval: z
    .number({ invalid_type_error: 'Interval must be a number' })
    .int('Interval must be a whole number')
    .min(2, 'Interval must be at least 2'),
  journal_cap: z
    .number({ invalid_type_error: 'Journal cap must be a number' })
    .int('Journal cap must be a whole number')
    .min(1, 'Journal cap must be at least 1')
    .max(3, 'Journal cap must be at most 3'),
  summary_backend: z.enum(['additional_context', 'key_memories']),
  bootstrap_mode: z.enum(['full_history', 'incremental_only']),
  journal_instructions_override: z
    .string()
    .max(4000, 'Instructions are too long (4000 char max)')
    .refine(noPlaceholderSyntax, 'Placeholder syntax ({{ }}) is not allowed')
    .optional()
    .nullable(),
  summary_instructions_override: z
    .string()
    .max(4000, 'Instructions are too long (4000 char max)')
    .refine(noPlaceholderSyntax, 'Placeholder syntax ({{ }}) is not allowed')
    .optional()
    .nullable(),
});
export type ChatAutomationSettingsForm = z.infer<typeof chatAutomationSettingsSchema>;

export const automationInstructionsSchema = z.object({
  journal: z
    .string()
    .max(4000, 'Instructions are too long (4000 char max)')
    .refine(noPlaceholderSyntax, 'Placeholder syntax ({{ }}) is not allowed'),
  summary: z
    .string()
    .max(4000, 'Instructions are too long (4000 char max)')
    .refine(noPlaceholderSyntax, 'Placeholder syntax ({{ }}) is not allowed'),
});
export type AutomationInstructionsForm = z.infer<typeof automationInstructionsSchema>;
