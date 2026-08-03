import { useCallback, useEffect, useState } from 'react';

export type ThemePreference = 'light' | 'dark' | 'system';
export type EffectiveTheme = 'light' | 'dark';

const STORAGE_KEY = 'kindroid-manager.theme';
const VALID: ThemePreference[] = ['light', 'dark', 'system'];

function readStoredTheme(): ThemePreference {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw === 'light' || raw === 'dark' || raw === 'system') return raw;
  } catch {
    // localStorage may be unavailable (private mode, file://, etc.) — fall through to default.
  }
  return 'system';
}

function systemPrefersDark(): boolean {
  return typeof window !== 'undefined' && window.matchMedia('(prefers-color-scheme: dark)').matches;
}

function resolve(pref: ThemePreference): EffectiveTheme {
  return pref === 'system' ? (systemPrefersDark() ? 'dark' : 'light') : pref;
}

function applyTheme(pref: ThemePreference) {
  const effective = resolve(pref);
  document.documentElement.setAttribute('data-theme', effective);
  document.documentElement.style.colorScheme = effective;
}

/**
 * Theme preference hook. Reads the stored preference, keeps the
 * `<html data-theme>` attribute (and `color-scheme` style) in sync, and
 * listens to OS-level changes when the preference is `system`.
 *
 * The boot script in `index.html` does the synchronous initial paint
 * before React mounts, so the first frame already matches the stored
 * preference and there is no light/dark flash on app start.
 */
export function useTheme(): {
  preference: ThemePreference;
  effective: EffectiveTheme;
  setPreference: (next: ThemePreference) => void;
} {
  const [preference, setPreferenceState] = useState<ThemePreference>(() => readStoredTheme());
  const [systemDark, setSystemDark] = useState<boolean>(() => systemPrefersDark());

  useEffect(() => {
    if (preference !== 'system' || typeof window === 'undefined') return;
    const mq = window.matchMedia('(prefers-color-scheme: dark)');
    const onChange = (e: MediaQueryListEvent) => setSystemDark(e.matches);
    // matchMedia.addEventListener is the modern API; older WebView2
    // versions only support addListener. Both no-op if unsupported.
    if (mq.addEventListener) mq.addEventListener('change', onChange);
    else mq.addListener(onChange);
    setSystemDark(mq.matches);
    return () => {
      if (mq.removeEventListener) mq.removeEventListener('change', onChange);
      else mq.removeListener(onChange);
    };
  }, [preference]);

  const effective: EffectiveTheme =
    preference === 'system' ? (systemDark ? 'dark' : 'light') : preference;

  useEffect(() => {
    applyTheme(preference);
  }, [preference]);

  const setPreference = useCallback((next: ThemePreference) => {
    if (!VALID.includes(next)) return;
    try {
      localStorage.setItem(STORAGE_KEY, next);
    } catch {
      // Ignore — the in-memory state still updates, so the session keeps working.
    }
    setPreferenceState(next);
  }, []);

  return { preference, effective, setPreference };
}
