import { useState } from 'react';
import { create } from 'zustand';
import type { StepResult } from '../lib/types';
export interface Toast {
  id: string;
  kind: 'success' | 'error' | 'info';
  message: string;
}
interface ToastStore {
  toasts: Toast[];
  push: (t: Omit<Toast, 'id'>) => void;
  dismiss: (id: string) => void;
}
export const useToastStore = create<ToastStore>((set) => ({
  toasts: [],
  push: (t) => {
    const id = Math.random().toString(36).slice(2);
    set((s) => ({ toasts: [...s.toasts, { ...t, id }] }));
    setTimeout(() => {
      set((s) => ({ toasts: s.toasts.filter((x) => x.id !== id) }));
    }, 5000);
  },
  dismiss: (id) => set((s) => ({ toasts: s.toasts.filter((x) => x.id !== id) })),
}));
export function toast(kind: Toast['kind'], message: string) {
  useToastStore.getState().push({ kind, message });
}
export function formatStepResult(label: string, r: StepResult | null | undefined): string {
  if (!r) return `${label}: skipped`;
  return `${label}: ${r.ok ? 'OK' : 'failed'} (status ${r.status}) — ${r.message}`;
}
export function Toaster() {
  const toasts = useToastStore((s) => s.toasts);
  const dismiss = useToastStore((s) => s.dismiss);
  return (
    <div className="toaster" role="status">
      {' '}
      {toasts.map((t) => (
        <div key={t.id} className={`toast toast-${t.kind}`}>
          {' '}
          <span>{t.message}</span>{' '}
          <button onClick={() => dismiss(t.id)} aria-label="dismiss">
            {' '}
            ×{' '}
          </button>{' '}
        </div>
      ))}{' '}
    </div>
  );
}
void useState;
