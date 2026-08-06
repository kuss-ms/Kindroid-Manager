import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useNavigate, useParams } from 'react-router-dom';
import { useEffect, useMemo, useRef, useState } from 'react';
import { Controller, useForm, type UseFormRegisterReturn } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { api, errorMessage, isAndroid } from '../lib/api';
import { characterInputSchema, type CharacterFormValues } from '../lib/schemas';
import { toast } from '../components/Toaster';
import { ConfirmDialog } from '../components/ConfirmDialog';
import { JournalEntryForm } from '../components/JournalEntryForm';
import { RowActions, type RowAction } from '../components/RowActions';
import { FIELD_SOFT_LIMITS, GENDER_OPTIONS, PERSONA_FIELD_LABELS } from '../lib/types';
import type { JournalEntry, JournalEntryInput, PersonaField, Uuid } from '../lib/types';

const MAX_NOTE = 5000;

export function CharacterEditorPage() {
  const params = useParams();
  const id = params.id as Uuid | undefined;
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [imageUrl, setImageUrl] = useState<string | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);

  const character = useQuery({
    queryKey: ['character', id ?? 'new'],
    queryFn: () => (id ? api.getCharacter(id) : Promise.resolve(null)),
    enabled: !!id,
  });

  const targets = useQuery({
    queryKey: ['targets'],
    queryFn: () => api.listTargets(),
  });

  const aiTargets = useMemo(
    () => (targets.data ?? []).filter((t) => t.kind === 'ai'),
    [targets.data],
  );

  const save = useMutation({
    mutationFn: (values: CharacterFormValues & { id?: Uuid }) =>
      api.saveCharacter({ ...values, id: values.id }),
    onSuccess: (c) => {
      queryClient.invalidateQueries({ queryKey: ['characters'] });
      queryClient.setQueryData(['character', c.id], c);
      toast('success', 'Saved');
      if (!id) navigate(`/characters/${c.id}`, { replace: true });
    },
    onError: (e) => toast('error', errorMessage(e)),
  });

  const del = useMutation({
    mutationFn: (id: Uuid) => api.deleteCharacter(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['characters'] });
      toast('success', 'Deleted');
      navigate('/characters');
    },
    onError: (e) => toast('error', errorMessage(e)),
  });

  const uploadImage = useMutation<Awaited<ReturnType<typeof api.setCharacterImage>>, unknown, File>(
    {
      mutationFn: async (file) => {
        const buf = await file.arrayBuffer();
        if (!id) throw new Error('Save the character first');
        return api.setCharacterImage(id, new Uint8Array(buf));
      },
      onSuccess: (c) => {
        queryClient.setQueryData(['character', c.id], c);
        queryClient.invalidateQueries({ queryKey: ['characters'] });
        toast('success', 'Image uploaded');
      },
      onError: (e) => toast('error', errorMessage(e)),
    },
  );

  const exportImage = useMutation<void, unknown, void>({
    mutationFn: async () => {
      if (!id) throw new Error('Save the character first');
      if (isAndroid()) {
        await api.copyShareImageToClipboard(id);
        return;
      }
      const bytes = await api.exportShareImage(id);
      const blob = new Blob([new Uint8Array(bytes)], { type: 'image/png' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      const aiName = character.data?.ai_name ?? character.data?.name ?? 'character';
      const safeName = aiName.replace(/[^\w.-]+/g, '_');
      a.href = url;
      a.download = `${safeName}.png`;
      document.body.appendChild(a);
      a.click();
      a.remove();
      URL.revokeObjectURL(url);
    },
    onSuccess: () =>
      toast('success', isAndroid() ? 'Share image copied to clipboard' : 'Share image downloaded'),
    onError: (e) => toast('error', errorMessage(e)),
  });

  useEffect(() => {
    let active = true;
    let url: string | null = null;
    if (id && character.data?.cover_image) {
      api
        .getCharacterImage(id)
        .then((bytes) => {
          if (!active || !bytes) return;
          const blob = new Blob([new Uint8Array(bytes)]);
          url = URL.createObjectURL(blob);
          setImageUrl(url);
        })
        .catch((e) => toast('error', errorMessage(e)));
    } else {
      setImageUrl(null);
    }
    return () => {
      active = false;
      if (url) URL.revokeObjectURL(url);
    };
  }, [id, character.data?.cover_image]);

  const onPickImage = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    e.target.value = '';
    if (!file) return;
    const previewUrl = URL.createObjectURL(file);
    setImageUrl(previewUrl);
    if (id) {
      uploadImage.mutate(file);
      return;
    }
    const valid = await new Promise<CharacterFormValues | null>((resolve) => {
      handleSubmit((v) => resolve(v))();
    });
    if (!valid) {
      URL.revokeObjectURL(previewUrl);
      setImageUrl(null);
      return;
    }
    save.mutate(
      { ...valid, id: undefined },
      {
        onSuccess: async (c) => {
          queryClient.setQueryData(['character', c.id], c);
          queryClient.invalidateQueries({ queryKey: ['characters'] });
          navigate(`/characters/${c.id}`, { replace: true });
          const buf = await file.arrayBuffer();
          try {
            const updated = await api.setCharacterImage(c.id, new Uint8Array(buf));
            queryClient.setQueryData(['character', updated.id], updated);
            toast('success', 'Saved and image uploaded');
          } catch (e) {
            toast('error', errorMessage(e));
          }
        },
        onError: (e) => toast('error', errorMessage(e)),
      },
    );
  };

  const {
    register,
    handleSubmit,
    reset,
    control,
    watch,
    formState: { errors, isDirty },
  } = useForm<CharacterFormValues>({
    resolver: zodResolver(characterInputSchema),
    defaultValues: {
      name: '',
      ai_name: undefined,
      ai_gender: undefined,
      ai_backstory: undefined,
      ai_memory: undefined,
      ai_directive: undefined,
      ai_example_message: undefined,
      ai_additional_context: undefined,
      current_scene: undefined,
      greeting: undefined,
      notes: undefined,
      ai_avatar_description: undefined,
      default_target_id: undefined,
    },
  });

  useEffect(() => {
    if (character.data) {
      reset({
        name: character.data.name,
        ai_name: character.data.ai_name ?? undefined,
        ai_gender: character.data.ai_gender ?? undefined,
        ai_backstory: character.data.ai_backstory ?? undefined,
        ai_memory: character.data.ai_memory ?? undefined,
        ai_directive: character.data.ai_directive ?? undefined,
        ai_example_message: character.data.ai_example_message ?? undefined,
        ai_additional_context: character.data.ai_additional_context ?? undefined,
        current_scene: character.data.current_scene ?? undefined,
        greeting: character.data.greeting ?? undefined,
        notes: character.data.notes ?? undefined,
        ai_avatar_description: character.data.ai_avatar_description ?? undefined,
        default_target_id: character.data.default_target_id ?? undefined,
      });
    }
  }, [character.data, reset]);

  const onSubmit = handleSubmit((v) => {
    save.mutate({ ...v, default_target_id: v.default_target_id || null, id });
  });

  const defaultTargetLabel = (): string | null => {
    const dtid = character.data?.default_target_id;
    if (!dtid) return null;
    return aiTargets.find((t) => t.id === dtid)?.label ?? null;
  };

  const saveAndGet = (): Promise<CharacterFormValues | null> =>
    new Promise((resolve) => {
      handleSubmit(
        async (v) => {
          try {
            await save.mutateAsync({ ...v, default_target_id: v.default_target_id || null, id });
            resolve(v);
          } catch {
            resolve(null);
          }
        },
        () => resolve(null),
      )();
    });

  const pushField = useMutation({
    mutationFn: async (field: PersonaField) => {
      if (!id || !character.data?.default_target_id) {
        throw new Error('No default push target');
      }
      const saved = await saveAndGet();
      if (!saved) throw new Error('Save failed');
      return api.pushToTarget({ character_id: id, target_id: character.data.default_target_id, fields: [field] });
    },
    onSuccess: (_res, field) => {
      const label = PERSONA_FIELD_LABELS[field];
      const target = defaultTargetLabel() ?? 'target';
      toast('success', `Pushed ${label} to ${target}`);
      queryClient.invalidateQueries({ queryKey: ['push-history'] });
      queryClient.invalidateQueries({ queryKey: ['targets'] });
    },
    onError: (e) => toast('error', errorMessage(e)),
  });

  const pushAction = (field: PersonaField): React.ReactNode =>
    id && character.data ? (
      <PushFieldButton
        field={field}
        value={watch(field) as string | null | undefined}
        defaultTargetLabel={defaultTargetLabel()}
        busy={pushField.isPending || save.isPending}
        onPush={() => pushField.mutate(field)}
      />
    ) : null;

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === 's') {
        e.preventDefault();
        onSubmit();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  });

  return (
    <div className="page">
      {id && character.isError && (
        <div className="error" role="alert" data-testid="character-load-error">
          Failed to load character: {errorMessage(character.error)}
        </div>
      )}
      {id && character.isLoading && <p className="muted">Loading character…</p>}
      <div className="page-header">
        <h2>{id ? 'Edit character' : 'New character'}</h2>
        <div className="page-header-actions">
          <RowActions
            actions={
              [
                {
                  label: 'Save',
                  onClick: onSubmit,
                  disabled: !isDirty && !!id,
                },
                {
                  label: 'Upload image',
                  onClick: () => fileInputRef.current?.click(),
                  disabled: uploadImage.isPending || save.isPending,
                  title: id
                    ? 'Upload a cover image'
                    : 'Save the character first (auto-saves with current form values)',
                },
                ...(id
                  ? [
                      {
                        label: 'Export share image',
                        onClick: () => exportImage.mutate(),
                        disabled: exportImage.isPending || !character.data?.cover_image,
                        title: character.data?.cover_image
                          ? isAndroid()
                            ? 'Copy a PNG with the persona + journal entries embedded to the clipboard'
                            : 'Download a PNG with the persona + journal entries embedded'
                          : 'Upload a cover image first',
                      },
                      {
                        label: 'View history',
                        onClick: () => navigate(`/characters/${id}/history`),
                        disabled: !character.data,
                        title: 'View and restore previous saved versions',
                      },
                      {
                        label: 'Delete',
                        danger: true,
                        onClick: () => setConfirmDelete(true),
                      },
                    ]
                  : []),
              ] satisfies RowAction[]
            }
          />
        </div>
      </div>

      <input
        ref={fileInputRef}
        type="file"
        accept="image/*"
        onChange={onPickImage}
        style={{ display: 'none' }}
        data-testid="image-upload"
      />

      <div className="card">
        <h3>Cover image</h3>
        <p className="muted" style={{ fontSize: 12, marginTop: 6 }}>
          Shown on this character and embedded in the share image you export. Any existing kindroid
          metadata in the uploaded image is stripped.
        </p>
        {imageUrl ? (
          <div className="image-preview" style={{ marginTop: 12 }}>
            <img src={imageUrl} alt="Cover" />
          </div>
        ) : (
          <div className="image-preview-empty" style={{ marginTop: 12 }}>
            No cover image yet.
          </div>
        )}
      </div>

      <form onSubmit={onSubmit} className="card form">
        <Field
          label="Local label"
          required
          hint="Used in the push combobox. Not sent to Kindroid."
          error={errors.name?.message}
        >
          <input className="input" {...register('name')} />
        </Field>

        <h3 style={{ marginTop: 8 }}>Kindroid</h3>
        <Field
          label="Name"
          hint="What the AI calls itself. Sent to Kindroid as ai_name."
          action={pushAction('ai_name')}
        >
          <input className="input" {...register('ai_name')} />
        </Field>
        <Field
          label="Gender"
          hint="Sent to Kindroid as ai_gender."
          action={pushAction('ai_gender')}
        >
          <select className="select" {...register('ai_gender')}>
            {GENDER_OPTIONS.map((opt) => (
              <option key={opt.value} value={opt.value}>
                {opt.label}
              </option>
            ))}
          </select>
        </Field>
        <TextAreaWithCounter
          label="Avatar Description"
          rows={4}
          name="ai_avatar_description"
          soft={FIELD_SOFT_LIMITS.ai_avatar_description}
          hint="Sent to Kindroid only when pushing as a new Kin. Ignored on regular pushes to an existing AI."
          control={control}
        />
        <TextAreaWithCounter
          label="Backstory"
          rows={6}
          name="ai_backstory"
          soft={FIELD_SOFT_LIMITS.ai_backstory}
          action={pushAction('ai_backstory')}
          control={control}
        />
        <TextArea
          label="Key memories"
          rows={4}
          reg={register('ai_memory')}
          soft={FIELD_SOFT_LIMITS.ai_memory}
          value={watch('ai_memory') ?? ''}
          action={pushAction('ai_memory')}
        />
        <TextAreaWithCounter
          label="Response directive"
          rows={3}
          name="ai_directive"
          soft={FIELD_SOFT_LIMITS.ai_directive}
          action={pushAction('ai_directive')}
          control={control}
        />
        <TextAreaWithCounter
          label="Example message"
          rows={2}
          name="ai_example_message"
          soft={FIELD_SOFT_LIMITS.ai_example_message}
          action={pushAction('ai_example_message')}
          control={control}
        />
        <TextAreaWithCounter
          label="Additional context"
          rows={4}
          name="ai_additional_context"
          soft={FIELD_SOFT_LIMITS.ai_additional_context}
          action={pushAction('ai_additional_context')}
          control={control}
        />
        <TextArea
          label="Current scene"
          rows={3}
          reg={register('current_scene')}
          soft={FIELD_SOFT_LIMITS.current_scene}
          value={watch('current_scene') ?? ''}
          action={pushAction('current_scene')}
        />

        <h3 style={{ marginTop: 8 }}>Greeting</h3>
        <TextAreaWithCounter
          label="Greeting"
          rows={3}
          name="greeting"
          soft={2000}
          hint="The AI's opening line. Used by the chat-break action — the push dialog pre-fills it when you enable chat-break. Not sent by /update-info."
          control={control}
        />

        <Field label="Notes (local only)" hint="Not sent to Kindroid, not in share code.">
          <textarea className="textarea" {...register('notes')} rows={3} maxLength={MAX_NOTE} />
        </Field>
      </form>

      <div className="card">
        <Field
          label="Default push target"
          hint="Pre-selected on the Push page. Group targets can't be the default because they can't be pushed to."
        >
          <Controller
            name="default_target_id"
            control={control}
            render={({ field }) => (
              <select
                className="select"
                data-testid="default-target-select"
                value={
                  field.value && aiTargets.some((t) => t.id === field.value)
                    ? field.value
                    : ''
                }
                onChange={(e) => field.onChange(e.target.value)}
                style={{ marginTop: 8 }}
              >
                <option value="">— none —</option>
                {aiTargets.map((t) => (
                  <option key={t.id} value={t.id}>
                    {t.label} — {t.ai_id}
                  </option>
                ))}
              </select>
            )}
          />
        </Field>
      </div>

      {id ? (
        <JournalEditor characterId={id} />
      ) : (
        <div className="card muted" style={{ fontSize: 12 }}>
          Save the character first to enable journal entries.
        </div>
      )}

      <ConfirmDialog
        open={confirmDelete}
        title="Delete character?"
        body={`Permanently delete "${character.data?.name ?? ''}"? This cannot be undone.`}
        confirmLabel="Delete"
        onConfirm={() => {
          if (id) del.mutate(id);
        }}
        onCancel={() => setConfirmDelete(false)}
      />
    </div>
  );
}

