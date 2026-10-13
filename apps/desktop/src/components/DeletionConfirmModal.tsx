import React from "react";
import { AlertTriangle, HardDrive, Trash2, X } from "lucide-react";
import { formatBytes } from "../api/client";

export interface ItemToDelete {
  id: number;
  type: "book" | "ebook";
  title: string;
  authors?: string;
  volumeName?: string;
  volumeOnline?: boolean;
  size?: number;
  path?: string;
}

interface Props {
  isOpen: boolean;
  items: ItemToDelete[];
  onConfirm: () => Promise<void> | void;
  onCancel: () => void;
  busy?: boolean;
}

export const DeletionConfirmModal: React.FC<Props> = ({
  isOpen,
  items,
  onConfirm,
  onCancel,
  busy = false,
}) => {
  if (!isOpen || items.length === 0) return null;

  const totalSize = items.reduce((acc, it) => acc + (it.size || 0), 0);
  const onlineCount = items.filter((it) => it.volumeOnline !== false).length;
  const offlineCount = items.length - onlineCount;

  return (
    <div
      className="modal-backdrop"
      onClick={busy ? undefined : onCancel}
      role="dialog"
      aria-modal="true"
      aria-labelledby="deletion-modal-title"
    >
      <div
        className="modal"
        style={{ maxWidth: 560, width: "100%" }}
        onClick={(e) => e.stopPropagation()}
      >
        <div
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            padding: "18px 22px 14px",
            borderBottom: "1px solid var(--border-subtle)",
          }}
        >
          <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
            <div
              style={{
                width: 36,
                height: 36,
                borderRadius: 10,
                background: "rgba(239, 68, 68, 0.15)",
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                color: "#ef4444",
              }}
            >
              <Trash2 size={20} />
            </div>
            <div>
              <h3 id="deletion-modal-title" style={{ margin: 0, fontSize: 17, fontWeight: 700 }}>
                Удаление {items.length} {items.length === 1 ? "книги" : "книг"}
              </h3>
              <span style={{ fontSize: 12.5, color: "var(--text-muted)" }}>
                Освободится ~{formatBytes(totalSize)}
              </span>
            </div>
          </div>
          <button
            className="plain-icon"
            onClick={onCancel}
            disabled={busy}
            aria-label="Закрыть"
          >
            <X size={18} />
          </button>
        </div>

        <div style={{ padding: "18px 22px", display: "flex", flexDirection: "column", gap: 14 }}>
          {offlineCount > 0 ? (
            <div
              style={{
                padding: "12px 14px",
                background: "rgba(245, 158, 11, 0.12)",
                border: "1px solid rgba(245, 158, 11, 0.3)",
                borderRadius: 8,
                fontSize: 13,
                lineHeight: 1.45,
                color: "var(--text-primary)",
                display: "flex",
                gap: 10,
              }}
            >
              <AlertTriangle size={18} color="#f59e0b" style={{ flexShrink: 0, marginTop: 2 }} />
              <div>
                <strong>Внимание: {offlineCount} из {items.length} книг находятся на отключённых дисках.</strong>
                <p style={{ margin: "4px 0 0", color: "var(--text-secondary)", fontSize: 12.5 }}>
                  Файлы на подключённых дисках ({onlineCount}) будут удалены сразу. Для отключённых дисков будет создана отложенная задача — файлы удалятся автоматически, как только вы подключите соответствующий диск.
                </p>
              </div>
            </div>
          ) : (
            <p style={{ margin: 0, fontSize: 13.5, color: "var(--text-secondary)", lineHeight: 1.45 }}>
              Файлы и папки выбранных книг будут безвозвратно удалены с накопителей, а записи удалены из каталога. Запись об удалении сохранится в истории.
            </p>
          )}

          <div
            style={{
              maxHeight: 220,
              overflowY: "auto",
              border: "1px solid var(--border-subtle)",
              borderRadius: 8,
              padding: "6px 10px",
              background: "var(--bg-surface)",
              display: "flex",
              flexDirection: "column",
              gap: 6,
            }}
          >
            {items.map((it) => (
              <div
                key={`${it.type}-${it.id}`}
                style={{
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "space-between",
                  gap: 10,
                  padding: "5px 4px",
                  borderBottom: "1px solid rgba(255,255,255,0.05)",
                  fontSize: 12.5,
                }}
              >
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div
                    style={{
                      fontWeight: 650,
                      color: "var(--text-primary)",
                      whiteSpace: "nowrap",
                      overflow: "hidden",
                      textOverflow: "ellipsis",
                    }}
                    title={it.title}
                  >
                    {it.title}
                  </div>
                  <div
                    style={{
                      color: "var(--text-muted)",
                      fontSize: 11.5,
                      whiteSpace: "nowrap",
                      overflow: "hidden",
                      textOverflow: "ellipsis",
                    }}
                  >
                    {it.authors || "Автор не указан"}
                    {it.size ? ` · ${formatBytes(it.size)}` : ""}
                  </div>
                </div>
                <div
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: 6,
                    fontSize: 11,
                    color: it.volumeOnline !== false ? "#86efac" : "#fca5a5",
                    flexShrink: 0,
                  }}
                >
                  <HardDrive size={13} />
                  <span>{it.volumeName || "Диск"}</span>
                  <small style={{ opacity: 0.8 }}>
                    {it.volumeOnline !== false ? "(онлайн)" : "(офлайн)"}
                  </small>
                </div>
              </div>
            ))}
          </div>
        </div>

        <div
          style={{
            padding: "14px 22px",
            borderTop: "1px solid var(--border-subtle)",
            display: "flex",
            justifyContent: "flex-end",
            gap: 10,
          }}
        >
          <button className="btn btn-secondary" onClick={onCancel} disabled={busy}>
            Отмена
          </button>
          <button className="btn btn-danger" onClick={onConfirm} disabled={busy}>
            {busy ? "Удаление…" : `Удалить ${items.length} ${items.length === 1 ? "книгу" : "книг"}`}
          </button>
        </div>
      </div>
    </div>
  );
};
