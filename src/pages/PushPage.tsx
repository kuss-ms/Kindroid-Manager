import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useEffect, useMemo, useState } from 'react';
import { useNavigate, useSearchParams } from 'react-router-dom';
import { api, errorMessage } from '../lib/api';
import { ConfirmDialog } from '../components/ConfirmDialog';
import { defaultSelected, FieldChecklist } from '../components/FieldChecklist';
import { toast } from '../components/Toaster';
import type { Character, JournalEntry, PushResult, StepResult } from '../lib/types';
export function PushPage() {
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const [params] = useSearchParams();
  const [characterId, setCharacterId] = useState<string>(params.get('characterId') ?? '');
  const [targetId, setTargetId] = useState<string>(params.get('targetId') ?? '');
  const [chatBreak, setChatBreak] = useState(params.get('chatBreak') === '1');
  const [greeting, setGreeting] = useState(params.get('greeting') ?? '');
  const [wipeCascaded, setWipeCascaded] = useState(params.get('wipe') === '1');
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [selectedJournal, setSelectedJournal] = useState<Set<string>>(new Set());
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [result, setResult] = useState<PushResult | null>(null);
  const characters = useQuery<Character[]>({
    queryKey: ['characters'],
    queryFn: api.listCharacters,
  });
  const targets = useQuery<Awaited<ReturnType<typeof api.listTargets>>>({
    queryKey: ['targets'],
    queryFn: api.listTargets,
  });
  const character = useQuery<Character | null>({
    queryKey: ['character', characterId],
    queryFn: () => (characterId ? api.getCharacter(characterId) : Promise.resolve(null)),
    enabled: !!characterId,
  });
  const target = useQuery<Awaited<ReturnType<typeof api.getTarget>> | null>({
    queryKey: ['target', targetId],
    queryFn: () => (targetId ? api.getTarget(targetId) : Promise.resolve(null)),
    enabled: !!targetId,
  });
  const journalEntries = useQuery<JournalEntry[]>({
    queryKey: ['journal-entries', characterId],
    queryFn: () => (characterId ? api.listJournalEntries(characterId) : Promise.resolve([])),
    enabled: !!characterId,
  });
  const fieldsParam = params.get('fields');
  const journalParams = params.get('journalEntryIds');
  useEffect(() => {
    if (!character.data) return;
    if (fieldsParam) {
      setSelected(new Set(fieldsParam.split(',').filter(Boolean)));
    } else {
      setSelected(defaultSelected(character.data));
    }
  }, [character.data, fieldsParam, params]);

  useEffect(() => {
    if (!journalParams) {
      setSelectedJournal(new Set());
      return;
    }
    setSelectedJournal(new Set(journalParams.split(',').filter(Boolean)));
  }, [journalParams, params]);

  // Pre-fill the greeting from the character whenever the character
  // loads or changes. Without this, the chat-break textarea stays
  // empty and the push button stays disabled until the user types.
  useEffect(() => {
    setGreeting(character.data?.greeting ?? '');
  }, [character.data?.greeting]);
  const push = useMutation<PushResult, unknown, void>({
    mutationFn: () => {
      if (!character.data || !target.data) throw new Error('pick character & target');
      const journalIds = Array.from(selectedJournal);
      const req = {
        character_id: character.data.id,
        target_id: target.data.id,
        fields: Array.from(selected),
        chat_break: chatBreak ? { greeting, wipe_cascaded: wipeCascaded } : null,
        journalEntryIds: journalIds.length ? journalIds : null,
      };
      return api.pushToTarget(req);
    },
    onSuccess: (res: PushResult) => {
      setResult(res);
      queryClient.invalidateQueries({ queryKey: ['push-history'] });
      const journalCount = res.journal_entries?.length ?? 0;
      const okCount = res.journal_entries?.filter((j) => j.ok).length ?? 0;
      const tail = journalCount > 0 ? `, ${okCount}/${journalCount} journal entries sent` : '';
      toast('success', `update-info ${res.update_info.ok ? 'OK' : 'failed'}${tail}`);
    },
    onError: (e) => toast('error', errorMessage(e)),
  });
  const previewBody = useMemo(() => {
    if (!character.data || !target.data) return null;
    const body: Record<string, string> = { ai_id: target.data.ai_id };
    for (const f of selected) {
      const v = character.data[f as keyof Character] as string | null | undefined;
      if (v) body[f] = v;
    }
    return body;
  }, [character.data, target.data, selected]);
  const previewChatBreak = useMemo(() => {
    if (!target.data || !chatBreak) return null;
    return { ai_id: target.data.ai_id, greeting, wipe_cascaded: wipeCascaded };
  }, [target.data, chatBreak, greeting, wipeCascaded]);
  const journalSelected = selectedJournal.size;
  const journalList = useMemo(() => journalEntries.data ?? [], [journalEntries.data]);
  const previewJournal = useMemo(() => {
    if (!target.data || journalList.length === 0) return null;
    const set = selectedJournal;
    return journalList
      .filter((e) => set.has(e.id))
      .map((e) => ({
        ai_id: target.data!.ai_id,
        entry: e.entry,
        keyphrases: e.keyphrases,
      }));
  }, [journalList, selectedJournal, target.data]);
  const canPush =
    !!character.data &&
    !!target.data &&
    (selected.size > 0 || journalSelected > 0 || chatBreak) &&
    (!chatBreak || greeting.trim().length > 0) &&
    !push.isPending;
  const fieldCount = selected.size;
  const journalLabel = journalSelected
    ? ` + ${journalSelected} journal${journalSelected === 1 ? '' : 's'}`
    : '';
  const targetLabel = target.data ? `${target.data.label} — ${target.data.ai_id}` : '';
  const avatarDescription = character.data?.ai_avatar_description?.trim() ?? '';
  const copyAvatar = async () => {
    if (!avatarDescription) return;
    try {
      if (typeof navigator !== 'undefined' && navigator.clipboard?.writeText) {
        await navigator.clipboard.writeText(avatarDescription);
        toast('success', 'Avatar description copied to clipboard');
        return;
      }
      throw new Error('Clipboard write not supported in this environment');
    } catch (e) {
      toast('error', errorMessage(e));
    }
  };
  return (
    <div className="page">
      {' '}
      <div className="page-header">
        {' '}
        <h2>Push</h2>{' '}
      </div>{' '}
      <div className="section-grid">
        {' '}
        <div className="card">
          {' '}
          <h3>Character</h3>{' '}
          <select
            className="select"
            value={characterId}
            onChange={(e) => setCharacterId(e.target.value)}
            style={{ marginTop: 8 }}
          >
            {' '}
            <option value="">— pick a character —</option>{' '}
            {(characters.data ?? []).map((c: Character) => (
              <option key={c.id} value={c.id}>
                {' '}
                {c.name} ({c.id.slice(0, 8)}){' '}
              </option>
            ))}{' '}
          </select>{' '}
        </div>{' '}
        <div className="card">
          {' '}
          <h3>Target</h3>{' '}
          <select
            className="select"
            value={targetId}
            onChange={(e) => setTargetId(e.target.value)}
            style={{ marginTop: 8 }}
          >
            {' '}
            <option value="">— pick a target —</option>{' '}
            {(targets.data ?? []).map((t: Awaited<ReturnType<typeof api.listTargets>>[number]) => (
              <option key={t.id} value={t.id}>
                {' '}
                {t.label} — {t.ai_id}{' '}
              </option>
            ))}{' '}
          </select>{' '}
          {targetLabel && (
            <div className="muted mono" style={{ marginTop: 6, fontSize: 12 }}>
              {targetLabel}
            </div>
          )}{' '}
        </div>{' '}
      </div>{' '}
      {character.data && (
        <div className="card">
          {' '}
          <h3 style={{ marginBottom: 8 }}>Fields to update</h3>{' '}
          <p className="muted" style={{ fontSize: 12, marginBottom: 12 }}>
            {' '}
            Default is on for any non-empty field. Untick anything you don&apos;t want to send.{' '}
          </p>{' '}
          <FieldChecklist
            character={character.data}
            selected={selected}
            onChange={setSelected}
          />{' '}
        </div>
      )}{' '}
      {journalList.length > 0 && (
        <div className="card">
          <h3 style={{ marginBottom: 8 }}>Journal entries</h3>
          <p className="muted" style={{ fontSize: 12, marginBottom: 8 }}>
            Selected entries become one <code>POST /journal-create</code> call each, sent after{' '}
            <code>/update-info</code> and before <code>/chat-break</code>. Local only; failures
            don&apos;t abort the push.
          </p>
          <div
            style={{
              display: 'flex',
              gap: 8,
              fontSize: 12,
              marginBottom: 6,
            }}
          >
            <button
              type="button"
              className="btn btn-sm"
              onClick={() => setSelectedJournal(new Set(journalList.map((e) => e.id)))}
            >
              Select all
            </button>
            <button
              type="button"
              className="btn btn-sm"
              onClick={() => setSelectedJournal(new Set())}
            >
              Clear
            </button>
          </div>
          {journalList.map((e) => (
            <label key={e.id} className="checkbox" style={{ marginTop: 4 }}>
              <input
                type="checkbox"
                checked={selectedJournal.has(e.id)}
                onChange={(ev) => {
                  const next = new Set(selectedJournal);
                  if (ev.target.checked) next.add(e.id);
                  else next.delete(e.id);
                  setSelectedJournal(next);
                }}
              />
              <span>
                <span className="mono" style={{ fontSize: 11 }}>
                  {e.id.slice(0, 8)}
                </span>{' '}
                — {e.entry.slice(0, 80)}
                {e.keyphrases.length > 0 && (
                  <span className="muted" style={{ marginLeft: 6 }}>
                    ({e.keyphrases.join(', ')})
                  </span>
                )}
              </span>
            </label>
          ))}
        </div>
      )}{' '}
      <div className="card">
        {' '}
        <h3>Chat break</h3>{' '}
        <label className="checkbox" style={{ marginTop: 8 }}>
          {' '}
          <input
            type="checkbox"
            checked={chatBreak}
            onChange={(e) => setChatBreak(e.target.checked)}
          />{' '}
          Send a chat break after update-info{' '}
        </label>{' '}
        {chatBreak && (
          <div className="flex-col" style={{ marginTop: 12 }}>
            {' '}
            <textarea
              className="textarea"
              rows={3}
              placeholder={
                character.data?.greeting
                  ? 'Pre-filled from character'
                  : 'This character has no default greeting. Type one, or disable chat-break.'
              }
              value={greeting}
              onChange={(e) => setGreeting(e.target.value)}
            />{' '}
            <label className="checkbox">
              {' '}
              <input
                type="checkbox"
                checked={wipeCascaded}
                onChange={(e) => setWipeCascaded(e.target.checked)}
              />{' '}
              Reset Cascaded Memory{' '}
            </label>{' '}
            {wipeCascaded && (
              <div className="fieldset-warning" role="alert">
                <span>
                  <strong>⚠ Warning.</strong> This will flush all previous cascaded memories. This
                  is a nuclear option to reset conversation context continuity, but you may lose up
                  to hundreds or thousands of messages worth of Cascaded Memory if they existed.
                </span>
              </div>
            )}
          </div>
        )}{' '}
      </div>{' '}
      <div className="card">
        {' '}
        <h3>Preview</h3>{' '}
        <p className="muted" style={{ fontSize: 12, marginBottom: 8 }}>
          {' '}
          The exact body that will be POSTed.{' '}
        </p>{' '}
        <details open>
          {' '}
          <summary>update-info body</summary> <pre>{JSON.stringify(previewBody, null, 2)}</pre>{' '}
        </details>{' '}
        {previewJournal && previewJournal.length > 0 && (
          <details style={{ marginTop: 8 }}>
            <summary>journal-create bodies ({previewJournal.length})</summary>
            <pre>{JSON.stringify(previewJournal, null, 2)}</pre>
          </details>
        )}
        {previewChatBreak && (
          <details style={{ marginTop: 8 }}>
            {' '}
            <summary>chat-break body</summary>{' '}
            <pre>{JSON.stringify(previewChatBreak, null, 2)}</pre>{' '}
          </details>
        )}{' '}
      </div>{' '}
      <div className="page-header" style={{ marginTop: 0 }}>
        {' '}
        <div className="page-header-actions">
          <button
            type="button"
            className="btn"
            onClick={copyAvatar}
            disabled={!avatarDescription}
            title={
              avatarDescription
                ? 'Copy the local-only Avatar Description to the clipboard so you can paste it into Kindroid manually'
                : 'Pick a character that has an Avatar Description'
            }
          >
            Copy Avatar Description
          </button>
          <button
            disabled={!canPush}
            title={
              !character.data || !target.data
                ? 'Pick a character and target'
                : !selected.size && !journalSelected && !chatBreak
                  ? 'Select at least one field, journal entry, or enable chat-break'
                  : chatBreak && !greeting.trim()
                    ? 'Type a greeting for chat-break'
                    : ''
            }
            onClick={() => setConfirmOpen(true)}
            className="btn btn-primary"
          >
            {' '}
            {push.isPending
              ? 'Pushing…'
              : `Push ${fieldCount} field${fieldCount === 1 ? '' : 's'}${journalLabel} to ${target.data?.label ?? 'target'}`}{' '}
          </button>{' '}
        </div>{' '}
      </div>{' '}
      {result && (
        <div className="card">
          {' '}
          <h3>Result</h3> <StepRow label="update-info" step={result.update_info} />{' '}
          {result.journal_entries && result.journal_entries.length > 0 && (
            <div style={{ marginTop: 8 }}>
              <h4 style={{ margin: '8px 0 4px', fontSize: 13 }}>journal entries</h4>
              {result.journal_entries.map((s) => (
                <StepRow key={s.id} label={`journal ${s.id.slice(0, 8)}`} step={s} />
              ))}
            </div>
          )}
          {result.chat_break && (
            <div style={{ marginTop: 4 }}>
              {' '}
              <StepRow label="chat-break" step={result.chat_break} />{' '}
            </div>
          )}{' '}
          <div style={{ marginTop: 12 }}>
            {' '}
            <button className="btn" onClick={() => navigate(`/history/${result.log_id}`)}>
              {' '}
              View in history{' '}
            </button>{' '}
          </div>{' '}
        </div>
      )}{' '}
      <ConfirmDialog
        open={confirmOpen}
        title="Confirm push"
        body={`Push ${fieldCount} field${fieldCount === 1 ? '' : 's'}${journalSelected ? ` and ${journalSelected} journal entr${journalSelected === 1 ? 'y' : 'ies'}` : ''}${chatBreak ? ' and chat-break' : ''} to ${targetLabel || 'target'}?`}
        confirmLabel="Push"
        onConfirm={() => {
          setConfirmOpen(false);
          push.mutate();
        }}
        onCancel={() => setConfirmOpen(false)}
      />{' '}
    </div>
  );
}
function StepRow({ label, step }: { label: string; step: StepResult }) {
  return (
    <div className="flex-row" style={{ marginTop: 6 }}>
      {' '}
      <span className={`badge ${step.ok ? 'badge-success' : 'badge-danger'}`}>{label}</span>{' '}
      <span>
        {' '}
        {step.ok ? 'OK' : 'failed'} (status {step.status}) — {step.message}{' '}
      </span>{' '}
    </div>
  );
}