function Field({
  label,
  hint,
  required,
  error,
  action,
  children,
}: {
  label: string;
  hint?: string;
  required?: boolean;
  error?: string;
  action?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <div className="form-row">
      <div className="form-row-header">
        <label className="form-label">
          {label}
          {required && <span style={{ color: 'var(--danger)' }}> *</span>}
          {hint && <span className="form-hint">{hint}</span>}
        </label>
        {action && <div className="form-row-action">{action}</div>}
      </div>
      {children}
      {error && <span className="form-error">{error}</span>}
    </div>
  );
}

function TextArea({
  label,
  rows,
  reg,
  hint,
  soft,
  value,
  action,
}: {
  label: string;
  rows: number;
  reg: UseFormRegisterReturn<keyof CharacterFormValues>;
  hint?: string;
  soft?: number;
  // Pre-watched value so the SoftCounter can show the current length
  // (was `undefined` before, which made the counter always read 0 —
  // see audit M14).
  value?: string;
  action?: React.ReactNode;
}) {
  return (
    <Field label={label} hint={hint} action={action}>
      <textarea className="textarea" {...reg} rows={rows} />
      <SoftCounter value={value} soft={soft} />
    </Field>
  );
}

function TextAreaWithCounter({
  label,
  rows,
  name,
  control,
  soft,
  hint,
  action,
}: {
  label: string;
  rows: number;
  name: keyof CharacterFormValues;
  control: ReturnType<typeof useForm<CharacterFormValues>>['control'];
  soft?: number;
  hint?: string;
  action?: React.ReactNode;
}) {
  return (
    <Field label={label} hint={hint} action={action}>
      <Controller
        control={control}
        name={name}
        render={({ field }) => {
          const value = typeof field.value === 'string' ? field.value : '';
          const len = value.length;
          const warn = soft != null && len > soft * 0.8;
          return (
            <div>
              <textarea
                ref={field.ref}
                rows={rows}
                name={field.name}
                value={value}
                onChange={field.onChange}
                onBlur={field.onBlur}
                className="textarea"
                style={warn ? { borderColor: 'var(--warning)' } : undefined}
              />
              <SoftCounter value={value} soft={soft} />
            </div>
          );
        }}
      />
    </Field>
  );
}

