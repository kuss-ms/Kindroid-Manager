import { useQuery } from '@tanstack/react-query';
import { useNavigate } from 'react-router-dom';
import { useState } from 'react';
import { api, errorMessage } from '../lib/api';
import type { PushLogEntry } from '../lib/types';
export function HistoryPage() {
  const [limit] = useState(50);
  const [offset, setOffset] = useState(0);
  const navigate = useNavigate();
  const history = useQuery<PushLogEntry[]>({
    queryKey: ['push-history', limit, offset],
    queryFn: () => api.listPushHistory(limit, offset),
  });
  return (
    <div className="page">
      <div className="page-header">
        <h2>Push history</h2>
      </div>
      {history.isLoading && <p className="muted">Loading…</p>}
      {history.isError && (
        <div className="error" role="alert" data-testid="history-error">
          Failed to load push history: {errorMessage(history.error)}
        </div>
      )}
      {(history.data ?? []).length === 0 && !history.isLoading && !history.isError && (
        <div className="empty">No pushes yet.</div>
      )}
      {(history.data ?? []).length > 0 && (
        <table className="table">
          <thead>
            <tr>
              <th>When</th>
              <th>Character</th>
              <th>Target</th>
              <th>Fields</th>
              <th>Chat break</th>
              <th title="create-new-ai for new Kin pushes; update-info for existing Kin pushes">
                Status
              </th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {(history.data ?? []).map((row: PushLogEntry) => (
              <tr key={row.id} onClick={() => navigate(`/history/${row.id}`)}>
                <td>{new Date(row.at).toLocaleString()}</td>
                <td>{row.character_name}</td>
                <td className="mono">{row.target_ai_id}</td>
                <td>{row.fields_sent.join(', ') || '—'}</td>
                <td>
                  {row.did_chat_break ? (
                    <span className="badge badge-success">yes</span>
                  ) : (
                    <span className="badge badge-muted">no</span>
                  )}
                </td>
                <td>
                  {row.create_new_ai_status !== undefined ? (
                    <span
                      className={`badge ${row.create_new_ai_status < 300 ? 'badge-success' : 'badge-danger'}`}
                      title="create-new-ai response status"
                    >
                      create-new-ai {row.create_new_ai_status}
                    </span>
                  ) : (
                    <span
                      className={`badge ${row.update_info_status < 300 ? 'badge-success' : 'badge-danger'}`}
                      title="update-info response status"
                    >
                      update-info {row.update_info_status}
                    </span>
                  )}
                </td>
                <td>
                  <button
                    className="btn btn-sm"
                    onClick={(e) => {
                      e.stopPropagation();
                      const fields = row.fields_sent.join(',');
                      const cb = row.did_chat_break ? '1' : '0';
                      const greet = row.greeting ?? '';
                      const wipe = row.wipe_cascaded ? '1' : '0';
                      const journalIds =
                        row.journal_entry_ids && row.journal_entry_ids.length > 0
                          ? `&journal_entry_ids=${encodeURIComponent(row.journal_entry_ids.join(','))}`
                          : '';
                      navigate(
                        `/push?characterId=${row.character_id}&targetId=${row.target_id}&fields=${fields}&chatBreak=${cb}&greeting=${encodeURIComponent(greet)}&wipe=${wipe}${journalIds}`,
                      );
                    }}
                  >
                    Re-push
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      <div className="flex-row">
        <button
          className="btn"
          disabled={offset === 0}
          onClick={() => setOffset(Math.max(0, offset - limit))}
        >
          ← Newer
        </button>
        <button
          className="btn"
          disabled={(history.data?.length ?? 0) < limit}
          onClick={() => setOffset(offset + limit)}
        >
          Older →
        </button>
      </div>
    </div>
  );
}
