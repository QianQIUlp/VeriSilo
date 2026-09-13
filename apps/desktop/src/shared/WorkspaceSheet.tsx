import { useLayoutEffect, useRef, type ReactNode } from "react";

export function WorkspaceSheet({
  title,
  onClose,
  children,
}: {
  title: string;
  onClose: () => void;
  children: ReactNode;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  useLayoutEffect(() => {
    const element = dialog.current;
    element?.showModal();
    return () => element?.close();
  }, []);
  return (
    <dialog
      ref={dialog}
      className="workspace-sheet"
      aria-label={title}
      onCancel={(event) => {
        event.preventDefault();
        onClose();
      }}
      onClick={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div className="sheet-surface">
        <header className="sheet-heading">
          <div>
            <span className="eyebrow">LOCAL WORKSPACE</span>
            <h2>{title}</h2>
          </div>
          <button
            type="button"
            className="button-secondary"
            aria-label={`关闭${title}`}
            onClick={onClose}
          >
            收起 ↗
          </button>
        </header>
        <div className="sheet-content">{children}</div>
      </div>
    </dialog>
  );
}
