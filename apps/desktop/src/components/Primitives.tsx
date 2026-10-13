import { useEffect, useRef, useState, type ReactNode } from "react";
import { BookOpen, Loader2, Search, X } from "lucide-react";
import { api } from "../api/client";

export function SearchField({
  value,
  onChange,
  placeholder = "Название, автор или чтец",
  loading = false,
  shortcut = false,
  autoFocus = false,
}: {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  loading?: boolean;
  shortcut?: boolean;
  autoFocus?: boolean;
}) {
  return (
    <div className="search-field">
      {loading ? <Loader2 size={15} className="spin" /> : <Search size={15} />}
      <input
        type="search"
        aria-label={placeholder}
        placeholder={placeholder}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        autoFocus={autoFocus}
        data-search={shortcut ? "global" : "local"}
      />
      {value ? (
        <button
          type="button"
          className="plain-icon"
          aria-label="Очистить поиск"
          onClick={() => onChange("")}
        >
          <X size={14} />
        </button>
      ) : (
        shortcut && <kbd>Ctrl K</kbd>
      )}
    </div>
  );
}
export function EmptyState({
  title,
  description,
  action,
}: {
  title: string;
  description: string;
  action?: ReactNode;
}) {
  return (
    <div className="empty-state">
      <BookOpen size={28} strokeWidth={1.3} />
      <h3>{title}</h3>
      <p>{description}</p>
      {action}
    </div>
  );
}
export function StatusIndicator({
  online,
  label,
}: {
  online: boolean;
  label?: string;
}) {
  return (
    <span className="status-indicator">
      <span className={`dot ${online ? "dot-online" : "dot-offline"}`} />
      {label || (online ? "Подключён" : "Отключён")}
    </span>
  );
}
export function getFormatClass(fmt?: string): string {
  if (!fmt) return "";
  const f = fmt.toLowerCase().trim();
  if (f.includes("fb2")) return "format-fb2";
  if (f === "epub") return "format-epub";
  if (f === "pdf") return "format-pdf";
  if (f === "djvu") return "format-djvu";
  if (f === "mobi") return "format-mobi";
  if (f.includes("zip")) return "format-zip";
  return "";
}

export function getCoverPalette(title: string, format?: string) {
  const f = (format || "").toLowerCase();
  if (f.includes("fb2")) {
    return {
      background: "linear-gradient(145deg, #064e3b 0%, #047857 55%, #022c22 100%)",
      accent: "#34d399",
      spine: "#059669",
    };
  }
  if (f === "epub") {
    return {
      background: "linear-gradient(145deg, #3b0764 0%, #6d28d9 55%, #1e1b4b 100%)",
      accent: "#c084fc",
      spine: "#7c3aed",
    };
  }
  if (f === "pdf") {
    return {
      background: "linear-gradient(145deg, #450a0a 0%, #b91c1c 55%, #2a0808 100%)",
      accent: "#f87171",
      spine: "#dc2626",
    };
  }
  if (f === "djvu") {
    return {
      background: "linear-gradient(145deg, #451a03 0%, #b45309 55%, #260e02 100%)",
      accent: "#fbbf24",
      spine: "#d97706",
    };
  }
  if (f === "mobi") {
    return {
      background: "linear-gradient(145deg, #082f49 0%, #0284c7 55%, #0c1c2e 100%)",
      accent: "#38bdf8",
      spine: "#0369a1",
    };
  }
  if (f.includes("zip")) {
    return {
      background: "linear-gradient(145deg, #4a044e 0%, #a21caf 55%, #2e0854 100%)",
      accent: "#f472b6",
      spine: "#c026d3",
    };
  }
  const hash = title.split("").reduce((acc, c) => acc + c.charCodeAt(0), 0);
  const palettes = [
    { background: "linear-gradient(145deg, #0f2b48 0%, #2563eb 55%, #0a192f 100%)", accent: "#60a5fa", spine: "#1d4ed8" },
    { background: "linear-gradient(145deg, #2d1844 0%, #7e22ce 55%, #180b26 100%)", accent: "#c084fc", spine: "#6b21a8" },
    { background: "linear-gradient(145deg, #0d3829 0%, #059669 55%, #062118 100%)", accent: "#34d399", spine: "#047857" },
    { background: "linear-gradient(145deg, #3d1c16 0%, #c2410c 55%, #210d0a 100%)", accent: "#fb923c", spine: "#9a3412" },
    { background: "linear-gradient(145deg, #162a3d 0%, #0284c7 55%, #0a1824 100%)", accent: "#38bdf8", spine: "#0369a1" },
    { background: "linear-gradient(145deg, #3d142d 0%, #be185d 55%, #240819 100%)", accent: "#f472b6", spine: "#9d174d" },
  ];
  return palettes[Math.abs(hash) % palettes.length];
}

