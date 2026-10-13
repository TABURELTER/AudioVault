import { useEffect, useRef, type ReactNode } from "react";
import { X } from "lucide-react";
export function Modal({
  title,
  onClose,
  children,
}: {
  title: string;
  onClose: () => void;
  children: ReactNode;
}) {
  const ref = useRef<HTMLElement>(null);
  const close = useRef(onClose);
  useEffect(() => {
    close.current = onClose;
  }, [onClose]);
  useEffect(() => {
    const before = document.activeElement as HTMLElement;
    const elements = () =>
      Array.from(
        ref.current?.querySelectorAll<HTMLElement>(
          "button:not(:disabled), input, select, textarea, a[href]",
        ) || [],
      );
    (
      ref.current?.querySelector<HTMLElement>("input, textarea") ||
      elements()[0]
    )?.focus();
    const key = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.stopImmediatePropagation();
        close.current();
      }
      if (event.key === "Tab") {
        const targets = elements();
        if (event.shiftKey && document.activeElement === targets[0]) {
          event.preventDefault();
          targets.at(-1)?.focus();
        } else if (
          !event.shiftKey &&
          document.activeElement === targets.at(-1)
        ) {
          event.preventDefault();
          targets[0]?.focus();
        }
      }
    };
    document.addEventListener("keydown", key, true);
    return () => {
      document.removeEventListener("keydown", key, true);
      before?.focus();
    };
  }, []);
  return (
    <div
      className="modal-backdrop"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <section
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-label={title}
        ref={ref}
      >
        <div
          style={{
            display: "flex",
            justifyContent: "space-between",
            alignItems: "center",
          }}
        >
          <h2>{title}</h2>
          <button
            className="plain-icon"
            aria-label="Закрыть диалог"
            onClick={onClose}
          >
            <X size={16} />
          </button>
        </div>
        {children}
      </section>
    </div>
  );
}
