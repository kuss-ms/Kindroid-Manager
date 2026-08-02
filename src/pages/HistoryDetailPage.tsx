import { useQuery } from '@tanstack/react-query';
import { useNavigate, useParams } from 'react-router-dom';
import { api } from '../lib/api';
export function HistoryDetailPage() {
  const { id } = useParams();
  const navigate = useNavigate();
  const entry = useQuery({
    queryKey: ['push-log', id],
    queryFn: () => (id ? api.getPushLog(id) : Promise.resolve(null)),
    enabled: !!id,
  });
  if (!entry.data) return <p className="muted">Loading…</p>;
  const e = entry.data;
  return (
    <div className="page">
      {' '}
      <button
        className="btn btn-sm"
        style={{ alignSelf: 'flex-start' }}
        onClick={() => navigate('/history')}
      >
        {' '}
        ← Back{' '}
      </button>{' '}
      <h2>Push detail</h2>{' '}
      <div className="card">
        {' '}
        <div className="detail-row">
          {' '}
          <span className="detail-label">When</span>{' '}
          <span className="detail-value">{new Date(e.at).toLocaleString()}</span>{' '}
        </div>{' '}
        <div className="detail-row">
          {' '}
          <span className="detail-label">Character</span>{' '}
          <span className="detail-value">{e.character_name}</span>{' '}
        </div>{' '}
        <div className="detail-row">
          {' '}
          <span className="detail-label">Target AI ID</span>{' '}
          <span className="detail-value mono">{e.target_ai_id}</span>{' '}
        </div>{' '}
        <div className="detail-row">
          {' '}
          <span className="detail-label">Fields sent</span>{' '}
          <span className="detail-value">{e.fields_sent.join(', ') || '—'}</span>{' '}
        </div>{' '}
        <div className="detail-row">
          {' '}
          <span className="detail-label">Chat break</span>{' '}
          <span className="detail-value">
            {' '}
            {e.did_chat_break ? 'yes' : 'no'}{' '}
            {e.did_chat_break && (
              <div className="muted" style={{ marginTop: 6, fontSize: 12 }}>
                {' '}
                Greeting sent: {e.greeting ?? ''} <br /> wipe_cascaded:{' '}
                {String(e.wipe_cascaded)}{' '}
              </div>
            )}{' '}
          </span>{' '}
        </div>{' '}
        {e.journal_entry_ids && e.journal_entry_ids.length > 0 && (
          <div className="detail-row">
            <span className="detail-label">Journal entries</span>
            <span className="detail-value">
              {e.journal_entry_ids.length} (
              {e.journal_entry_ids.map((id: string) => id.slice(0, 8)).join(', ')})
            </span>
          </div>
        )}{' '}
      </div>{' '}
      {e.create_new_ai_status !== undefined && (
        <div className="card">
          {' '}
          <h3>create-new-ai response</h3>{' '}
          <p className="muted" style={{ fontSize: 12 }}>
            Status: {e.create_new_ai_status}
          </p>{' '}
          <pre>{e.create_new_ai_body ?? ''}</pre>{' '}
        </div>
      )}{' '}
      <div className="card">
        {' '}
        <h3>update-info response</h3>{' '}
        <p className="muted" style={{ fontSize: 12 }}>
          Status: {e.update_info_status}
        </p>{' '}
        <pre>{e.update_info_body}</pre>{' '}
      </div>{' '}
      {e.chat_break_status !== null && e.chat_break_status !== undefined && (
        <div className="card">
          {' '}
          <h3>chat-break response</h3>{' '}
          <p className="muted" style={{ fontSize: 12 }}>
            Status: {e.chat_break_status}
          </p>{' '}
          <pre>{e.chat_break_body ?? ''}</pre>{' '}
        </div>
      )}{' '}
    </div>
  );
}
