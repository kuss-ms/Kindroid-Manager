import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useEffect, useRef, useState } from 'react';
import { NavLink, Outlet, useNavigate } from 'react-router-dom';
import { api, errorMessage } from '../lib/api';
import { OnboardingBanner } from './OnboardingBanner';
import { toast } from './Toaster';
import { useTheme } from '../lib/theme';
export function AppLayout() {
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const [dropActive, setDropActive] = useState(false);
  const dragCounter = useRef(0);
  // Mount the theme hook here so the saved preference stays applied
  // for the entire lifetime of the app shell, and any future
  // `<html data-theme>` mutations are immediately followed by
  // `color-scheme` style updates. The boot script in index.html has
  // already set the initial attribute before the first paint.
  useTheme();
  const settings = useQuery({ queryKey: ['settings'], queryFn: api.getSettings });
  const characters = useQuery({ queryKey: ['characters'], queryFn: api.listCharacters });
  const targets = useQuery({ queryKey: ['targets'], queryFn: api.listTargets });
  const showBanner =
    !!settings.data &&
    (!settings.data.token_configured ||
      (characters.data?.length ?? 0) === 0 ||
      (targets.data?.length ?? 0) === 0);
  useEffect(() => {
    const handleBytes = async (bytes: Uint8Array) => {
      try {
        const draft = await api.importShareImage(bytes);
        queryClient.setQueryData(['character', draft.id], draft);
        queryClient.invalidateQueries({ queryKey: ['characters'] });
        toast('success', `Imported "${draft.name}"`);
        navigate(`/characters/${draft.id}`);
      } catch (e) {
        toast('error', errorMessage(e));
      }
    };
    const readBytesFromFile = async (file: File) => {
      const buf = await file.arrayBuffer();
      await handleBytes(new Uint8Array(buf));
    };
    const onDrop = async (e: DragEvent) => {
      e.preventDefault();
      dragCounter.current = 0;
      setDropActive(false);
      const file = e.dataTransfer?.files[0];
      if (!file || !file.type.startsWith('image/')) {
        toast('error', 'Please drop a PNG image.');
        return;
      }
      // Prefer the in-app stash (verbatim bytes from the most recent
      // export) over the dropped file's bytes — the OS clipboard may
      // have transcoded the file and stripped the `kindroid` tEXt chunk.
      const stashed = await api.takeStashedShareImage();
      if (stashed) {
        await handleBytes(new Uint8Array(stashed));
        return;
      }
      await readBytesFromFile(file);
    };
    const onPaste = async (e: ClipboardEvent) => {
      const items = e.clipboardData?.items;
      if (!items) return;
      // Prefer the in-app stash first (see onDrop for rationale).
      const stashed = await api.takeStashedShareImage();
      if (stashed) {
        e.preventDefault();
        await handleBytes(new Uint8Array(stashed));
        return;
      }
      for (const item of items) {
        if (item.type.startsWith('image/')) {
          const file = item.getAsFile();
          if (file) {
            e.preventDefault();
            await readBytesFromFile(file);
            return;
          }
        }
      }
    };
    const onDragEnter = (e: DragEvent) => {
      e.preventDefault();
      if (e.dataTransfer?.types.includes('Files')) {
        dragCounter.current += 1;
        setDropActive(true);
      }
    };
    const onDragLeave = (e: DragEvent) => {
      e.preventDefault();
      dragCounter.current = Math.max(0, dragCounter.current - 1);
      if (dragCounter.current === 0) setDropActive(false);
    };
    const onDragOver = (e: DragEvent) => {
      e.preventDefault();
    };
    window.addEventListener('dragenter', onDragEnter);
    window.addEventListener('dragleave', onDragLeave);
    window.addEventListener('dragover', onDragOver);
    window.addEventListener('drop', onDrop);
    window.addEventListener('paste', onPaste);
    return () => {
      window.removeEventListener('dragenter', onDragEnter);
      window.removeEventListener('dragleave', onDragLeave);
      window.removeEventListener('dragover', onDragOver);
      window.removeEventListener('drop', onDrop);
      window.removeEventListener('paste', onPaste);
    };
  }, [queryClient, navigate]);
  return (
    <div className="app">
      {' '}
      <header className="app-header">
        {' '}
        <div className="app-brand">
          {' '}
          <span className="app-brand-mark">K</span>{' '}
          <span className="app-brand-text">Kindroid Manager</span>{' '}
        </div>{' '}
        <nav className="app-nav">
          {' '}
          <NavLink to="/characters" className={({ isActive }) => (isActive ? 'active' : '')}>
            {' '}
            Characters{' '}
          </NavLink>{' '}
          <NavLink to="/targets" className={({ isActive }) => (isActive ? 'active' : '')}>
            {' '}
            Targets{' '}
          </NavLink>{' '}
          <NavLink to="/push" className={({ isActive }) => (isActive ? 'active' : '')}>
            {' '}
            Push{' '}
          </NavLink>{' '}
          <NavLink to="/history" className={({ isActive }) => (isActive ? 'active' : '')}>
            {' '}
            Push History{' '}
          </NavLink>{' '}
          <NavLink to="/chat-history" className={({ isActive }) => (isActive ? 'active' : '')}>
            {' '}
            Chat History{' '}
          </NavLink>{' '}
          <NavLink to="/settings" className={({ isActive }) => (isActive ? 'active' : '')}>
            {' '}
            Settings{' '}
          </NavLink>{' '}
        </nav>{' '}
      </header>{' '}
      {showBanner && <OnboardingBanner />}{' '}
      <main className="app-main">
        {' '}
        <Outlet />{' '}
      </main>{' '}
      <nav className="app-bottom-nav">
        {' '}
        <NavLink to="/characters" className={({ isActive }) => (isActive ? 'active' : '')}>
          {' '}
          Characters{' '}
        </NavLink>{' '}
        <NavLink to="/targets" className={({ isActive }) => (isActive ? 'active' : '')}>
          {' '}
          Targets{' '}
        </NavLink>{' '}
        <NavLink to="/push" className={({ isActive }) => (isActive ? 'active' : '')}>
          {' '}
          Push{' '}
        </NavLink>{' '}
        <NavLink to="/history" className={({ isActive }) => (isActive ? 'active' : '')}>
          {' '}
          History{' '}
        </NavLink>{' '}
        <NavLink to="/chat-history" className={({ isActive }) => (isActive ? 'active' : '')}>
          {' '}
          Chat{' '}
        </NavLink>{' '}
        <NavLink to="/settings" className={({ isActive }) => (isActive ? 'active' : '')}>
          {' '}
          Settings{' '}
        </NavLink>{' '}
      </nav>{' '}
      {dropActive && (
        <div className="drop-overlay">
          {' '}
          <div className="drop-card">
            {' '}
            <div className="drop-icon">⬇</div>{' '}
            <div className="drop-title">Drop a Kindroid share image</div>{' '}
            <div className="drop-sub">PNG with embedded persona metadata</div>{' '}
          </div>{' '}
        </div>
      )}{' '}
    </div>
  );
}
