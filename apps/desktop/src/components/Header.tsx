import { RefreshCw, Loader2, Moon, Sun } from "lucide-react";
import type { Task } from "../api/types";
import { SearchField } from "./Primitives";

interface HeaderProps {
  title: string;
  subtitle: string;
  searchQuery: string;
  onSearchChange: (q: string) => void;
  onScanAll: () => void;
  activeTasks: Task[];
  isScanning: boolean;
  page: string;
  searching?: boolean;
  currentTheme?: "dark" | "light";
  onToggleTheme?: () => void;
}

export function Header({
  title,
  subtitle,
  searchQuery,
  onSearchChange,
  onScanAll,
  activeTasks,
  isScanning,
  page,
  searching,
  currentTheme = "dark",
  onToggleTheme,
}: HeaderProps) {
  const running = activeTasks.filter(
    (t) => t.state === "running" || t.state === "queued",
  );
  return (
    <header className="page-header">
      <div className="page-heading">
        <h1>{title}</h1>
        <p title={subtitle}>{subtitle}</p>
      </div>
      <div className="header-search">
        <SearchField
          value={searchQuery}
          onChange={onSearchChange}
          loading={searching}
          shortcut
          placeholder="Поиск в фонотеке"
        />
      </div>

      {onToggleTheme && (
        <button
          className="btn btn-secondary"
          style={{
            height: 34,
            width: 34,
            padding: 0,
            borderRadius: 8,
            display: "inline-flex",
            alignItems: "center",
            justifyContent: "center",
            flexShrink: 0,
          }}
          onClick={onToggleTheme}
          title={
            currentTheme === "dark"
              ? "Переключить на светлую тему"
              : "Переключить на тёмную тему"
          }
          aria-label="Сменить тему оформления"
        >
          {currentTheme === "dark" ? (
            <Sun size={16} color="var(--accent)" />
          ) : (
            <Moon size={16} color="var(--accent)" />
          )}
        </button>
      )}

      {(page === "library" || page === "storages") && (
        <button
          className="btn btn-primary"
          disabled={
            isScanning ||
            activeTasks.some(
              (t) =>
                t.kind === "scan" &&
                (t.state === "running" || t.state === "queued"),
            )
          }
          onClick={onScanAll}
        >
          {isScanning ? (
            <Loader2 size={14} className="spin" />
          ) : (
            <RefreshCw size={14} />
          )}
          <span>{isScanning ? "Запуск…" : "Сканировать"}</span>
        </button>
      )}
      {running.length > 0 && (
        <span
          className="header-activity"
          title={`${running.length} фоновых задач`}
        >
          <Loader2 size={14} className="spin" />
        </span>
      )}
    </header>
  );
}
