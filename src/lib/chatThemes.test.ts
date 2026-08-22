import { describe, expect, it, beforeEach } from 'vitest';
import {
  applyChatTheme,
  CUSTOM_PRESET_ID,
  defaultChatThemeState,
  loadChatTheme,
  PRESET_THEMES,
  resolvePalette,
  saveChatTheme,
} from './chatThemes';

function clearStorage(): void {
  const keys: string[] = [];
  for (let i = 0; i < localStorage.length; i += 1) {
    const k = localStorage.key(i);
    if (k) keys.push(k);
  }
  keys.forEach((k) => localStorage.removeItem(k));
}

describe('chatThemes', () => {
  beforeEach(() => {
    clearStorage();
  });

  it('returns the default state when storage is empty', () => {
    const state = loadChatTheme();
    expect(state.base.presetId).toBe('default');
    expect(state.overridesByAi).toEqual({});
  });

  it('round-trips a base theme through save/load', () => {
    const state = defaultChatThemeState();
    state.base = { presetId: 'sepia' };
    saveChatTheme(state);
    const loaded = loadChatTheme();
    expect(loaded.base.presetId).toBe('sepia');
    expect(loaded.overridesByAi).toEqual({});
  });

  it('round-trips a custom palette', () => {
    const state = defaultChatThemeState();
    state.base = {
      presetId: CUSTOM_PRESET_ID,
      custom: {
        bg: '#000000',
        userBubble: '#111111',
        userText: '#ffffff',
        aiBubble: '#222222',
        accent: '#333333',
        text: '#ffffff',
      },
    };
    saveChatTheme(state);
    const loaded = loadChatTheme();
    expect(loaded.base.presetId).toBe(CUSTOM_PRESET_ID);
    expect(loaded.base.custom?.bg).toBe('#000000');
    expect(loaded.base.custom?.userText).toBe('#ffffff');
  });

  it('round-trips per-ai overrides', () => {
    const state = defaultChatThemeState();
    state.overridesByAi['ai_x'] = { presetId: 'midnight' };
    state.overridesByAi['ai_y'] = { presetId: 'paper' };
    saveChatTheme(state);
    const loaded = loadChatTheme();
    expect(loaded.overridesByAi['ai_x']?.presetId).toBe('midnight');
    expect(loaded.overridesByAi['ai_y']?.presetId).toBe('paper');
  });

  it('falls back to the default preset when the stored id is unknown', () => {
    localStorage.setItem('kindroid-manager.chat-theme', JSON.stringify({ presetId: 'nonsense' }));
    const loaded = loadChatTheme();
    expect(loaded.base.presetId).toBe('nonsense');
    const palette = resolvePalette(loaded.base);
    expect(palette).toEqual(PRESET_THEMES[0].palette);
  });

  it('resolvePalette prefers an override for the active ai_id', () => {
    const state = defaultChatThemeState();
    state.base = { presetId: 'default' };
    state.overridesByAi['ai_x'] = { presetId: 'midnight' };
    const el = document.createElement('div');
    applyChatTheme(state, 'ai_x', el);
    expect(el.style.getPropertyValue('--chat-bg')).toBe(
      PRESET_THEMES.find((p) => p.id === 'midnight')!.palette.bg,
    );
  });

  it('applyChatTheme falls back to the base palette when no override exists', () => {
    const state = defaultChatThemeState();
    state.base = { presetId: 'paper' };
    const el = document.createElement('div');
    applyChatTheme(state, 'ai_x', el);
    expect(el.style.getPropertyValue('--chat-bg')).toBe(
      PRESET_THEMES.find((p) => p.id === 'paper')!.palette.bg,
    );
  });

  it('applyChatTheme writes --chat-user-text from the active palette', () => {
    const state = defaultChatThemeState();
    state.base = { presetId: 'midnight' };
    const el = document.createElement('div');
    applyChatTheme(state, 'ai_x', el);
    expect(el.style.getPropertyValue('--chat-user-text')).toBe(
      PRESET_THEMES.find((p) => p.id === 'midnight')!.palette.userText,
    );
  });

  it('sanitisePalette fills in a default userText when missing', () => {
    // Legacy overrides saved before the userText field existed should
    // still parse and default to white.
    localStorage.setItem(
      'kindroid-manager.chat-theme',
      JSON.stringify({
        presetId: 'custom',
        custom: {
          bg: '#000000',
          userBubble: '#111111',
          aiBubble: '#222222',
          accent: '#333333',
          text: '#ffffff',
        },
      }),
    );
    const loaded = loadChatTheme();
    expect(loaded.base.custom?.userText).toBe('#ffffff');
  });

  it('drops override keys that no longer exist on save', () => {
    saveChatTheme({
      base: { presetId: 'default' },
      overridesByAi: { ai_old: { presetId: 'midnight' } },
    });
    // Now overwrite without `ai_old`.
    saveChatTheme({
      base: { presetId: 'default' },
      overridesByAi: { ai_new: { presetId: 'sepia' } },
    });
    const loaded = loadChatTheme();
    expect(loaded.overridesByAi['ai_old']).toBeUndefined();
    expect(loaded.overridesByAi['ai_new']?.presetId).toBe('sepia');
  });
});
