import { useCallback, useState } from "react";
import {
  HardDrive,
  FolderPlus,
  RefreshCw,
  FolderOpen,
  BookOpen,
  Edit2,
  Check,
  X,
} from "lucide-react";
import type { Volume, Task } from "../api/types";
import { api, formatBytes } from "../api/client";
import {
  EmptyState,
  StatusIndicator,
  ProgressBar,
} from "../components/Primitives";
import { ContextMenu } from "../components/ContextMenu";

interface Props {
  volumes: Volume[];
  tasks?: Task[];
  onRefresh: () => void;
  onAddFolder: () => void;
  onScanVolume: (id: number) => void;
  onRevealFolder?: (path: string) => void;
  onOpenVolumeBooks?: (id: number) => void;
  onRenameVolume?: (id: number, newName: string) => Promise<void>;
}

function getDriveLetter(mountPath: string): string {
  const match = mountPath.match(/^[A-Za-z]:/);
  if (match) return match[0].toUpperCase();
  const first = mountPath.split(/[\\/]/)[0];
  return first && first.includes(":") ? first.toUpperCase() : first;
}

export function StoragesPage({
  volumes,
  tasks = [],
  onRefresh,
  onAddFolder,
  onScanVolume,
  onRevealFolder,
  onOpenVolumeBooks,
  onRenameVolume,
}: Props) {
  const [menu, setMenu] = useState<{
    volume: Volume;
    x: number;
    y: number;
  } | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const [editingId, setEditingId] = useState<number | null>(null);
  const [editingName, setEditingName] = useState("");
  const [savingRename, setSavingRename] = useState(false);

  const startRename = (v: Volume) => {
    setEditingId(v.id);
    setEditingName(v.label);
  };

  const handleSaveRename = async (volumeId: number) => {
    if (!editingName.trim()) return;
    setSavingRename(true);
    try {
      if (onRenameVolume) {
        await onRenameVolume(volumeId, editingName.trim());
      } else {
        await api.renameVolume(volumeId, editingName.trim());
        onRefresh();
      }
      setEditingId(null);
    } catch (e) {
      console.error("Failed to rename volume:", e);
    } finally {
      setSavingRename(false);
    }
  };

  const closeMenu = useCallback(() => setMenu(null), []);
  return (
    <div>
      <div className="page-intro">
        <p>
          <strong>
            {volumes.filter((v) => v.online).length} накопителей подключено
          </strong>
          <br />
          Каталог отключённых дисков остаётся доступен для поиска.
        </p>
        <div className="storage-actions">
          <button
            className="btn btn-quiet"
            disabled={refreshing}
            onClick={async () => {
              setRefreshing(true);
              try {
                await onRefresh();
              } finally {
                setRefreshing(false);
              }
            }}
          >
            <RefreshCw size={14} className={refreshing ? "spin" : ""} />
            Обновить
          </button>
          <button className="btn btn-secondary" onClick={onAddFolder}>
            <FolderPlus size={14} />
            Добавить папку
          </button>
        </div>
      </div>
      {!volumes.length && (
        <EmptyState
          title="Подключите вашу коллекцию"
          description="Добавьте папку с книгами на компьютере или внешнем накопителе."
          action={
            <button className="btn btn-primary" onClick={onAddFolder}>
              Добавить папку
            </button>
          }
        />
      )}
      {volumes.map((v) => {
        const used = Math.max(
          0,
          Math.min(
            100,
            v.capacity_bytes ? (1 - v.free_bytes / v.capacity_bytes) * 100 : 0,
          ),
        );
        const scanning = tasks.some(
          (t) =>
            (t.state === "running" || t.state === "queued") &&
            t.kind.includes("scan") &&
            (t.description.includes(v.mount_path) || t.label.includes(v.label)),
        );
        const driveLetter = getDriveLetter(v.mount_path);
        const isEditing = editingId === v.id;

        return (
          <section
            key={v.id}
            className="storage-row"
            onContextMenu={(e) => {
              e.preventDefault();
              setMenu({ volume: v, x: e.clientX, y: e.clientY });
            }}
          >
            <HardDrive size={30} strokeWidth={1.4} />
            <div className="storage-identity">
              <div style={{ display: "flex", alignItems: "center", gap: 10, flexWrap: "wrap", marginBottom: 4 }}>
                {driveLetter && (
                  <span className="drive-letter-badge" title={`Буква диска в системе: ${driveLetter}`}>
                    {driveLetter}
                  </span>
                )}
                {isEditing ? (
                  <form
                    onSubmit={(e) => {
                      e.preventDefault();
                      handleSaveRename(v.id);
                    }}
                    style={{ display: "inline-flex", alignItems: "center", gap: 6 }}
                  >
                    <input
                      type="text"
                      className="input-text"
                      value={editingName}
                      onChange={(e) => setEditingName(e.target.value)}
                      onKeyDown={(e) => {
                        if (e.key === "Escape") setEditingId(null);
                      }}
                      autoFocus
                      disabled={savingRename}
                      placeholder="Название накопителя"
                      style={{
                        padding: "4px 10px",
                        fontSize: "15px",
                        fontWeight: 700,
                        width: "220px",
                        height: "34px",
                      }}
                    />
                    <button
                      type="submit"
                      className="btn btn-primary"
                      disabled={savingRename || !editingName.trim()}
                      style={{ padding: "4px 10px", minHeight: "34px" }}
                      title="Сохранить имя"
                    >
                      <Check size={14} />
                      Сохранить
                    </button>
                    <button
                      type="button"
                      className="btn btn-quiet"
                      onClick={() => setEditingId(null)}
                      disabled={savingRename}
                      style={{ padding: "4px 8px", minHeight: "34px" }}
                      title="Отмена (Esc)"
                    >
                      <X size={14} />
                    </button>
                  </form>
                ) : (
                  <div style={{ display: "inline-flex", alignItems: "center", gap: 8 }}>
                    <h3 style={{ fontSize: "17.5px", fontWeight: 700, margin: 0 }}>{v.label}</h3>
                    <button
                      className="plain-icon rename-btn"
                      title="Переименовать накопитель в AudioVault"
                      onClick={() => startRename(v)}
                    >
                      <Edit2 size={13} />
                    </button>
                  </div>
                )}
                <StatusIndicator online={v.online} />
              </div>
              <p style={{ fontSize: "13px", color: "var(--text-secondary)", margin: 0 }}>
                {v.mount_path} ·{" "}
                {v.storage_class === "unknown"
                  ? "Тип не определён"
                  : v.storage_class.toUpperCase()}{" "}
                · {v.fs_type}
              </p>
            </div>
            <div className="storage-capacity">
              <p style={{ fontSize: "13px" }}>
                {v.capacity_bytes ? (
                  <>
                    <strong>{formatBytes(v.free_bytes)}</strong> свободно из{" "}
                    {formatBytes(v.capacity_bytes)}
                  </>
                ) : (
                  "Ёмкость неизвестна"
                )}
              </p>
              <ProgressBar value={used} label={`Занято на ${v.label}`} />
              <p style={{ fontSize: "12.5px" }}>
                {scanning
                  ? "Сканирование…"
                  : `${v.roots_count} папок библиотеки`}
              </p>
            </div>
            <div className="storage-count">
              <strong>{v.book_count.toLocaleString("ru-RU")}</strong>
              <small>аудиокниг</small>
            </div>
            <div className="storage-actions">
              <button
                className="btn btn-quiet"
                title="Переименовать накопитель"
                onClick={() => startRename(v)}
              >
                <Edit2 size={13} />
                Имя
              </button>
              <button
                className="btn btn-quiet"
                title="Книги на накопителе"
                onClick={() => onOpenVolumeBooks?.(v.id)}
              >
                <BookOpen size={14} />
                Книги
              </button>
              <button
                className="plain-icon"
                title="Показать в Проводнике"
                aria-label={`Показать ${v.label} в Проводнике`}
                disabled={!v.online}
                onClick={() => onRevealFolder?.(v.mount_path)}
              >
                <FolderOpen size={15} />
              </button>
              <button
                className="btn btn-secondary"
                disabled={!v.online || scanning}
                onClick={() => onScanVolume(v.id)}
              >
                <RefreshCw size={13} />
                {scanning ? "Сканирование…" : "Сканировать"}
              </button>
            </div>
          </section>
        );
      })}
      {menu && (
        <ContextMenu x={menu.x} y={menu.y} onClose={closeMenu}>
          <p>{menu.volume.label}</p>
          <button
            role="menuitem"
            onClick={() => {
              startRename(menu.volume);
              closeMenu();
            }}
          >
            <Edit2 size={14} />
            Переименовать
          </button>
          <button
            role="menuitem"
            onClick={() => {
              onOpenVolumeBooks?.(menu.volume.id);
              closeMenu();
            }}
          >
            <BookOpen size={14} />
            Открыть книги
          </button>
          <button
            role="menuitem"
            disabled={!menu.volume.online}
            onClick={() => {
              onRevealFolder?.(menu.volume.mount_path);
              closeMenu();
            }}
          >
            <FolderOpen size={14} />
            Показать в Проводнике
          </button>
          <button
            role="menuitem"
            disabled={!menu.volume.online}
            onClick={() => {
              onScanVolume(menu.volume.id);
              closeMenu();
            }}
          >
            <RefreshCw size={14} />
            Сканировать
          </button>
        </ContextMenu>
      )}
    </div>
  );
}
