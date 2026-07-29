import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useEffect, useState } from 'react';
import { useForm } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { api, errorMessage } from '../lib/api';
import { settingsSchema } from '../lib/schemas';
import { ConfirmDialog } from '../components/ConfirmDialog';
import { toast } from '../components/Toaster';
export function SettingsPage() {
  const queryClient = useQueryClient();
  const settings = useQuery({ queryKey: ['settings'], queryFn: api.getSettings });
  const [token, setToken] = useState('');
  const [showToken, setShowToken] = useState(false);
  const [confirmClear, setConfirmClear] = useState(false);
  const {
    register,
    handleSubmit,
    reset,
    formState: { errors, isDirty },
  } = useForm<{ base_url: string }>({
    resolver: zodResolver(settingsSchema),
    defaultValues: { base_url: 'https://api.kindroid.ai/v1' },
  });
  useEffect(() => {
    if (settings.data) reset({ base_url: settings.data.base_url });
  }, [settings.data, reset]);
  const saveSettings = useMutation({
    mutationFn: (input: { base_url: string }) => api.setSettings(input),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['settings'] });
      toast('success', 'Settings saved');
    },
    onError: (e) => toast('error', errorMessage(e)),
  });
  const saveToken = useMutation({
    mutationFn: (t: string) => api.setToken(t),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['settings'] });
      toast('success', 'Token saved to OS keychain');
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
  return (
    <div className="page">
      {' '}
      <div className="page-header">
        {' '}
        <h2>Settings</h2>{' '}
      </div>{' '}
      <div className="card">
        {' '}
        <h3>API token</h3>{' '}
        <p className="muted" style={{ marginTop: 6, fontSize: 13 }}>
          {' '}
          The token is stored in your OS keychain (Windows Credential Manager, macOS Keychain, Linux
          Secret Service). It is never written to disk and never leaves the app.{' '}
        </p>{' '}
        <p className="muted" style={{ fontSize: 12, marginTop: 6 }}>
          {' '}
          Where do I find my API key and AI ID?{' '}
          <a href="https://kindroid.ai/home/" target="_blank" rel="noreferrer">
            Kindroid → Profile Settings
          </a>{' '}
        </p>{' '}
        <div className="flex-row" style={{ marginTop: 12 }}>
          {' '}
          <span
            className={`badge ${settings.data?.token_configured ? 'badge-success' : 'badge-danger'}`}
          >
            {' '}
            {settings.data?.token_configured ? 'configured' : 'not configured'}{' '}
          </span>{' '}
        </div>{' '}
        <div className="flex-row" style={{ marginTop: 12 }}>
          {' '}
          <input
            type={showToken ? 'text' : 'password'}
            className="input input-mono"
            placeholder="kn_…"
            value={token}
            onChange={(e) => setToken(e.target.value)}
            style={{ flex: 1 }}
          />{' '}
          <button className="btn" onClick={() => setShowToken((v) => !v)}>
            {' '}
            {showToken ? 'Hide' : 'Show'}{' '}
          </button>{' '}
          <button
            className="btn btn-primary"
            disabled={token.trim().length === 0}
            onClick={() => saveToken.mutate(token.trim())}
          >
            {' '}
            Save{' '}
          </button>{' '}
          <button
            className="btn btn-danger"
            disabled={!settings.data?.token_configured}
            onClick={() => setConfirmClear(true)}
          >
            {' '}
            Clear{' '}
          </button>{' '}
          <button
            className="btn"
            disabled={!settings.data?.token_configured || testToken.isPending}
            onClick={() => testToken.mutate()}
          >
            {' '}
            {testToken.isPending ? 'Testing…' : 'Test'}{' '}
          </button>{' '}
        </div>{' '}
        {testToken.data && (
          <p className="muted" style={{ fontSize: 12, marginTop: 8 }}>
            {' '}
            Test result: {testToken.data.message} — checks reachability and auth, not character
            validity.{' '}
          </p>
        )}{' '}
      </div>{' '}
      <div className="card">
        {' '}
        <h3>Base URL</h3>{' '}
        <form
          onSubmit={handleSubmit((v) => saveSettings.mutate({ base_url: v.base_url.trim() }))}
          className="flex-row"
          style={{ marginTop: 8 }}
        >
          {' '}
          <input className="input" {...register('base_url')} style={{ flex: 1 }} />{' '}
          <button type="submit" className="btn btn-primary" disabled={!isDirty}>
            {' '}
            Save{' '}
          </button>{' '}
        </form>{' '}
        {errors.base_url && <span className="form-error">{errors.base_url.message}</span>}{' '}
      </div>{' '}
      <div className="card">
        {' '}
        <h3>About</h3>{' '}
        <p className="muted" style={{ fontSize: 12 }}>
          Kindroid Manager v0.2.0
        </p>{' '}
      </div>{' '}
      <ConfirmDialog
        open={confirmClear}
        title="Clear API token?"
        body="The token is removed from your OS keychain. You'll need to re-enter it to push."
        confirmLabel="Clear"
        onConfirm={() => clearToken.mutate()}
        onCancel={() => setConfirmClear(false)}
      />{' '}
    </div>
  );
}