function SoftCounter({ value, soft }: { value: string | undefined; soft: number | undefined }) {
  if (soft == null) return null;
  const len = value?.length ?? 0;
  const warn = soft != null && len > soft * 0.8;
  return (
    <div className={`soft-counter ${warn ? 'warn' : ''}`}>
      {len} / {soft}
    </div>
  );
}

export function PushFieldButton({
  field,
  value,
  defaultTargetLabel,
  busy,
  onPush,
}: {
  field: PersonaField;
  value: string | null | undefined;
  defaultTargetLabel: string | null;
  busy: boolean;
  onPush: () => void;
}) {
  const empty = value == null || value === '';
  const noTarget = defaultTargetLabel == null;
  const disabled = empty || noTarget || busy;
  let title = `Push ${PERSONA_FIELD_LABELS[field]} to ${defaultTargetLabel ?? 'default target'}`;
  if (busy) title = 'Busy…';
  else if (noTarget) title = 'Set a default push target to enable push.';
  else if (empty) title = 'Field is empty.';

  return (
    <button
      type="button"
      className="btn btn-sm"
      disabled={disabled}
      title={title}
      aria-label={`Push ${PERSONA_FIELD_LABELS[field]}`}
      data-testid={`push-field-${field}`}
      onClick={onPush}
    >
      Push
    </button>
  );
}

