import { useEffect, useLayoutEffect, useRef, useState } from 'react';

export interface RowAction {
  label: string;
  onClick: () => void;
  danger?: boolean;
  disabled?: boolean;
  title?: string;
}

// Matches `.overflow-menu-popup { min-width: 180px }` in global.css.
const POPUP_MIN_WIDTH = 180;

/**
 * Renders an action group for a list row. On desktop every action is shown
 * inline; on mobile (≤720px) only the first (primary) action is visible and
 * the rest live behind a "⋯" overflow menu. See `.list-item-secondary` /
 * `.list-item-overflow` in `global.css` for the breakpoint flip.
 *
 * The popup aligns its right edge to the ⋯ button by default (extending
 * leftward). When the button sits in the left half of the viewport — e.g.
 * the page-header in CharacterEditorPage where the ⋯ is the second of two
 * inline buttons in a full-width row — the popup would overflow the left
 * edge of the screen. We measure the button's position before paint and
 * flip the alignment so the popup stays inside the viewport.
 */
export function RowActions({ actions }: { actions: RowAction[] }) {
  const [open, setOpen] = useState(false);
  const [alignLeft, setAlignLeft] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const buttonRef = useRef<HTMLButtonElement>(null);

  useLayoutEffect(() => {
    if (!open) {
      setAlignLeft(false);
      return;
    }
    const button = buttonRef.current;
    if (!button) return;
    const buttonRect = button.getBoundingClientRect();
    const margin = 8;
    // If extending leftward by POPUP_MIN_WIDTH would cross the viewport's
    // left edge, flip to extending rightward instead. Measured off the
    // button alone so there's no first-frame flicker.
    setAlignLeft(buttonRect.right - POPUP_MIN_WIDTH < margin);
  }, [open]);

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
            ref={buttonRef}
            className="btn btn-sm btn-icon"
            onClick={() => setOpen((o) => !o)}
            aria-label="More actions"
            aria-expanded={open}
          >
            ⋯
          </button>
          {open && (
            <div
              className="overflow-menu-popup"
              style={alignLeft ? { left: 0, right: 'auto' } : undefined}
              role="menu"
            >
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
