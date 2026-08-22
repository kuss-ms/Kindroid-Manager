/**
 * Chat bubble themes: 4 presets + a custom palette editor.
 *
 * Themes are stored in `localStorage` at `kindroid-manager.chat-theme`,
 * with optional per-target overrides under
 * `kindroid-manager.chat-theme.overrides.<ai_id>`. `applyChatTheme(state,
 * aiId)` resolves the active palette (override → preset → default) and
 * writes CSS custom properties to the chat-view wrapper element.
 *
 * Independent of `useTheme` — chat themes don't follow the app's
 * light/dark mode toggle because users may want a custom palette on
 * either side.
 */

export interface ChatThemePalette {
  bg: string;
  userBubble: string;
  aiBubble: string;
  accent: string;
  text: string;
}

export interface ChatTheme {
  presetId: string;
  /** Only set when `presetId === 'custom'`. */
  custom?: ChatThemePalette;
}

export interface ChatThemeState {
  base: ChatTheme;
  /** Map of `aiId` → override theme. */
  overridesByAi: Record<string, ChatTheme>;
}

export interface ChatThemePreset {
  id: string;
  label: string;
  palette: ChatThemePalette;
}

const STORAGE_KEY = 'kindroid-manager.chat-theme';
const OVERRIDE_PREFIX = 'kindroid-manager.chat-theme.overrides.';

export const PRESET_THEMES: ChatThemePreset[] = [
  {
    id: 'default',
    label: 'Default',
    // The "Default" preset adapts to the active light/dark theme by
    // referencing the page-level CSS custom properties (--surface-2,
    // --text) instead of baking a specific colour. This way the same
    // preset renders correctly whether the user is on light or dark
    // mode — only the explicit presets (Sepia / Midnight / Paper)
    // commit to a fixed palette.
    palette: {
      bg: 'transparent',
      userBubble: 'var(--primary)',
      aiBubble: 'var(--surface-2)',
      accent: 'var(--primary)',
      text: 'var(--text)',
    },
  },
  {
    id: 'sepia',
    label: 'Sepia',
    palette: {
      bg: '#f5ecd9',
      userBubble: '#a07a4c',
      aiBubble: '#e8dcc0',
      accent: '#a07a4c',
      text: '#3e2c1a',
    },
  },
  {
    id: 'midnight',
    label: 'Midnight',
    palette: {
      bg: '#0b1220',
      userBubble: '#60a5fa',
      aiBubble: '#1f2a44',
      accent: '#60a5fa',
      text: '#e2e8f0',
    },
  },
  {
    id: 'paper',
    label: 'Paper',
    palette: {
      bg: '#ffffff',
      userBubble: '#16a34a',
      aiBubble: '#f0fdf4',
      accent: '#16a34a',
      text: '#0f172a',
    },
  },
];

export const CUSTOM_PRESET_ID = 'custom';

export const DEFAULT_THEME: ChatTheme = { presetId: 'default' };

export function defaultChatThemeState(): ChatThemeState {
  return { base: { ...DEFAULT_THEME }, overridesByAi: {} };
}

function safeGetItem(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function safeSetItem(key: string, value: string): void {
  try {
    localStorage.setItem(key, value);
  } catch {
    // localStorage unavailable — keep the in-memory state so the
    // session still works.
  }
}

function parseBase(raw: string | null): ChatTheme {
  if (!raw) return { ...DEFAULT_THEME };
  try {
    const obj = JSON.parse(raw);
    if (obj && typeof obj === 'object' && typeof obj.presetId === 'string') {
      const palette = obj.custom;
      if (obj.presetId === CUSTOM_PRESET_ID && palette) {
        return {
          presetId: CUSTOM_PRESET_ID,
          custom: sanitisePalette(palette),
        };
      }
      return { presetId: obj.presetId };
    }
  } catch {
    // fall through to default
  }
  return { ...DEFAULT_THEME };
}

function sanitisePalette(p: unknown): ChatThemePalette {
  const o = p as Record<string, unknown>;
  return {
    bg: typeof o.bg === 'string' ? o.bg : '#ffffff',
    userBubble: typeof o.userBubble === 'string' ? o.userBubble : '#2563eb',
    aiBubble: typeof o.aiBubble === 'string' ? o.aiBubble : '#f1f5f9',
    accent: typeof o.accent === 'string' ? o.accent : '#2563eb',
    text: typeof o.text === 'string' ? o.text : '#0f172a',
  };
}

export function loadChatTheme(): ChatThemeState {
  const base = parseBase(safeGetItem(STORAGE_KEY));
  const overridesByAi: Record<string, ChatTheme> = {};
  for (let i = 0; i < localStorage.length; i += 1) {
    const k = localStorage.key(i);
    if (k && k.startsWith(OVERRIDE_PREFIX)) {
      const aiId = k.slice(OVERRIDE_PREFIX.length);
      const parsed = parseBase(safeGetItem(k));
      overridesByAi[aiId] = parsed;
    }
  }
  return { base, overridesByAi };
}

export function saveChatTheme(state: ChatThemeState): void {
  safeSetItem(STORAGE_KEY, JSON.stringify(state.base));
  // Drop existing override keys first so renames don't leak.
  const toRemove: string[] = [];
  for (let i = 0; i < localStorage.length; i += 1) {
    const k = localStorage.key(i);
    if (k && k.startsWith(OVERRIDE_PREFIX)) toRemove.push(k);
  }
  toRemove.forEach((k) => localStorage.removeItem(k));
  for (const [aiId, theme] of Object.entries(state.overridesByAi)) {
    safeSetItem(OVERRIDE_PREFIX + aiId, JSON.stringify(theme));
  }
}

/**
 * Resolve the active palette for `aiId` (override → base → default),
 * write the CSS custom properties onto `el`, and return the resolved
 * palette so the caller can use it for inline styles.
 */
export function applyChatTheme(
  state: ChatThemeState,
  aiId: string,
  el: HTMLElement,
): ChatThemePalette {
  const theme = state.overridesByAi[aiId] ?? state.base;
  const palette = resolvePalette(theme);
  el.style.setProperty('--chat-bg', palette.bg);
  el.style.setProperty('--chat-user-bubble', palette.userBubble);
  el.style.setProperty('--chat-ai-bubble', palette.aiBubble);
  el.style.setProperty('--chat-accent', palette.accent);
  el.style.setProperty('--chat-text', palette.text);
  return palette;
}

export function resolvePalette(theme: ChatTheme): ChatThemePalette {
  if (theme.presetId === CUSTOM_PRESET_ID && theme.custom) {
    return theme.custom;
  }
  const preset = PRESET_THEMES.find((p) => p.id === theme.presetId);
  if (preset) return preset.palette;
  // Unknown preset id → fall back to the default preset's palette.
  return PRESET_THEMES[0].palette;
}

/**
 * `useChatTheme` returns the current state and a setter that persists.
 * The hook also re-runs on every change so subscribers update.
 */
import { useCallback, useEffect, useState } from 'react';

export function useChatTheme(): {
  state: ChatThemeState;
  setState: (next: ChatThemeState) => void;
} {
  const [state, setStateState] = useState<ChatThemeState>(() => loadChatTheme());
  const setState = useCallback((next: ChatThemeState) => {
    saveChatTheme(next);
    setStateState(next);
  }, []);
  // Re-read on mount so a second hook instance in a different subtree
  // sees the latest storage value (the storage layer is the source of
  // truth, not React state).
  useEffect(() => {
    setStateState(loadChatTheme());
  }, []);
  return { state, setState };
}
