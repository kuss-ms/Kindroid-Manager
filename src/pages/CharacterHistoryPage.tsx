import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useNavigate, useParams } from 'react-router-dom';
import { useState } from 'react';
import { api, errorMessage } from '../lib/api';
import type {
  CharacterRevision,
  CharacterRevisionSummary,
  CharacterSnapshotFields,
  Uuid,
} from '../lib/types';
import { ConfirmDialog } from '../components/ConfirmDialog';
import { toast } from '../components/Toaster';

const PERSONA_LABELS: Array<{ key: keyof CharacterSnapshotFields; label: string }> = [
  { key: 'name', label: 'Local label' },
  { key: 'ai_name', label: 'Name' },
  { key: 'ai_gender', label: 'Gender' },
  { key: 'ai_backstory', label: 'Backstory' },
  { key: 'ai_memory', label: 'Key memories' },
  { key: 'ai_directive', label: 'Response directive' },
  { key: 'ai_example_message', label: 'Example message' },
  { key: 'ai_additional_context', label: 'Additional context' },
  { key: 'current_scene', label: 'Current scene' },
  { key: 'user_name', label: 'User name' },
  { key: 'user_gender', label: 'User gender' },
  { key: 'greeting', label: 'Greeting' },
  { key: 'notes', label: 'Notes' },
  { key: 'ai_avatar_description', label: 'Avatar description' },
];

export function CharacterHistoryPage() {
  const params = useParams();
  const id = params.id as Uuid | undefined;
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [confirmRestore, setConfirmRestore] = useState<string | null>(null);

  const character = useQuery({
    queryKey: ['character', id ?? 'new'],
    queryFn: () => (id ? api.getCharacter(id) : Promise.resolve(null)),
    enabled: !!id,
  });

  const revisions = useQuery<CharacterRevisionSummary[]>({
    queryKey: ['character-revisions', id],
    queryFn: () => api.listCharacterRevisions(id!),
    enabled: !!id,
  });

  const restore = useMutation({
    mutationFn: (revisionId: string) => api.restoreCharacterRevision(id!, revisionId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['character', id] });
      queryClient.invalidateQueries({ queryKey: ['characters'] });
      queryClient.invalidateQueries({ queryKey: ['journal-entries', id] });
      queryClient.invalidateQueries({ queryKey: ['character-revisions', id] });
      toast('success', 'Restored');
      navigate(`/characters/${id}`);
    },
    onError: (e) => toast('error', errorMessage(e)),
  });

  if (!id) return null;

  return (
    <div className="page">
      <div className="page-header">
        <h2>History · {character.data?.name ?? '…'}</h2>
        <div className="page-header-actions">
          <button type="button" className="btn" onClick={() => navigate(`/characters/${id}`)}>
            ← Back
          </button>
        </div>
      </div>
      <p className="muted" style={{ fontSize: 12 }}>
        Snapshots are captured automatically before every save. Restore replaces the
        character&apos;s persona fields, notes, and all journal entries with the snapshot&apos;s
        contents (cover image and creation time are preserved).
      </p>

      {revisions.isLoading && <p className="muted">Loading…</p>}
      {revisions.isError && (
        <div className="error" role="alert" data-testid="history-error">
          Failed to load history: {errorMessage(revisions.error)}
        </div>
      )}
      {(revisions.data ?? []).length === 0 && !revisions.isLoading && !revisions.isError && (
        <div className="empty">
          No snapshots yet. Snapshots are created automatically when you save.
        </div>
      )}
      {(revisions.data ?? []).length > 0 && (
        <ul style={{ listStyle: 'none', padding: 0, marginTop: 12 }}>
          {(revisions.data ?? []).map((r) => (
            <RevisionRow
              key={r.id}
              summary={r}
              expanded={expanded.has(r.id)}
              onToggle={() =>
                setExpanded((prev) => {
                  const next = new Set(prev);
                  if (next.has(r.id)) next.delete(r.id);
                  else next.add(r.id);
                  return next;
                })
              }
              onRestore={() => setConfirmRestore(r.id)}
            />
          ))}
        </ul>
      )}

      <ConfirmDialog
        open={confirmRestore != null}
        title="Restore this snapshot?"
        body={`Current persona fields, notes, and all journal entries for "${character.data?.name ?? ''}" will be replaced with the snapshot's contents. This cannot be undone.`}
        confirmLabel="Restore"
        onConfirm={() => {
          if (confirmRestore) restore.mutate(confirmRestore);
          setConfirmRestore(null);
        }}
        onCancel={() => setConfirmRestore(null)}
      />
    </div>
  );
}

