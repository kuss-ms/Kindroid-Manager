import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useEffect, useState } from 'react';
import { useForm } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { api, errorMessage } from '../lib/api';
import { aiSettingsSchema, automationInstructionsSchema, settingsSchema } from '../lib/schemas';
import { ConfirmDialog } from '../components/ConfirmDialog';
import { toast } from '../components/Toaster';

export function SettingsPage() {
  const queryClient = useQueryClient();
  const settings = useQuery({ queryKey: ['settings'], queryFn: api.getSettings });
  const aiSettings = useQuery({ queryKey: ['ai-settings'], queryFn: api.getAiSettings });
  const [token, setToken] = useState('');
  const [showToken, setShowToken] = useState(false);
  const [confirmClear, setConfirmClear] = useState(false);
  const [aiToken, setAiToken] = useState('');
  const [showAiToken, setShowAiToken] = useState(false);
  const [confirmClearAi, setConfirmClearAi] = useState(false);
  const {
    register,
    reset,
    watch,
    formState: { errors, isDirty },
  } = useForm<{ base_url: string }>({
    resolver: zodResolver(settingsSchema),
    defaultValues: { base_url: 'https://api.kindroid.ai/v1' },
  });
  const {
    register: registerAi,
    handleSubmit: handleSubmitAi,
    reset: resetAi,
    formState: { errors: aiErrors, isDirty: aiIsDirty },
    watch: watchAi,
  } = useForm<{ base_url: string; model: string }>({
    resolver: zodResolver(aiSettingsSchema),
    defaultValues: { base_url: 'https://api.openai.com/v1', model: '' },
  });
  useEffect(() => {
    if (settings.data) reset({ base_url: settings.data.base_url });
  }, [settings.data, reset]);
  useEffect(() => {
    if (aiSettings.data)
      resetAi({ base_url: aiSettings.data.base_url, model: aiSettings.data.model });
  }, [aiSettings.data, resetAi]);
  const saveAll = useMutation({
    mutationFn: async () => {
      const values = { base_url: watch('base_url').trim() };
      const ok = await settingsSchema.safeParseAsync(values);
      if (!ok.success) {
        throw new Error(ok.error.issues[0]?.message ?? 'Invalid base URL');
      }
      await api.setSettings({ base_url: values.base_url });
      if (token.trim().length > 0) {
        await api.setToken(token.trim());
      }
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['settings'] });
      toast('success', 'Settings saved');
      setToken('');
    },
    onError: (e) => toast('error', errorMessage(e)),
  });
  const clearToken = useMutation({
    mutationFn: () => api.clearToken(),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['settings'] });
      toast('success', 'Token cleared');
      setConfirmClear(false);
    },
    onError: (e) => toast('error', errorMessage(e)),
  });
  const testToken = useMutation<Awaited<ReturnType<typeof api.testToken>>, unknown, void>({
    mutationFn: () => api.testToken(),
    onSuccess: (r) => {
      if (r.ok) toast('success', r.message);
      else toast('error', r.message);
    },
    onError: (e) => toast('error', errorMessage(e)),
  });

  const aiFormValues = watchAi();
  const saveAiSettings = useMutation({
    mutationFn: async (input: { base_url: string; model: string }) => {
      if (aiToken.trim().length > 0) {
        await api.setAiToken(aiToken.trim());
      }
      await api.setAiSettings({ base_url: input.base_url.trim(), model: input.model });
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['ai-settings'] });
      toast('success', 'AI settings saved');
      setAiToken('');
    },
    onError: (e) => toast('error', errorMessage(e)),
  });
  const clearAiToken = useMutation({
    mutationFn: () => api.clearAiToken(),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['ai-settings'] });
      toast('success', 'AI token cleared');
      setConfirmClearAi(false);
    },
    onError: (e) => toast('error', errorMessage(e)),
  });
  const testAiConnection = useMutation({
    mutationFn: () =>
      api.testAiConnection({
        base_url: aiFormValues.base_url.trim(),
        model: aiFormValues.model,
        bearer_token: aiToken.trim() === '' ? null : aiToken.trim(),
      }),
    onSuccess: (r) => {
      if (r.ok) toast('success', r.message);
      else toast('error', r.message);
    },
    onError: (e) => toast('error', errorMessage(e)),
  });

  const instructionsDefaults = useQuery({
    queryKey: ['automation-instruction-defaults'],
    queryFn: api.getAutomationInstructionsDefaults,
    staleTime: Infinity,
  });

  const {
    register: registerInstr,
    handleSubmit: handleSubmitInstr,
    reset: resetInstr,
    setValue: setInstrValue,
    formState: { errors: instrErrors, isDirty: instrIsDirty },
  } = useForm<{ journal: string; summary: string }>({
    resolver: zodResolver(automationInstructionsSchema),
    defaultValues: { journal: '', summary: '' },
  });

  useEffect(() => {
    if (instructionsDefaults.data) {
      resetInstr({
        journal: instructionsDefaults.data.journal,
        summary: instructionsDefaults.data.summary,
      });
    }
  }, [instructionsDefaults.data, resetInstr]);

  const saveInstructions = useMutation({
    mutationFn: (input: { journal: string; summary: string }) =>
      api.setAutomationInstructions(input),
    onSuccess: async () => {
      // Re-fetch the canonical defaults from the server so the form
      // resets to what was actually saved (the previous code reset to
      // the pre-save cached values, leaving the UI out of sync with the
      // server until the next page reload — audit M7).
      const data = await queryClient.fetchQuery({
        queryKey: ['automation-instruction-defaults'],
        queryFn: api.getAutomationInstructionsDefaults,
      });
      resetInstr({ journal: data.journal, summary: data.summary });
      toast('success', 'Automation instructions saved');
    },
    onError: (e) => toast('error', errorMessage(e)),
  });

  return (
    <div className="page">
      <div className="page-header">
        <h2>Settings</h2>
      </div>
      <div className="card">
        <h3>Kindroid</h3>
        <p className="muted" style={{ fontSize: 12, marginTop: 6 }}>
          The token is stored in your OS keychain (Windows Credential Manager, macOS Keychain, Linux
          Secret Service). It is never written to disk and never leaves the app.
        </p>
        <p className="muted" style={{ fontSize: 12, marginTop: 6 }}>
          Where do I find my API key and AI ID?{' '}
          <a href="https://kindroid.ai/home/" target="_blank" rel="noreferrer">
            Kindroid → Profile Settings
          </a>
        </p>
        <div className="flex-row" style={{ marginTop: 12 }}>
          <span
            className={`badge ${settings.data?.token_configured ? 'badge-success' : 'badge-danger'}`}
          >
            {settings.data?.token_configured ? 'configured' : 'No token configured'}
          </span>
        </div>
        <form onSubmit={(e) => e.preventDefault()} className="flex-col" style={{ marginTop: 12 }}>
          <label className="form-label">Base URL</label>
          <input
            className="input"
            placeholder="https://api.kindroid.ai/v1"
            {...register('base_url')}
          />
          {errors.base_url && <span className="form-error">{errors.base_url.message}</span>}
          <label className="form-label" style={{ marginTop: 8 }}>
            Token
          </label>
          <div className="flex-row">
            <input
              type={showToken ? 'text' : 'password'}
              className="input input-mono"
              placeholder="kn_… (leave empty to keep existing)"
              value={token}
              onChange={(e) => setToken(e.target.value)}
              style={{ flex: 1 }}
            />
            <button type="button" className="btn" onClick={() => setShowToken((v) => !v)}>
              {showToken ? 'Hide' : 'Show'}
            </button>
          </div>
          <div className="flex-row" style={{ marginTop: 12 }}>
            <button
              type="button"
              className="btn btn-primary"
              disabled={!isDirty && token.trim().length === 0}
              onClick={() => saveAll.mutate()}
            >
              Save
            </button>
            <button
              type="button"
              className="btn btn-danger"
              disabled={!settings.data?.token_configured}
              onClick={() => setConfirmClear(true)}
            >
              Clear token
            </button>
            <button
              type="button"
              className="btn"
              disabled={!settings.data?.token_configured || testToken.isPending}
              onClick={() => testToken.mutate()}
            >
              {testToken.isPending ? 'Testing…' : 'Test connection'}
            </button>
          </div>
          {testToken.data && (
            <p className="muted" style={{ fontSize: 12, marginTop: 8 }}>
              Test result: {testToken.data.message} — checks reachability and auth, not character
              validity.
            </p>
          )}
        </form>
      </div>
      <div className="card">
        <h3>AI provider (OpenAI-compatible)</h3>
        <p className="muted" style={{ fontSize: 12, marginTop: 6 }}>
          Used by chat-history automation (auto-journal, auto-summary). Leave the bearer token empty
          for local servers that don&apos;t require auth.
        </p>
        <div className="flex-row" style={{ marginTop: 12 }}>
          <span
            className={`badge ${aiSettings.data?.token_configured ? 'badge-success' : 'badge-danger'}`}
          >
            {aiSettings.data?.token_configured ? 'configured' : 'No token configured'}
          </span>
        </div>
        <form
          onSubmit={handleSubmitAi((v) =>
            saveAiSettings.mutate({ base_url: v.base_url.trim(), model: v.model }),
          )}
          className="flex-col"
          style={{ marginTop: 12 }}
        >
          <label className="form-label">Base URL</label>
          <input
            className="input"
            placeholder="https://api.openai.com/v1"
            {...registerAi('base_url')}
          />
          {aiErrors.base_url && <span className="form-error">{aiErrors.base_url.message}</span>}
          <label className="form-label" style={{ marginTop: 8 }}>
            Model
          </label>
          <input
            className="input"
            placeholder="leave empty for server default"
            {...registerAi('model')}
          />
          <label className="form-label" style={{ marginTop: 8 }}>
            Bearer token
          </label>
          <div className="flex-row">
            <input
              type={showAiToken ? 'text' : 'password'}
              className="input input-mono"
              placeholder="sk-… (leave empty to keep existing)"
              value={aiToken}
              onChange={(e) => setAiToken(e.target.value)}
              style={{ flex: 1 }}
            />
            <button type="button" className="btn" onClick={() => setShowAiToken((v) => !v)}>
              {showAiToken ? 'Hide' : 'Show'}
            </button>
          </div>
          <div className="flex-row" style={{ marginTop: 12 }}>
            <button
              type="submit"
              className="btn btn-primary"
              disabled={!aiIsDirty && aiToken.trim().length === 0}
            >
              Save
            </button>
            <button
              type="button"
              className="btn btn-danger"
              disabled={!aiSettings.data?.token_configured}
              onClick={() => setConfirmClearAi(true)}
            >
              Clear token
            </button>
            <button
              type="button"
              className="btn"
              disabled={testAiConnection.isPending}
              onClick={() => testAiConnection.mutate()}
            >
              {testAiConnection.isPending ? 'Testing…' : 'Test connection'}
            </button>
          </div>
        </form>
      </div>
      <div className="card">
        <h3>Automation instructions (global defaults)</h3>
        <p className="muted" style={{ fontSize: 12, marginTop: 6 }}>
          Default instructions sent to the AI provider for auto-journal and auto-summary. Each
          target on the Chat History page can override these per feature. Use{' '}
          <code>{'{ai_name}'}</code> in either field and it will be replaced with the AI&apos;s name
          (the target&apos;s label) when the prompt is sent.
        </p>
        <form
          onSubmit={handleSubmitInstr((v) =>
            saveInstructions.mutate({ journal: v.journal, summary: v.summary }),
          )}
          className="flex-col"
          style={{ marginTop: 12 }}
        >
          <label className="form-label">Auto-journal instructions</label>
          <textarea className="textarea" rows={4} maxLength={4000} {...registerInstr('journal')} />
          {instrErrors.journal && <span className="form-error">{instrErrors.journal.message}</span>}
          <div className="flex-row" style={{ marginTop: 4 }}>
            <button
              type="button"
              className="btn btn-sm"
              disabled={!instructionsDefaults.data}
              onClick={() =>
                setInstrValue('journal', instructionsDefaults.data?.journal ?? '', {
                  shouldDirty: true,
                })
              }
            >
              Restore default
            </button>
          </div>
          <label className="form-label" style={{ marginTop: 12 }}>
            Auto-summary instructions
          </label>
          <textarea className="textarea" rows={4} maxLength={4000} {...registerInstr('summary')} />
          {instrErrors.summary && <span className="form-error">{instrErrors.summary.message}</span>}
          <div className="flex-row" style={{ marginTop: 4 }}>
            <button
              type="button"
              className="btn btn-sm"
              disabled={!instructionsDefaults.data}
              onClick={() =>
                setInstrValue('summary', instructionsDefaults.data?.summary ?? '', {
                  shouldDirty: true,
                })
              }
            >
              Restore default
            </button>
          </div>
          <div className="flex-row" style={{ marginTop: 12 }}>
            <button
              type="submit"
              className="btn btn-primary"
              disabled={!instrIsDirty || saveInstructions.isPending}
            >
              {saveInstructions.isPending ? 'Saving…' : 'Save instructions'}
            </button>
          </div>
        </form>
      </div>
      <div className="card">
        <h3>About</h3>
        <p className="muted" style={{ fontSize: 12 }}>
          Kindroid Manager v0.3.0
        </p>
      </div>
      <ConfirmDialog
        open={confirmClear}
        title="Clear API token?"
        body="The token is removed from your OS keychain. You'll need to re-enter it to push."
        confirmLabel="Clear"
        onConfirm={() => clearToken.mutate()}
        onCancel={() => setConfirmClear(false)}
      />
      <ConfirmDialog
        open={confirmClearAi}
        title="Clear AI bearer token?"
        body="The token is removed from your OS keychain. You'll need to re-enter it to send requests to providers that require auth."
        confirmLabel="Clear"
        onConfirm={() => clearAiToken.mutate()}
        onCancel={() => setConfirmClearAi(false)}
      />
    </div>
  );
}
