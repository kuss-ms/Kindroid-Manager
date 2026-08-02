import { useEffect, useRef, useState } from 'react';

export interface RowAction {
  label: string;
  onClick: () => void;
  danger?: boolean;
  disabled?: boolean;
  title?: string;
}

/**
 * Renders an action group for a list row. On desktop every action is shown
 * inline; on mobile (≤720px) only the first (primary) action is visible and
 * the rest live behind a "⋯" overflow menu. See `.list-item-secondary` /
 * `.list-item-overflow` in `global.css` for the breakpoint flip.
 */
export function RowActions({ actions }: { actions: RowAction[] }) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onMouseDown = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener('mousedown', onMouseDown);
    return () => document.removeEventListener('mousedown', onMouseDown);
  }, [open]);

  if (actions.length === 0) return null;
  const [primary, ...secondary] = actions;

  return (
    <div className="list-item-actions">
      <button
        className="btn btn-sm"
        onClick={primary.onClick}
        disabled={primary.disabled}
        title={primary.title}
      >
        {primary.label}
      </button>
      {secondary.map((a, i) => (
        <button
          key={i}
          className={
            'btn btn-sm list-item-secondary' + (a.danger ? ' btn-danger' : '')
          }
          onClick={a.onClick}
          disabled={a.disabled}
          title={a.title}
        >
          {a.label}
        </button>
      ))}
      {secondary.length > 0 && (
        <div className="overflow-menu list-item-overflow" ref={ref}>
          <button
            className="btn btn-sm btn-icon"
            onClick={() => setOpen((o) => !o)}
            aria-label="More actions"
            aria-expanded={open}
          >
            ⋯
          </button>
          {open && (
            <div className="overflow-menu-popup" role="menu">
              {secondary.map((a, i) => (
                <button
                  key={i}
                  className={'overflow-menu-item' + (a.danger ? ' danger' : '')}
                  onClick={() => {
                    setOpen(false);
                    a.onClick();
                  }}
                  disabled={a.disabled}
                  title={a.title}
                  role="menuitem"
                >
                  {a.label}
                </button>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
