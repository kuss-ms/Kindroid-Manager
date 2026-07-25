import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useNavigate, useParams } from 'react-router-dom';
import { useEffect, useRef, useState } from 'react';
import { Controller, useForm, type UseFormRegisterReturn } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { api, errorMessage } from '../lib/api';
import { characterInputSchema, type CharacterFormValues } from '../lib/schemas';
import { toast } from '../components/Toaster';
import { ConfirmDialog } from '../components/ConfirmDialog';
import { FIELD_SOFT_LIMITS, GENDER_OPTIONS } from '../lib/types';
import type { Uuid } from '../lib/types';

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
    onSuccess: () => toast('success', 'Share image downloaded'),
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
      });
    }
  }, [character.data, reset]);

  const onSubmit = handleSubmit((v) => {
    save.mutate({ ...v, id });
  });

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
      <div className="page-header">
        <h2>{id ? 'Edit character' : 'New character'}</h2>
        <div className="page-header-actions">
          <button
            type="button"
            className="btn btn-primary"
            disabled={!isDirty && !!id}
            onClick={onSubmit}
          >
            Save
          </button>
          <button
            type="button"
            className="btn"
            onClick={() => fileInputRef.current?.click()}
            disabled={uploadImage.isPending || save.isPending}
            title={
              id
                ? 'Upload a cover image'
                : 'Save the character first (auto-saves with current form values)'
            }
          >
            {uploadImage.isPending || save.isPending ? 'Uploading…' : 'Upload image'}
          </button>
          {id && (
            <>
              <button
                type="button"
                className="btn"
                onClick={() => exportImage.mutate()}
                disabled={exportImage.isPending || !character.data?.cover_image}
                title={
                  character.data?.cover_image
                    ? 'Download a PNG with the persona embedded'
                    : 'Upload a cover image first'
                }
              >
                Export share image
              </button>
              <button
                type="button"
                className="btn btn-danger"
                onClick={() => setConfirmDelete(true)}
              >
                Delete
              </button>
            </>
          )}
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
        <Field label="Name" hint="What the AI calls itself. Sent to Kindroid as ai_name.">
          <input className="input" {...register('ai_name')} />
        </Field>
        <Field label="Gender" hint="Sent to Kindroid as ai_gender.">
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
          hint="Local-only. Not sent to Kindroid. Use the copy button on the Push screen to paste it manually into Kindroid's avatar prompt."
          control={control}
        />
        <TextAreaWithCounter
          label="Backstory"
          rows={6}
          name="ai_backstory"
          soft={FIELD_SOFT_LIMITS.ai_backstory}
          control={control}
        />
        <TextArea
          label="Key memories"
          rows={4}
          reg={register('ai_memory')}
          soft={FIELD_SOFT_LIMITS.ai_memory}
        />
        <TextAreaWithCounter
          label="Response directive"
          rows={3}
          name="ai_directive"
          soft={FIELD_SOFT_LIMITS.ai_directive}
          control={control}
        />
        <TextAreaWithCounter
          label="Example message"
          rows={2}
          name="ai_example_message"
          soft={FIELD_SOFT_LIMITS.ai_example_message}
          control={control}
        />
        <TextAreaWithCounter
          label="Additional context"
          rows={4}
          name="ai_additional_context"
          soft={FIELD_SOFT_LIMITS.ai_additional_context}
          control={control}
        />
        <TextArea
          label="Current scene"
          rows={3}
          reg={register('current_scene')}
          soft={FIELD_SOFT_LIMITS.current_scene}
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
  children,
}: {
  label: string;
  hint?: string;
  required?: boolean;
  error?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="form-row">
      <label className="form-label">
        {label}
        {required && <span style={{ color: 'var(--danger)' }}> *</span>}
        {hint && <span className="form-hint">{hint}</span>}
      </label>
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
}: {
  label: string;
  rows: number;
  reg: UseFormRegisterReturn<keyof CharacterFormValues>;
  hint?: string;
  soft?: number;
}) {
  return (
    <Field label={label} hint={hint}>
      <textarea className="textarea" {...reg} rows={rows} />
      <SoftCounter value={undefined} soft={soft} />
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
}: {
  label: string;
  rows: number;
  name: keyof CharacterFormValues;
  control: ReturnType<typeof useForm<CharacterFormValues>>['control'];
  soft?: number;
  hint?: string;
}) {
  return (
    <Field label={label} hint={hint}>
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
