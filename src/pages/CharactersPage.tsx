import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { api, errorMessage } from '../lib/api';
import type { Character, CreateNewKinResult } from '../lib/types';
import { ConfirmDialog } from '../components/ConfirmDialog';
import { RowActions, type RowAction } from '../components/RowActions';
import { toast } from '../components/Toaster';
export function CharactersPage() {
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const [search, setSearch] = useState('');
  const [deleteId, setDeleteId] = useState<string | null>(null);
  const [createId, setCreateId] = useState<string | null>(null);
  const characters = useQuery<Character[]>({
    queryKey: ['characters'],
    queryFn: api.listCharacters,
  });
  const del = useMutation<void, unknown, string>({
    mutationFn: (id) => api.deleteCharacter(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['characters'] });
      toast('success', 'Character deleted');
    },
    onError: (e) => toast('error', errorMessage(e)),
  });
  const duplicate = useMutation<Character, unknown, string>({
    mutationFn: (id) => api.duplicateCharacter(id),
    onSuccess: (c) => {
      queryClient.invalidateQueries({ queryKey: ['characters'] });
      toast('success', `Duplicated as "${c.name}"`);
    },
    onError: (e) => toast('error', errorMessage(e)),
  });

  const createNew = useMutation<CreateNewKinResult, unknown, string>({
    mutationFn: (id) => api.pushCreateNewKin(id),
    onSuccess: (result) => {
      if (!result.create_new_ai.ok) {
        toast('error', result.create_new_ai.message);
        return;
      }
      queryClient.invalidateQueries({ queryKey: ['targets'] });
      queryClient.invalidateQueries({ queryKey: ['push-history'] });
      queryClient.invalidateQueries({ queryKey: ['characters'] });
      toast('success', `New Kin created with ai_id ${result.target.ai_id}`);
      navigate(`/history/${result.log_id}`);
    },
    onError: (e) => toast('error', errorMessage(e)),
  });

  const share = useMutation<void, unknown, string>({
    mutationFn: async (id) => {
      const bytes = await api.exportShareImage(id);
      const blob = new Blob([new Uint8Array(bytes)], { type: 'image/png' });
      if (
        typeof ClipboardItem !== 'undefined' &&
        typeof navigator !== 'undefined' &&
        navigator.clipboard?.write
      ) {
        await navigator.clipboard.write([new ClipboardItem({ 'image/png': blob })]);
        return;
      }
      throw new Error('Image clipboard write not supported in this environment');
    },
    onSuccess: () => toast('success', 'Share image copied to clipboard'),
    onError: (e) => toast('error', errorMessage(e)),
  });
  const list = (characters.data ?? []).filter((c: Character) =>
    c.name.toLowerCase().includes(search.toLowerCase()),
  );
  return (
    <div className="page">
      {' '}
      <div className="page-header">
        {' '}
        <h2>Characters</h2>{' '}
        <div className="page-header-actions">
          {' '}
          <input
            className="input input-search"
            placeholder="Search by name…"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === '/') {
                e.preventDefault();
                (e.target as HTMLInputElement).focus();
              }
            }}
          />{' '}
          <button className="btn btn-primary" onClick={() => navigate('/characters/new')}>
            {' '}
            New{' '}
          </button>{' '}
        </div>{' '}
      </div>{' '}
      <p className="muted" style={{ fontSize: 12, marginBottom: 0 }}>
        {' '}
        Drop a PNG anywhere on the window (or paste from clipboard) to import a share image.{' '}
      </p>{' '}
      {list.length === 0 && (
        <div className="empty">No characters yet. Create one or drop a share image.</div>
      )}{' '}
      {list.length > 0 && (
        <div className="card" style={{ padding: 0 }}>
          {' '}
          <div className="list">
            {' '}
            {list.map((c) => (
              <div key={c.id} className="list-item">
                {' '}
                <div className="list-item-main">
                  {' '}
                  <div className="list-item-title">{c.name}</div>{' '}
                  <div className="list-item-sub mono">
                    {' '}
                    {c.id.slice(0, 8)} {c.ai_name && <> · AI: {c.ai_name}</>}{' '}
                    {c.cover_image && <> · 🖼</>}{' '}
                  </div>{' '}
                </div>{' '}
                <div className="list-item-actions">
                  {' '}
                  <RowActions
                    actions={
                      [
                        {
                          label: 'Edit',
                          onClick: () => navigate(`/characters/${c.id}`),
                        },
                        {
                          label: 'Share',
                          onClick: () => share.mutate(c.id),
                          disabled: share.isPending || !c.cover_image,
                          title: c.cover_image
                            ? 'Copy share image (with persona) to clipboard'
                            : 'Upload a cover image first',
                        },
                        {
                          label: 'Duplicate',
                          onClick: () => duplicate.mutate(c.id),
                        },
                        {
                          label:
                            createNew.isPending && createNew.variables === c.id
                              ? 'Pushing…'
                              : 'Push as new Kin',
                          onClick: () => {
                            if (!c.ai_name?.trim()) {
                              toast('error', 'ai_name is required to create a new Kin');
                              return;
                            }
                            setCreateId(c.id);
                          },
                          disabled: createNew.isPending && createNew.variables === c.id,
                          title: c.ai_name?.trim()
                            ? 'Create a new Kin from this character'
                            : 'Set an AI name before creating a new Kin',
                        },
                        {
                          label: 'Delete',
                          danger: true,
                          onClick: () => setDeleteId(c.id),
                        },
                      ] satisfies RowAction[]
                    }
                  />{' '}
                </div>{' '}
              </div>
            ))}{' '}
          </div>{' '}
        </div>
      )}{' '}
      <ConfirmDialog
        open={!!createId}
        title="Push as new Kin?"
        body={`Create a new Kin on Kindroid from "${list.find((c) => c.id === createId)?.name ?? ''}"? This will push all fields and journal entries, then add the new AI as a local target.`}
        confirmLabel="Create"
        onConfirm={() => {
          if (createId) createNew.mutate(createId);
          setCreateId(null);
        }}
        onCancel={() => setCreateId(null)}
      />{' '}
      <ConfirmDialog
        open={!!deleteId}
        title="Delete character?"
        body="This permanently removes the character and its cover image, and cannot be undone."
        confirmLabel="Delete"
        onConfirm={() => {
          if (deleteId) del.mutate(deleteId);
          setDeleteId(null);
        }}
        onCancel={() => setDeleteId(null)}
      />{' '}
    </div>
  );
}
