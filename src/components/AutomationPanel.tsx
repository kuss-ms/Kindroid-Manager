import { useEffect, useMemo, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { api, errorMessage } from '../lib/api';
import {
  BOOTSTRAP_MODE_LABELS,
  SUMMARY_BACKEND_LABELS,
  SUMMARY_BACKEND_LIMIT,
  type AutoJournalEntry,
  type AutoJournalEntryStatus,
  type ChatAutomationDto,
  type SetChatAutomationSettingsInput,
  type SummaryBackend,
  type SummaryBootstrapMode,
} from '../lib/types';
import { chatAutomationSettingsSchema } from '../lib/schemas';
import { ConfirmDialog } from '../components/ConfirmDialog';
import { toast } from '../components/Toaster';

const DEFAULT_INTERVAL = 10;
const MIN_INTERVAL = 2;
const DEFAULT_JOURNAL_CAP = 1;
const DEFAULT_BACKEND: SummaryBackend = 'additional_context';
const DEFAULT_BOOTSTRAP: SummaryBootstrapMode = 'full_history';
const MAX_INSTRUCTIONS_CHARS = 4000;

interface AutomationPanelProps {
  aiId: string | null;
  automationInProgress: boolean;
}

interface PendingState {
  journalEnabled: boolean;
  summaryEnabled: boolean;
  interval: number;
  journalCap: number;
  summaryBackend: SummaryBackend;
  bootstrapMode: SummaryBootstrapMode;
  journalOverride: string;
  summaryOverride: string;
  hasJournalOverride: boolean;
  hasSummaryOverride: boolean;
  summary: string | null;
  pendingReformat: boolean;
  pendingSummaryCandidate: string | null;
}

function statusBadge(s: AutoJournalEntryStatus): { label: string; cls: string } {
  switch (s) {
    case 'sent':
      return { label: 'sent', cls: 'badge-success' };
    case 'pending':
      return { label: 'pending', cls: 'badge-warning' };
    case 'error':
      return { label: 'error', cls: 'badge-danger' };
  }
}

function isoOrEmpty(iso: string | null | undefined): string {
  if (!iso) return 'never';
  const then = new Date(iso).getTime();
  if (!Number.isFinite(then)) return iso;
  const diff = Math.max(0, Date.now() - then);
  const s = Math.floor(diff / 1000);
  if (s < 60) return `${s}s ago`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m ago`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ago`;
  const d = Math.floor(h / 24);
  return `${d}d ago`;
}

function charCount(s: string | null | undefined): number {
  if (!s) return 0;
  return Array.from(s).length;
}

export function AutomationPanel({
  aiId,
  automationInProgress: fallbackInProgress,
}: AutomationPanelProps) {
  const queryClient = useQueryClient();
  const enabled = !!aiId;

  const stateQuery = useQuery<ChatAutomationDto | null>({
    queryKey: ['chat-automation', aiId],
    queryFn: () => (aiId ? api.getChatAutomationState(aiId) : Promise.resolve(null)),
    enabled,
    refetchInterval: 5000,
  });

  const dto = stateQuery.data;

  const [pending, setPending] = useState<PendingState | null>(null);
  useEffect(() => {
    if (!dto) {
      setPending(null);
      return;
    }
    setPending({
      journalEnabled: dto.state.auto_journal_enabled,
      summaryEnabled: dto.state.auto_summary_enabled,
      interval: dto.state.interval || DEFAULT_INTERVAL,
      journalCap: dto.state.journal_cap || DEFAULT_JOURNAL_CAP,
      summaryBackend: dto.state.summary_backend ?? DEFAULT_BACKEND,
      bootstrapMode: dto.state.bootstrap_mode ?? DEFAULT_BOOTSTRAP,
      journalOverride: dto.state.journal_instructions_override ?? '',
      summaryOverride: dto.state.summary_instructions_override ?? '',
      hasJournalOverride: !!dto.state.journal_instructions_override,
      hasSummaryOverride: !!dto.state.summary_instructions_override,
      summary: dto.state.summary,
      pendingReformat: dto.state.pending_reformat,
      pendingSummaryCandidate: dto.state.pending_summary_candidate,
    });
  }, [dto]);

  const validationError = useMemo(() => {
    if (!pending) return null;
    if (pending.interval < MIN_INTERVAL) return `Interval must be at least ${MIN_INTERVAL}.`;
    if (pending.journalCap < 1 || pending.journalCap > 3) return 'Journal cap must be 1-3.';
    if (pending.journalOverride.length > MAX_INSTRUCTIONS_CHARS)
      return 'Journal override too long.';
    if (pending.summaryOverride.length > MAX_INSTRUCTIONS_CHARS)
      return 'Summary override too long.';
    if (pending.journalOverride.includes('{{') || pending.journalOverride.includes('}}'))
      return 'Journal override contains placeholder syntax.';
    if (pending.summaryOverride.includes('{{') || pending.summaryOverride.includes('}}'))
      return 'Summary override contains placeholder syntax.';
    return null;
  }, [pending]);

  const dirty = useMemo(() => {
    if (!dto || !pending) return false;
    const s = dto.state;
    if (pending.journalEnabled !== s.auto_journal_enabled) return true;
    if (pending.summaryEnabled !== s.auto_summary_enabled) return true;
    if (pending.interval !== s.interval) return true;
    if (pending.journalCap !== s.journal_cap) return true;
    if (pending.summaryBackend !== s.summary_backend) return true;
    if (pending.bootstrapMode !== s.bootstrap_mode) return true;
    if (pending.hasJournalOverride !== !!s.journal_instructions_override) return true;
    if (pending.hasSummaryOverride !== !!s.summary_instructions_override) return true;
    if (pending.journalOverride !== (s.journal_instructions_override ?? '')) return true;
    if (pending.summaryOverride !== (s.summary_instructions_override ?? '')) return true;
    return false;
  }, [dto, pending]);

  const [confirmBackendSwitch, setConfirmBackendSwitch] = useState<{
    next: SummaryBackend;
  } | null>(null);
  const [confirmEnableSummary, setConfirmEnableSummary] = useState(false);
  const [confirmResetSummary, setConfirmResetSummary] = useState(false);
  const [confirmClearJournalOverride, setConfirmClearJournalOverride] = useState(false);
  const [confirmClearSummaryOverride, setConfirmClearSummaryOverride] = useState(false);

  function update<K extends keyof PendingState>(key: K, value: PendingState[K]) {
    setPending((p) => (p ? { ...p, [key]: value } : p));
  }

  const saveMutation = useMutation<ChatAutomationDto, unknown, SetChatAutomationSettingsInput>({
    mutationFn: (input) => api.setChatAutomationSettings(input),
    onSuccess: (data) => {
      queryClient.setQueryData(['chat-automation', aiId], data);
      toast('success', 'Automation settings saved');
    },
    onError: (e) => toast('error', errorMessage(e)),
  });

  function applyPending() {
    if (!aiId || !pending) return;
    const candidate: SetChatAutomationSettingsInput = {
      ai_id: aiId,
      auto_journal_enabled: pending.journalEnabled,
      auto_summary_enabled: pending.summaryEnabled,
      interval: pending.interval,
      journal_cap: pending.journalCap,
      summary_backend: pending.summaryBackend,
      bootstrap_mode: pending.bootstrapMode,
      journal_instructions_override: pending.hasJournalOverride
        ? pending.journalOverride.trim() === ''
          ? null
          : pending.journalOverride
        : null,
      summary_instructions_override: pending.hasSummaryOverride
        ? pending.summaryOverride.trim() === ''
          ? null
          : pending.summaryOverride
        : null,
    };
    const parsed = chatAutomationSettingsSchema.safeParse(candidate);
    if (!parsed.success) {
      toast('error', parsed.error.issues[0]?.message ?? 'Invalid settings');
      return;
    }
    saveMutation.mutate(candidate);
  }

  function onSaveClick() {
    if (!aiId || !pending || !dto) return;
    if (validationError) {
      toast('error', validationError);
      return;
    }
    const backendChanged = pending.summaryBackend !== dto.state.summary_backend;
    const enablingSummary = !dto.state.auto_summary_enabled && pending.summaryEnabled;
    if (backendChanged && dto.state.summary) {
      setConfirmBackendSwitch({ next: pending.summaryBackend });
      return;
    }
    if (enablingSummary && pending.bootstrapMode === 'full_history') {
      setConfirmEnableSummary(true);
      return;
    }
    applyPending();
  }

  const resetMutation = useMutation({
    mutationFn: () => api.resetChatSummary({ ai_id: aiId! }),
    onSuccess: (data: ChatAutomationDto) => {
      queryClient.setQueryData(['chat-automation', aiId], data);
      toast('success', 'Summary reset');
    },
    onError: (e) => toast('error', errorMessage(e)),
  });

  const runNowMutation = useMutation({
    mutationFn: () => api.runSummaryNow({ ai_id: aiId! }),
    onSuccess: (r) => {
      toast(r.ran ? 'success' : 'info', r.message);
      queryClient.invalidateQueries({ queryKey: ['chat-automation', aiId] });
    },
    onError: (e) => toast('error', errorMessage(e)),
  });

  const defaultsQuery = useQuery({
    queryKey: ['automation-instruction-defaults'],
    queryFn: api.getAutomationInstructionsDefaults,
    staleTime: Infinity,
  });

  if (!aiId) return null;
  if (stateQuery.isLoading || !pending || !dto) {
    return (
      <div className="card" style={{ marginTop: 12 }}>
        <h3>Automation</h3>
        <p className="muted">Loading automation state…</p>
      </div>
    );
  }
  if (stateQuery.error) {
    return (
      <div className="card" style={{ marginTop: 12 }}>
        <h3>Automation</h3>
        <p className="form-error">Failed to load: {errorMessage(stateQuery.error)}</p>
      </div>
    );
  }

  const safeDto: ChatAutomationDto = dto;
  const busy = safeDto.automation_in_progress || fallbackInProgress;
  const backendLimit = SUMMARY_BACKEND_LIMIT[pending.summaryBackend];
  const summaryChars = charCount(pending.summary);
  const candidateChars = charCount(pending.pendingSummaryCandidate);

  return (
    <div className="card" style={{ marginTop: 12 }}>
      <div
        style={{
          display: 'flex',
          alignItems: 'baseline',
          gap: 8,
          flexWrap: 'wrap',
        }}
      >
        <h3 style={{ marginBottom: 0, flex: 1 }}>Automation</h3>
        {busy ? (
          <span className="badge badge-warning">Automation in progress…</span>
        ) : (
          <span className="badge badge-muted">Automation idle</span>
        )}
      </div>
      <p className="muted" style={{ fontSize: 12, marginTop: 6 }}>
        Runs after each completed chat sync. Newest 10 local messages are excluded. Auto-journal
        entries are sent directly to Kindroid and cannot be edited or deleted. Auto-summary
        overwrites the selected persona field on the AI.
      </p>

      <div className="section" style={{ marginTop: 12 }}>
        <label className="checkbox">
          <input
            type="checkbox"
            checked={pending.journalEnabled}
            disabled={!enabled}
            onChange={(e) => update('journalEnabled', e.target.checked)}
          />
          Auto journal
        </label>
        <label className="checkbox">
          <input
            type="checkbox"
            checked={pending.summaryEnabled}
            disabled={!enabled}
            onChange={(e) => update('summaryEnabled', e.target.checked)}
          />
          Auto summarize
        </label>
      </div>

      <div className="section-grid" style={{ marginTop: 12 }}>
        <div className="form-row">
          <label className="form-label" htmlFor="automation-interval">
            Message interval
          </label>
          <input
            id="automation-interval"
            className="input"
            type="number"
            min={MIN_INTERVAL}
            step={1}
            value={pending.interval}
            onChange={(e) =>
              update('interval', Math.max(MIN_INTERVAL, Number(e.target.value) || MIN_INTERVAL))
            }
            disabled={!enabled}
          />
          <span className="form-hint">
            Minimum {MIN_INTERVAL}. The next run waits for this many new stable messages.
          </span>
        </div>
        <div className="form-row">
          <label className="form-label" htmlFor="automation-journal-cap">
            Max journal entries per run
          </label>
          <input
            id="automation-journal-cap"
            className="input"
            type="number"
            min={1}
            max={3}
            step={1}
            value={pending.journalCap}
            onChange={(e) =>
              update('journalCap', Math.min(3, Math.max(1, Number(e.target.value) || 1)))
            }
            disabled={!enabled}
          />
          <span className="form-hint">1-3 entries per interval.</span>
        </div>
      </div>

      <div className="section-grid" style={{ marginTop: 12 }}>
        <div className="form-row">
          <label className="form-label" htmlFor="automation-backend">
            Summary backend
          </label>
          <select
            id="automation-backend"
            className="select"
            value={pending.summaryBackend}
            onChange={(e) => update('summaryBackend', e.target.value as SummaryBackend)}
            disabled={!enabled}
          >
            {(Object.keys(SUMMARY_BACKEND_LABELS) as SummaryBackend[]).map((b) => (
              <option key={b} value={b}>
                {SUMMARY_BACKEND_LABELS[b]} ({SUMMARY_BACKEND_LIMIT[b]} chars)
              </option>
            ))}
          </select>
          <span className="form-hint">Where the AI summary is stored on the AI persona.</span>
        </div>
        {pending.summaryEnabled && (
          <div className="form-row">
            <label className="form-label">Summary bootstrap</label>
            <label className="checkbox">
              <input
                type="radio"
                name="automation-bootstrap"
                checked={pending.bootstrapMode === 'full_history'}
                onChange={() => update('bootstrapMode', 'full_history')}
              />
              {BOOTSTRAP_MODE_LABELS.full_history}
            </label>
            <label className="checkbox">
              <input
                type="radio"
                name="automation-bootstrap"
                checked={pending.bootstrapMode === 'incremental_only'}
                onChange={() => update('bootstrapMode', 'incremental_only')}
              />
              {BOOTSTRAP_MODE_LABELS.incremental_only}
            </label>
            <span className="form-hint">Only the first option summarises existing history.</span>
          </div>
        )}
      </div>

      <div className="section" style={{ marginTop: 16 }}>
        <h4 style={{ marginBottom: 4 }}>Custom instructions (this target)</h4>
        <p className="muted" style={{ fontSize: 12 }}>
          Overrides the global default. Leave empty and toggle off to use the global.
        </p>
        <div className="form-row">
          <label className="form-label">Journal instructions override</label>
          <textarea
            className="textarea"
            rows={3}
            value={pending.journalOverride}
            disabled={!enabled || !pending.hasJournalOverride}
            onChange={(e) => update('journalOverride', e.target.value)}
            placeholder={
              pending.hasJournalOverride
                ? 'Custom journal instructions for this target'
                : 'Toggle "Use override" to enter custom instructions'
            }
            maxLength={MAX_INSTRUCTIONS_CHARS}
          />
          <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
            <label className="checkbox">
              <input
                type="checkbox"
                checked={pending.hasJournalOverride}
                disabled={!enabled}
                onChange={(e) => update('hasJournalOverride', e.target.checked)}
              />
              Use override
            </label>
            <button
              type="button"
              className="btn btn-sm"
              disabled={!pending.hasJournalOverride || !pending.journalOverride}
              onClick={() => update('journalOverride', defaultsQuery.data?.journal ?? '')}
            >
              Restore default
            </button>
            <button
              type="button"
              className="btn btn-sm btn-danger"
              disabled={!pending.hasJournalOverride}
              onClick={() => setConfirmClearJournalOverride(true)}
            >
              Clear override
            </button>
          </div>
        </div>
        <div className="form-row">
          <label className="form-label">Summary instructions override</label>
          <textarea
            className="textarea"
            rows={3}
            value={pending.summaryOverride}
            disabled={!enabled || !pending.hasSummaryOverride}
            onChange={(e) => update('summaryOverride', e.target.value)}
            placeholder={
              pending.hasSummaryOverride
                ? 'Custom summary instructions for this target'
                : 'Toggle "Use override" to enter custom instructions'
            }
            maxLength={MAX_INSTRUCTIONS_CHARS}
          />
          <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
            <label className="checkbox">
              <input
                type="checkbox"
                checked={pending.hasSummaryOverride}
                disabled={!enabled}
                onChange={(e) => update('hasSummaryOverride', e.target.checked)}
              />
              Use override
            </label>
            <button
              type="button"
              className="btn btn-sm"
              disabled={!pending.hasSummaryOverride || !pending.summaryOverride}
              onClick={() => update('summaryOverride', defaultsQuery.data?.summary ?? '')}
            >
              Restore default
            </button>
            <button
              type="button"
              className="btn btn-sm btn-danger"
              disabled={!pending.hasSummaryOverride}
              onClick={() => setConfirmClearSummaryOverride(true)}
            >
              Clear override
            </button>
          </div>
        </div>
      </div>

      {validationError && (
        <p className="form-error" style={{ marginTop: 8 }}>
          {validationError}
        </p>
      )}

      <div className="flex-row" style={{ marginTop: 12, flexWrap: 'wrap' }}>
        <button
          type="button"
          className="btn btn-primary"
          onClick={onSaveClick}
          disabled={!dirty || !!validationError || saveMutation.isPending}
        >
          {saveMutation.isPending ? 'Saving…' : 'Save settings'}
        </button>
        <button
          type="button"
          className="btn"
          onClick={() => runNowMutation.mutate()}
          disabled={busy || runNowMutation.isPending}
          title={busy ? 'Automation is running' : ''}
        >
          {runNowMutation.isPending ? 'Running…' : 'Run summary now'}
        </button>
        <button
          type="button"
          className="btn btn-danger"
          onClick={() => setConfirmResetSummary(true)}
          disabled={busy || !pending.summary}
        >
          Reset summary
        </button>
      </div>

      <hr style={{ margin: '16px 0', border: 0, borderTop: '1px solid var(--border)' }} />

      <div className="section-grid">
        <div className="section">
          <h4 style={{ marginBottom: 4 }}>Auto-journal status</h4>
          <p className="muted" style={{ fontSize: 12 }}>
            Last run: {isoOrEmpty(safeDto.state.journal_last_run_at)} · Last error:{' '}
            {safeDto.state.journal_last_error ?? '—'}
          </p>
          <p className="muted" style={{ fontSize: 12 }}>
            Initialised:{' '}
            {safeDto.state.journal_initialised
              ? 'yes'
              : 'no (first sync will seed watermark, no backfill)'}
          </p>
        </div>
        <div className="section">
          <h4 style={{ marginBottom: 4 }}>Auto-summary status</h4>
          <p className="muted" style={{ fontSize: 12 }}>
            Last run: {isoOrEmpty(safeDto.state.summary_last_run_at)} · Last error:{' '}
            {safeDto.state.summary_last_error ?? '—'}
          </p>
          {pending.pendingReformat && (
            <p className="form-error" style={{ fontSize: 12 }}>
              Pending reformat: local summary exceeds the {backendLimit}-char limit for{' '}
              {SUMMARY_BACKEND_LABELS[pending.summaryBackend]}. Will not send until it fits.
            </p>
          )}
          {pending.pendingSummaryCandidate && !pending.pendingReformat && (
            <p className="muted" style={{ fontSize: 12 }}>
              Pending candidate ({candidateChars} chars) will be retried on the next drain.
            </p>
          )}
        </div>
      </div>

      <div className="section" style={{ marginTop: 12 }}>
        <h4 style={{ marginBottom: 4 }}>Local summary (read-only)</h4>
        {pending.summary ? (
          <div
            className="card-tight"
            style={{
              background: 'var(--surface-2)',
              whiteSpace: 'pre-wrap',
              fontSize: 13,
            }}
          >
            {pending.summary}
          </div>
        ) : (
          <p className="muted">No summary yet.</p>
        )}
        <span className="form-hint">
          {pending.summary ? `${summaryChars} / ${backendLimit} characters` : ''}
        </span>
      </div>

      <div className="section" style={{ marginTop: 12 }}>
        <h4 style={{ marginBottom: 4 }}>Recent auto-journal entries</h4>
        {safeDto.recent_journal_entries.length === 0 ? (
          <p className="muted">No entries yet.</p>
        ) : (
          <ul style={{ listStyle: 'none', padding: 0, margin: 0 }}>
            {dto.recent_journal_entries.map((e: AutoJournalEntry) => {
              const b = statusBadge(e.status);
              return (
                <li
                  key={e.id}
                  className="card-tight"
                  style={{ marginBottom: 8, background: 'var(--surface-2)' }}
                >
                  <div
                    style={{
                      display: 'flex',
                      gap: 8,
                      alignItems: 'baseline',
                      flexWrap: 'wrap',
                    }}
                  >
                    <span className={`badge ${b.cls}`}>{b.label}</span>
                    <span className="muted" style={{ fontSize: 12 }}>
                      {isoOrEmpty(e.created_at)}
                    </span>
                    {e.response_status != null && (
                      <span className="muted" style={{ fontSize: 12 }}>
                        HTTP {e.response_status}
                      </span>
                    )}
                  </div>
                  <div style={{ marginTop: 4, whiteSpace: 'pre-wrap' }}>{e.entry}</div>
                  {e.keyphrases.length > 0 && (
                    <div className="muted" style={{ fontSize: 12, marginTop: 4 }}>
                      Keyphrases: {e.keyphrases.join(', ')}
                    </div>
                  )}
                  {e.response_message && e.status === 'error' && (
                    <div className="form-error" style={{ fontSize: 12, marginTop: 4 }}>
                      {e.response_message}
                    </div>
                  )}
                </li>
              );
            })}
          </ul>
        )}
      </div>

      <ConfirmDialog
        open={!!confirmBackendSwitch}
        title="Switch summary backend?"
        body={`Switching to ${confirmBackendSwitch ? SUMMARY_BACKEND_LABELS[confirmBackendSwitch.next] : ''} will overwrite the corresponding persona field on the AI on the next successful run. If the existing summary does not fit the new limit, the next run will reformat it locally before sending.`}
        confirmLabel="Switch backend"
        onConfirm={() => {
          setConfirmBackendSwitch(null);
          applyPending();
        }}
        onCancel={() => setConfirmBackendSwitch(null)}
      />
      <ConfirmDialog
        open={confirmEnableSummary}
        title="Bootstrap from existing history?"
        body="Auto-summarize is enabled with bootstrap mode 'full history'. The next completed sync will summarise the entire stable chat history. Use 'Incremental only' to skip the bootstrap."
        confirmLabel="Enable auto-summarize"
        onConfirm={() => {
          setConfirmEnableSummary(false);
          applyPending();
        }}
        onCancel={() => setConfirmEnableSummary(false)}
      />
      <ConfirmDialog
        open={confirmResetSummary}
        title="Reset local summary?"
        body="Clears the local summary, pending candidate, and summary watermark. The Kindroid field is not modified. Auto-journal state and audit are preserved."
        confirmLabel="Reset summary"
        onConfirm={() => {
          setConfirmResetSummary(false);
          resetMutation.mutate();
        }}
        onCancel={() => setConfirmResetSummary(false)}
      />
      <ConfirmDialog
        open={confirmClearJournalOverride}
        title="Clear journal override?"
        body="Removes the per-target journal instructions override. The next run will use the global default."
        confirmLabel="Clear override"
        onConfirm={() => {
          setConfirmClearJournalOverride(false);
          update('hasJournalOverride', false);
          update('journalOverride', '');
        }}
        onCancel={() => setConfirmClearJournalOverride(false)}
      />
      <ConfirmDialog
        open={confirmClearSummaryOverride}
        title="Clear summary override?"
        body="Removes the per-target summary instructions override. The next run will use the global default."
        confirmLabel="Clear override"
        onConfirm={() => {
          setConfirmClearSummaryOverride(false);
          update('hasSummaryOverride', false);
          update('summaryOverride', '');
        }}
        onCancel={() => setConfirmClearSummaryOverride(false)}
      />
    </div>
  );
}
