interface ConfirmDialogProps {
  open: boolean;
  title: string;
  body: string;
  confirmLabel?: string;
  cancelLabel?: string;
  onConfirm: () => void;
  onCancel: () => void;
}
export function ConfirmDialog({
  open,
  title,
  body,
  confirmLabel = 'Confirm',
  cancelLabel = 'Cancel',
  onConfirm,
  onCancel,
}: ConfirmDialogProps) {
  if (!open) return null;
  return (
    <div
      className="modal-overlay"
      role="dialog"
      aria-modal="true"
      aria-label={title}
      onKeyDown={(e) => {
        if (e.key === 'Escape') onCancel();
        if (e.key === 'Enter') onConfirm();
      }}
    >
      {' '}
      <div className="modal">
        {' '}
        <h3>{title}</h3> <p className="modal-body">{body}</p>{' '}
        <div className="modal-actions">
          {' '}
          <button className="btn" onClick={onCancel}>
            {' '}
            {cancelLabel}{' '}
          </button>{' '}
          <button className="btn btn-danger" onClick={onConfirm}>
            {' '}
            {confirmLabel}{' '}
          </button>{' '}
        </div>{' '}
      </div>{' '}
    </div>
  );
}