export function BookCover({
  title,
  thumb,
  large = false,
  format,
  author,
  size,
}: {
  title: string;
  thumb?: string;
  large?: boolean;
  format?: string;
  author?: string;
  size?: "compact" | "medium" | "large" | "xlarge";
}) {
  const [cover, setCover] = useState<{ path?: string; url: string | null }>({
    url: null,
  });
  const url = cover.path === thumb ? cover.url : null;
  useEffect(() => {
    let alive = true;
    if (thumb)
      api.getCover(thumb).then((value) => {
        if (alive) setCover({ path: thumb, url: value });
      });
    return () => {
      alive = false;
    };
  }, [thumb]);

  const palette = getCoverPalette(title || "", format);
  const sizeClass = size || (large ? "large" : "medium");
  const iconSize = size === "compact" ? 14 : size === "large" ? 22 : size === "xlarge" ? 28 : (large ? 32 : 16);

  return (
    <div
      className={`book-cover ${sizeClass}`}
      style={!url ? { background: palette.background, borderLeftColor: palette.spine } : undefined}
      aria-hidden="true"
    >
      {url ? (
        <img
          src={url}
          alt=""
          loading="lazy"
          onError={() => setCover({ path: thumb, url: null })}
        />
      ) : (
        <>
          <div className="cover-spine" style={{ background: palette.spine }} />
          {large && !size && format && (
            <span className={`badge badge-format ${getFormatClass(format)} cover-badge`}>
              {format.toUpperCase()}
            </span>
          )}
          <BookOpen size={iconSize} strokeWidth={1.4} color={palette.accent} />
          {large && !size && (
            <div className="cover-text">
              <span className="cover-title">{title}</span>
              {author && <span className="cover-author">{author}</span>}
            </div>
          )}
        </>
      )}
    </div>
  );
}
export function LoadingRows() {
  return (
    <div className="loading-rows" role="status" aria-label="Загрузка каталога">
      {Array.from({ length: 8 }, (_, i) => (
        <div key={i}>
          <span />
          <span />
          <span />
        </div>
      ))}
    </div>
  );
}
/** Fixed-height windowing; ResizeObserver accommodates resizing and desktop zoom. */
export function VirtualList({
  count,
  rowHeight,
  renderRow,
  header,
  className = "",
  onEnd,
}: {
  count: number;
  rowHeight: number;
  renderRow: (index: number) => ReactNode;
  header?: ReactNode;
  className?: string;
  onEnd?: () => void;
}) {
  const viewport = useRef<HTMLDivElement>(null);
  const [position, setPosition] = useState({ top: 0, height: 600 });
  useEffect(() => {
    const el = viewport.current;
    if (!el) return;
    const observer = new ResizeObserver(() =>
      setPosition((p) => ({ ...p, height: el.clientHeight })),
    );
    observer.observe(el);
    return () => observer.disconnect();
  }, []);
  const start = Math.max(0, Math.floor(position.top / rowHeight) - 5);
  const end = Math.min(
    count,
    Math.ceil((position.top + position.height) / rowHeight) + 5,
  );
  useEffect(() => {
    if (onEnd && count > 0 && end >= count) onEnd();
  }, [end, count, onEnd]);
  return (
    <div
      ref={viewport}
      className={`virtual-list ${className}`}
      onScroll={(e) => {
        const top = e.currentTarget.scrollTop;
        setPosition((p) => ({ ...p, top }));
      }}
    >
      {header}
      <div style={{ height: count * rowHeight, position: "relative" }}>
        <div
          style={{
            position: "absolute",
            top: start * rowHeight,
            left: 0,
            right: 0,
          }}
        >
          {Array.from({ length: Math.max(0, end - start) }, (_, i) => (
            <div key={start + i} style={{ height: rowHeight }}>
              {renderRow(start + i)}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
export function ProgressBar({
  value,
  label,
}: {
  value: number;
  label?: string;
}) {
  const percent = Math.max(0, Math.min(100, value));
  return (
    <div
      className="progress-track"
      role="progressbar"
      aria-label={label || "Прогресс"}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(percent)}
    >
      <div style={{ width: `${percent}%` }} />
    </div>
  );
}