function RevisionRow({
  summary,
  expanded,
  onToggle,
  onRestore,
}: {
  summary: CharacterRevisionSummary;
  expanded: boolean;
  onToggle: () => void;
  onRestore: () => void;
}) {
  const detail = useQuery<CharacterRevision>({
    queryKey: ['character-revision', summary.id],
    queryFn: () => api.getCharacterRevision(summary.id),
    enabled: expanded,
  });

  return (
    <li
      style={{
        borderTop: '1px solid var(--border)',
        padding: '12px 0',
      }}
    >
      <div
        style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', gap: 8 }}
      >
        <div style={{ flex: 1 }}>
          <div>{new Date(summary.saved_at).toLocaleString()}</div>
          <div className="muted" style={{ fontSize: 12, marginTop: 4 }}>
            {summary.journal_entry_count} journal{' '}
            {summary.journal_entry_count === 1 ? 'entry' : 'entries'}
          </div>
        </div>
        <div style={{ display: 'flex', gap: 8 }}>
          <button type="button" className="btn btn-sm" onClick={onToggle}>
            {expanded ? 'Hide details' : 'Show details'}
          </button>
          <button
            type="button"
            className="btn btn-sm btn-danger"
            onClick={onRestore}
            title="Restore this snapshot"
          >
            Restore
          </button>
        </div>
      </div>
      {expanded && (
        <div style={{ marginTop: 12 }}>
          {detail.isLoading && <p className="muted">Loading details…</p>}
          {detail.isError && (
            <div className="error" role="alert">
              Failed to load snapshot: {errorMessage(detail.error)}
            </div>
          )}
          {detail.data && <RevisionDetail revision={detail.data} />}
        </div>
      )}
    </li>
  );
}

function RevisionDetail({ revision }: { revision: CharacterRevision }) {
  return (
    <div className="card" style={{ marginTop: 8 }}>
      <h3 style={{ marginTop: 0 }}>Character fields</h3>
      <dl style={{ display: 'grid', gridTemplateColumns: '160px 1fr', gap: '6px 12px', margin: 0 }}>
        {PERSONA_LABELS.map(({ key, label }) => {
          const value = revision.character_payload[key];
          return <FragmentRow key={key} label={label} value={value} />;
        })}
      </dl>
      <h3 style={{ marginTop: 16 }}>Journal entries ({revision.journal_entries.length})</h3>
      {revision.journal_entries.length === 0 ? (
        <p className="muted" style={{ fontSize: 12 }}>
          No journal entries in this snapshot.
        </p>
      ) : (
        <ul style={{ listStyle: 'none', padding: 0, marginTop: 8 }}>
          {revision.journal_entries.map((e) => (
            <li
              key={e.id}
              style={{
                borderTop: '1px solid var(--border)',
                padding: '8px 0',
              }}
            >
              <div style={{ whiteSpace: 'pre-wrap' }}>{e.entry}</div>
              {e.keyphrases.length > 0 && (
                <div style={{ display: 'flex', gap: 4, flexWrap: 'wrap', marginTop: 6 }}>
                  {e.keyphrases.map((k) => (
                    <span key={k} className="badge badge-muted">
                      {k}
                    </span>
                  ))}
                </div>
              )}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function FragmentRow({ label, value }: { label: string; value: string | null | undefined }) {
  const isLongField =
    label === 'Backstory' ||
    label === 'Key memories' ||
    label === 'Response directive' ||
    label === 'Example message' ||
    label === 'Additional context' ||
    label === 'Current scene' ||
    label === 'Greeting' ||
    label === 'Notes' ||
    label === 'Avatar description';
  const display = value == null || value === '' ? <span className="muted">(none)</span> : value;
  return (
    <>
      <dt className="muted" style={{ fontSize: 12 }}>
        {label}
      </dt>
      <dd
        style={{
          margin: 0,
          whiteSpace: isLongField ? 'pre-wrap' : 'normal',
          wordBreak: 'break-word',
        }}
      >
        {display}
      </dd>
    </>
  );
}
