import { useEffect, useRef, type ReactNode } from "react";
export function ContextMenu({
  x,
  y,
  onClose,
  children,
}: {
  x: number;
  y: number;
  onClose: () => void;
  children: ReactNode;
}) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const before = document.activeElement as HTMLElement;
    const menu = ref.current;
    menu?.querySelector<HTMLButtonElement>("button")?.focus();
    const close = (e: MouseEvent) => {
      if (!menu?.contains(e.target as Node)) onClose();
    };
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      }
      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault();
        const buttons = Array.from(
          menu?.querySelectorAll<HTMLButtonElement>("button:not(:disabled)") ||
            [],
        );
        const index = buttons.indexOf(
          document.activeElement as HTMLButtonElement,
        );
        buttons[
          (index + (e.key === "ArrowDown" ? 1 : -1) + buttons.length) %
            buttons.length
        ]?.focus();
      }
    };
    document.addEventListener("mousedown", close);
    document.addEventListener("keydown", key);
    window.addEventListener("resize", onClose);
    return () => {
      document.removeEventListener("mousedown", close);
      document.removeEventListener("keydown", key);
      window.removeEventListener("resize", onClose);
      before?.focus();
    };
  }, [onClose]);
  return (
    <div
      className="context-menu"
      role="menu"
      ref={ref}
      style={{
        left: Math.max(8, Math.min(x, innerWidth - 240)),
        top: Math.max(8, Math.min(y, innerHeight - 230)),
      }}
    >
      {children}
    </div>
  );
}
