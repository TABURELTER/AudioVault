import {
  BookOpen,
  Search,
  HardDrive,
  Copy,
  AlertCircle,
  Globe,
  Download,
  Zap,
  ListTodo,
  Settings,
  Headphones,
} from "lucide-react";
import type { Stats, Volume } from "../api/types";
import { StatusIndicator } from "./Primitives";
interface SidebarProps {
  currentPage: string;
  onNavigate: (page: string) => void;
  stats: Stats;
  volumes: Volume[];
}
export function Sidebar({
  currentPage,
  onNavigate,
  stats,
  volumes,
}: SidebarProps) {
  const groups = [
    [
      {
        id: "library",
        label: "Фонотека",
        icon: Headphones,
        count: stats.total_books,
      },
      {
        id: "ebooks",
        label: "Электронные книги",
        icon: BookOpen,
        count: stats.total_ebooks,
      },
      { id: "search", label: "Поиск", icon: Search },
    ],
    [
      {
        id: "storages",
        label: "Хранилища",
        icon: HardDrive,
        count: volumes.length,
      },
      {
        id: "duplicates",
        label: "Дубликаты",
        icon: Copy,
        count: stats.duplicates_count,
      },
      {
        id: "issues",
        label: "Проблемы",
        icon: AlertCircle,
        count: stats.issues_count,
      },
    ],
    [
      { id: "rutracker", label: "RuTracker", icon: Globe },
      { id: "downloads", label: "Загрузки", icon: Download },
      { id: "optimizer", label: "Оптимизация", icon: Zap },
      { id: "tasks", label: "Задачи", icon: ListTodo },
    ],
  ];
  const online = volumes.filter((v) => v.online);
  return (
    <aside className="sidebar">
      <div className="brand">
        <div className="brand-mark">
          <Headphones size={20} strokeWidth={1.6} />
        </div>
        <div>
          <strong>AudioVault</strong>
          <span>Ваша коллекция. Рядом.</span>
        </div>
      </div>
      <nav aria-label="Основная навигация">
        {groups.map((group, index) => (
          <div className="nav-group" key={index}>
            {group.map((item) => (
              <button
                key={item.id}
                title={item.label}
                className={`nav-item ${currentPage === item.id ? "active" : ""}`}
                aria-current={currentPage === item.id ? "page" : undefined}
                onClick={() => onNavigate(item.id)}
              >
                <item.icon size={17} strokeWidth={1.6} />
                <span>{item.label}</span>
                {"count" in item && Number(item.count) > 0 && (
                  <small>{item.count?.toLocaleString("ru-RU")}</small>
                )}
              </button>
            ))}
          </div>
        ))}
      </nav>
      <button
        className={`nav-item settings-link ${currentPage === "settings" ? "active" : ""}`}
        aria-current={currentPage === "settings" ? "page" : undefined}
        onClick={() => onNavigate("settings")}
      >
        <Settings size={17} strokeWidth={1.6} />
        <span>Настройки</span>
      </button>
      <button className="storage-footer" onClick={() => onNavigate("storages")}>
        <StatusIndicator
          online={online.length > 0}
          label={`${online.length} из ${volumes.length} накопителей подключено`}
        />
        <span>
          {volumes.length > 0
            ? volumes
                .map((v) => {
                  const letter = v.mount_path.match(/^[A-Za-z]:/)?.[0] || v.mount_path.replace(/[\\/]/g, "");
                  return `${letter || "Диск"} ${v.label ? `· ${v.label}` : ""}`;
                })
                .join("  |  ")
            : "Добавьте папку коллекции"}
          <HardDrive size={13} />
        </span>
      </button>
    </aside>
  );
}
