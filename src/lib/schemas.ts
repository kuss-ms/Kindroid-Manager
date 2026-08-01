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
export const shareCodeSchema = z.object({
  code: z.string().trim().min(1, 'Share code is required'),
});
