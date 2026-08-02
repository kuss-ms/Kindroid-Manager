import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { useForm } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { api, errorMessage, type TargetInput } from '../lib/api';
import { targetInputSchema, type TargetFormValues } from '../lib/schemas';
import { ConfirmDialog } from '../components/ConfirmDialog';
import { toast } from '../components/Toaster';
export function TargetsPage() {
  const queryClient = useQueryClient();
  const targets = useQuery<Awaited<ReturnType<typeof api.listTargets>>>({
    queryKey: ['targets'],
    queryFn: api.listTargets,
  });
  const [editing, setEditing] = useState<TargetInput | null>(null);
  const [deleting, setDeleting] = useState<string | null>(null);
  const save = useMutation<Awaited<ReturnType<typeof api.saveTarget>>, unknown, TargetInput>({
    mutationFn: (input) => api.saveTarget(input),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['targets'] });
      toast('success', 'Saved');
      setEditing(null);
    },
    onError: (e) => toast('error', errorMessage(e)),
  });
  const del = useMutation<void, unknown, string>({
    mutationFn: (id) => api.deleteTarget(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['targets'] });
      toast('success', 'Deleted');
      setDeleting(null);
    },
    onError: (e) => toast('error', errorMessage(e)),
  });
  return (
    <div className="page">
      {' '}
      <div className="page-header">
        {' '}
        <h2>Targets</h2>{' '}
        <div className="page-header-actions">
          {' '}
          <button className="btn btn-primary" onClick={() => setEditing({ ai_id: '', label: '' })}>
            {' '}
            Add target{' '}
          </button>{' '}
        </div>{' '}
      </div>{' '}
      <p className="muted">
        {' '}
        Your AI ID is shown in Kindroid → Profile Settings.{' '}
        <a href="https://kindroid.ai/home/" target="_blank" rel="noreferrer">
          {' '}
          Open Kindroid{' '}
        </a>{' '}
      </p>{' '}
      {(targets.data ?? []).length === 0 && (
        <div className="empty">No targets yet. Add one to start pushing.</div>
      )}{' '}
      {(targets.data ?? []).length > 0 && (
        <div className="card" style={{ padding: 0 }}>
          {' '}
          <div className="list">
            {' '}
            {(targets.data ?? []).map((t) => (
              <div key={t.id} className="list-item">
                {' '}
                <div className="list-item-main">
                  {' '}
                  <div className="list-item-title">{t.label}</div>{' '}
                  <div className="list-item-sub mono">{t.ai_id}</div>{' '}
                </div>{' '}
                <div className="list-item-actions">
                  {' '}
                  <button className="btn btn-sm" onClick={() => setEditing({ ...t })}>
                    {' '}
                    Edit{' '}
                  </button>{' '}
                  <button className="btn btn-sm btn-danger" onClick={() => setDeleting(t.id)}>
                    {' '}
                    Delete{' '}
                  </button>{' '}
                </div>{' '}
              </div>
            ))}{' '}
          </div>{' '}
        </div>
      )}{' '}
      {editing && (
        <TargetDialog
          initial={editing}
          onCancel={() => setEditing(null)}
          onSave={(v) => save.mutate(v)}
        />
      )}{' '}
      <ConfirmDialog
        open={!!deleting}
        title="Delete target?"
        body="Push history referencing this target is kept, but the target row itself is removed."
        confirmLabel="Delete"
        onConfirm={() => deleting && del.mutate(deleting)}
        onCancel={() => setDeleting(null)}
      />{' '}
    </div>
  );
}
function TargetDialog({
  initial,
  onCancel,
  onSave,
}: {
  initial: TargetInput;
  onCancel: () => void;
  onSave: (input: TargetInput) => void;
}) {
  const {
    register,
    handleSubmit,
    formState: { errors },
  } = useForm<TargetFormValues>({
    resolver: zodResolver(targetInputSchema),
    defaultValues: { ai_id: initial.ai_id, label: initial.label },
  });
  return (
    <div
      className="modal-overlay"
      role="dialog"
      aria-modal="true"
      onKeyDown={(e) => {
        if (e.key === 'Escape') onCancel();
      }}
    >
      {' '}
      <div className="modal">
        {' '}
        <h3>{initial.id ? 'Edit target' : 'New target'}</h3>{' '}
        <form
          onSubmit={handleSubmit((v) =>
            onSave({ id: initial.id, ai_id: v.ai_id.trim(), label: v.label.trim() }),
          )}
          className="form"
        >
          {' '}
          <div className="form-row">
            {' '}
            <label className="form-label">
              {' '}
              Label <span style={{ color: 'var(--danger)' }}>*</span>{' '}
            </label>{' '}
            <input className="input" {...register('label')} />{' '}
            {errors.label && <span className="form-error">{errors.label.message}</span>}{' '}
          </div>{' '}
          <div className="form-row">
            {' '}
            <label className="form-label">
              {' '}
              AI ID <span style={{ color: 'var(--danger)' }}>*</span>{' '}
            </label>{' '}
            <input className="input input-mono" {...register('ai_id')} />{' '}
            {errors.ai_id && <span className="form-error">{errors.ai_id.message}</span>}{' '}
          </div>{' '}
          <div className="modal-actions">
            {' '}
            <button type="button" className="btn" onClick={onCancel}>
              {' '}
              Cancel{' '}
            </button>{' '}
            <button type="submit" className="btn btn-primary">
              {' '}
              Save{' '}
            </button>{' '}
          </div>{' '}
        </form>{' '}
      </div>{' '}
    </div>
  );
}