function JournalEditor({ characterId }: { characterId: Uuid }) {
  const queryClient = useQueryClient();
  const entries = useQuery({
    queryKey: ['journal-entries', characterId],
    queryFn: () => api.listJournalEntries(characterId),
  });
  const [editing, setEditing] = useState<JournalEntryInput | null>(null);
  const [confirmDelete, setConfirmDelete] = useState<string | null>(null);

  const save = useMutation({
    mutationFn: (input: JournalEntryInput) => api.saveJournalEntry(characterId, input),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['journal-entries', characterId] });
      setEditing(null);
      toast('success', 'Journal entry saved');
    },
    onError: (e) => toast('error', errorMessage(e)),
  });

  const del = useMutation({
    mutationFn: (entryId: string) => api.deleteJournalEntry(characterId, entryId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['journal-entries', characterId] });
      toast('success', 'Journal entry deleted');
    },
    onError: (e) => toast('error', errorMessage(e)),
  });

  return (
    <div className="card">
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
        }}
      >
        <h3 style={{ margin: 0 }}>Journal entries</h3>
        {editing == null && (
          <button
            type="button"
            className="btn btn-sm"
            onClick={() => setEditing({ entry: '', keyphrases: [] })}
          >
            Add entry
          </button>
        )}
      </div>
      <p className="muted" style={{ fontSize: 12, marginTop: 6 }}>
        Up to 500 characters and up to 8 specific, 1-3 word keyphrases per entry (Kindroid&apos;s
        recall is verbatim on the user&apos;s input, so generic single common words like
        &quot;love&quot; or &quot;wings&quot; hurt recall — pick proper nouns, dates, distinctive
        compounds). Not sent unless you tick the entry on the Push page.
      </p>

      {editing && (
        <JournalEntryForm
          initial={editing}
          onSubmit={(v) => save.mutate(v)}
          onCancel={() => setEditing(null)}
          submitting={save.isPending}
        />
      )}

      {entries.isLoading && <p className="muted">Loading…</p>}
      {entries.isError && (
        <div className="error" role="alert" data-testid="journal-error">
          Failed to load journal entries: {errorMessage(entries.error)}
        </div>
      )}
      {(entries.data ?? []).length === 0 && !entries.isLoading && !entries.isError && editing == null && (
        <div className="empty" style={{ marginTop: 12 }}>
          No journal entries yet.
        </div>
      )}
      <ul style={{ listStyle: 'none', padding: 0, marginTop: 12 }}>
        {(entries.data ?? []).map((e: JournalEntry) => (
          <li
            key={e.id}
            style={{
              borderTop: '1px solid var(--border)',
              padding: '8px 0',
            }}
          >
            <div style={{ display: 'flex', justifyContent: 'space-between', gap: 8 }}>
              <div style={{ flex: 1 }}>
                <div style={{ whiteSpace: 'pre-wrap' }}>{e.entry}</div>
                {e.keyphrases.length > 0 && (
                  <div
                    style={{
                      display: 'flex',
                      gap: 4,
                      flexWrap: 'wrap',
                      marginTop: 6,
                    }}
                  >
                    {e.keyphrases.map((k) => (
                      <span key={k} className="badge badge-muted">
                        {k}
                      </span>
                    ))}
                  </div>
                )}
                <div className="muted" style={{ fontSize: 11, marginTop: 4 }}>
                  updated {new Date(e.updated_at).toLocaleString()}
                </div>
              </div>
              <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
                <button
                  type="button"
                  className="btn btn-sm"
                  onClick={() =>
                    setEditing({
                      id: e.id,
                      entry: e.entry,
                      keyphrases: e.keyphrases,
                    })
                  }
                >
                  Edit
                </button>
                <button
                  type="button"
                  className="btn btn-sm btn-danger"
                  onClick={() => setConfirmDelete(e.id)}
                >
                  Delete
                </button>
              </div>
            </div>
          </li>
        ))}
      </ul>
      <ConfirmDialog
        open={confirmDelete != null}
        title="Delete journal entry?"
        body="This cannot be undone. The entry is local; nothing on the Kindroid server is touched."
        confirmLabel="Delete"
        onConfirm={() => {
          if (confirmDelete) del.mutate(confirmDelete);
          setConfirmDelete(null);
        }}
        onCancel={() => setConfirmDelete(null)}
      />
    </div>
  );
}
