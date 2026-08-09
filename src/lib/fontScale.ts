import { useCallback, useEffect, useState } from 'react';

export type FontScaleId = 'small' | 'medium' | 'large' | 'extraLarge';

export interface FontScaleOption {
  id: FontScaleId;
  label: string;
  value: number;
}

export const FONT_SCALES: readonly FontScaleOption[] = [
  { id: 'small', label: 'Small', value: 0.875 },
  { id: 'medium', label: 'Medium', value: 1 },
  { id: 'large', label: 'Large', value: 1.125 },
  { id: 'extraLarge', label: 'Extra Large', value: 1.25 },
] as const;

const STORAGE_KEY = 'kindroid-manager.fontScale';
const VALID_IDS = new Set<FontScaleId>(FONT_SCALES.map((o) => o.id));
const VALUE_BY_ID = new Map<FontScaleId, number>(FONT_SCALES.map((o) => [o.id, o.value]));

function readStoredScale(): FontScaleId {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw && VALID_IDS.has(raw as FontScaleId)) return raw as FontScaleId;
  } catch {
    // localStorage may be unavailable (private mode, file://, etc.) — fall through to default.
  }
  return 'medium';
}

function applyScale(id: FontScaleId) {
  const value = VALUE_BY_ID.get(id) ?? 1;
  document.documentElement.style.setProperty('--app-font-scale', String(value));
}

/**
 * Font-scale preference hook. Reads the stored preset id, keeps the
 * `--app-font-scale` CSS custom property on `<html>` in sync, and
 * exposes the current numeric value so the Settings UI can render a
 * readout. The boot script in `index.html` applies the saved scale
 * before first paint so there is no flash of default-size text on
 * reload.
 */
export function useFontScale(): {
  scale: FontScaleId;
  value: number;
  setScale: (next: FontScaleId) => void;
} {
  const [scale, setScaleState] = useState<FontScaleId>(() => readStoredScale());

  useEffect(() => {
    applyScale(scale);
  }, [scale]);

  const setScale = useCallback((next: FontScaleId) => {
    if (!VALID_IDS.has(next)) return;
    try {
      localStorage.setItem(STORAGE_KEY, next);
    } catch {
      // Ignore — the in-memory state still updates, so the session keeps working.
    }
    setScaleState(next);
  }, []);

  return { scale, value: VALUE_BY_ID.get(scale) ?? 1, setScale };
}
