import React, { useEffect, useState, useMemo } from "react";
import { History, Trash2, Clock, CheckCircle2, HardDrive, AlertCircle, X, Search, RefreshCw } from "lucide-react";
import { api, formatBytes } from "../api/client";
import type { DeletionHistoryEntry, PendingDeletion } from "../api/types";

interface Props {
  isOpen: boolean;
  onClose: () => void;
}

export const DeletionHistoryModal: React.FC<Props> = ({ isOpen, onClose }) => {
  const [tab, setTab] = useState<"history" | "pending">("history");
  const [history, setHistory] = useState<DeletionHistoryEntry[]>([]);
  const [pending, setPending] = useState<PendingDeletion[]>([]);
  const [loading, setLoading] = useState(false);
  const [query, setQuery] = useState("");
  const [clearing, setClearing] = useState(false);

  const loadData = async () => {
    setLoading(true);
    try {
      const [h, p] = await Promise.all([
        api.getDeletionHistory(200, 0),
        api.getPendingDeletions(),
      ]);
      setHistory(h);
      setPending(p);
    } catch (e) {
      console.error("Failed to load deletion history/pending:", e);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (isOpen) {
      loadData();
    }
  }, [isOpen]);

  const handleClearHistory = async () => {
    if (!window.confirm("Очистить журнал истории удалений? Сами файлы уже удалены, очистится только запись в журнале.")) return;
    setClearing(true);
    try {
      await api.clearDeletionHistory();
      setHistory([]);
    } catch (e) {
      console.error("clearDeletionHistory error:", e);
    } finally {
      setClearing(false);
    }
  };

  const filteredHistory = useMemo(() => {
    if (!query.trim()) return history;
    const q = query.toLowerCase();
    return history.filter(
      (h) =>
        h.title.toLowerCase().includes(q) ||
        h.authors.toLowerCase().includes(q) ||
        h.volume_name.toLowerCase().includes(q) ||
        h.rel_path.toLowerCase().includes(q)
    );
  }, [history, query]);

  const filteredPending = useMemo(() => {
    if (!query.trim()) return pending;
    const q = query.toLowerCase();
    return pending.filter(
      (p) =>
        p.title.toLowerCase().includes(q) ||
        p.authors.toLowerCase().includes(q) ||
        p.volume_name.toLowerCase().includes(q) ||
        p.rel_path.toLowerCase().includes(q)
    );
  }, [pending, query]);

  if (!isOpen) return null;

  return (
    <div
      className="modal-backdrop"
      onClick={onClose}
      role="dialog"
      aria-modal="true"
      aria-labelledby="history-modal-title"
    >
      <div
        className="modal"
        style={{ maxWidth: 840, width: "100%", maxHeight: "88vh", display: "flex", flexDirection: "column" }}
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
                background: "rgba(120, 119, 198, 0.15)",
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                color: "var(--accent-hover)",
              }}
            >
              <History size={20} />
            </div>
            <div>
              <h3 id="history-modal-title" style={{ margin: 0, fontSize: 17, fontWeight: 700 }}>
                Архив и история удалений
              </h3>
              <span style={{ fontSize: 12.5, color: "var(--text-muted)" }}>
                Журнал всех удалённых книг и очередь отложенных задач для внешних накопителей
              </span>
            </div>
          </div>
          <button className="plain-icon" onClick={onClose} aria-label="Закрыть">
            <X size={18} />
          </button>
        </div>

        {/* Subheader Toolbar */}
        <div
          style={{
            padding: "12px 22px",
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            gap: 12,
            borderBottom: "1px solid var(--border-subtle)",
            background: "var(--bg-surface)",
          }}
        >
          <div className="segmented" style={{ display: "inline-flex" }}>
            <button
              className={tab === "history" ? "is-active" : ""}
              onClick={() => setTab("history")}
            >
              Журнал удалённых ({history.length})
            </button>
            <button
              className={tab === "pending" ? "is-active" : ""}
              onClick={() => setTab("pending")}
            >
              Очередь офлайн ({pending.length})
            </button>
          </div>

          <div style={{ display: "flex", alignItems: "center", gap: 8, flex: 1, maxWidth: 300 }}>
            <div
              style={{
                position: "relative",
                width: "100%",
                display: "flex",
                alignItems: "center",
              }}
            >
              <Search
                size={14}
                style={{ position: "absolute", left: 10, color: "var(--text-muted)" }}
              />
              <input
                type="text"
                placeholder="Поиск по архиву…"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                style={{
                  width: "100%",
                  padding: "6px 10px 6px 30px",
                  borderRadius: 6,
                  border: "1px solid var(--border-card)",
                  background: "var(--bg-card)",
                  color: "var(--text-primary)",
                  fontSize: 12.5,
                }}
              />
              {query && (
                <button
                  className="plain-icon"
                  onClick={() => setQuery("")}
                  style={{ position: "absolute", right: 6 }}
                >
                  <X size={13} />
                </button>
              )}
            </div>
          </div>

          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <button
              className="btn btn-quiet"
              onClick={loadData}
              disabled={loading}
              title="Обновить журнал"
            >
              <RefreshCw size={13} className={loading ? "spin" : ""} />
            </button>
            {tab === "history" && history.length > 0 && (
              <button
                className="btn btn-quiet"
                onClick={handleClearHistory}
                disabled={clearing}
                style={{ fontSize: 12, color: "#f87171" }}
              >
                <Trash2 size={13} />
                Очистить журнал
              </button>
            )}
          </div>
        </div>

        {/* Content list */}
        <div style={{ flex: 1, overflowY: "auto", padding: "16px 22px" }}>
          {tab === "history" ? (
            filteredHistory.length === 0 ? (
              <div style={{ textAlign: "center", padding: "40px 20px", color: "var(--text-muted)" }}>
                {query ? "Ничего не найдено по вашему запросу." : "Журнал удалений пуст. Здесь будет сохраняться история удалённых книг."}
              </div>
            ) : (
              <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
                {filteredHistory.map((item) => {
                  const dateStr = item.deleted_at
                    ? new Date(item.deleted_at * 1000).toLocaleString("ru-RU", {
                        day: "numeric",
                        month: "short",
                        year: "numeric",
                        hour: "2-digit",
                        minute: "2-digit",
                      })
                    : "—";
                  const isPending = item.status === "pending_volume";
                  return (
                    <div
                      key={item.id}
                      style={{
                        padding: "10px 14px",
                        background: "var(--bg-surface)",
                        border: "1px solid var(--border-subtle)",
                        borderRadius: 8,
                        display: "flex",
                        alignItems: "center",
                        justifyContent: "space-between",
                        gap: 12,
                      }}
                    >
                      <div style={{ flex: 1, minWidth: 0 }}>
                        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                          <span
                            className="badge badge-format"
                            style={{ fontSize: 10, padding: "1px 6px" }}
                          >
                            {item.item_type === "ebook" ? "КНИГА" : "АУДИО"} · {item.format.toUpperCase()}
                          </span>
                          <strong
                            style={{
                              fontSize: 13.5,
                              color: "var(--text-primary)",
                              whiteSpace: "nowrap",
                              overflow: "hidden",
                              textOverflow: "ellipsis",
                            }}
                            title={item.title}
                          >
                            {item.title}
                          </strong>
                        </div>
                        <div
                          style={{
                            fontSize: 12,
                            color: "var(--text-secondary)",
                            marginTop: 3,
                            display: "flex",
                            gap: 12,
                            flexWrap: "wrap",
                          }}
                        >
                          <span>{item.authors || "Автор не указан"}</span>
                          {item.total_size > 0 && <span>· {formatBytes(item.total_size)}</span>}
                          <span>· Диск: {item.volume_name}</span>
                          <span style={{ color: "var(--text-muted)" }}>· {dateStr}</span>
                        </div>
                        <div
                          style={{
                            fontSize: 11,
                            color: "var(--text-muted)",
                            fontFamily: "monospace",
                            marginTop: 3,
                            whiteSpace: "nowrap",
                            overflow: "hidden",
                            textOverflow: "ellipsis",
                          }}
                          title={item.rel_path}
                        >
                          {item.rel_path}
                        </div>
                      </div>

                      <div style={{ flexShrink: 0, textAlign: "right" }}>
                        {isPending ? (
                          <span
                            style={{
                              display: "inline-flex",
                              alignItems: "center",
                              gap: 5,
                              fontSize: 11.5,
                              color: "#f59e0b",
                              background: "rgba(245, 158, 11, 0.12)",
                              border: "1px solid rgba(245, 158, 11, 0.3)",
                              padding: "3px 8px",
                              borderRadius: 4,
                            }}
                          >
                            <Clock size={12} />
                            Ожидает диск
                          </span>
                        ) : (
                          <span
                            style={{
                              display: "inline-flex",
                              alignItems: "center",
                              gap: 5,
                              fontSize: 11.5,
                              color: "#86efac",
                              background: "rgba(34, 197, 94, 0.12)",
                              border: "1px solid rgba(34, 197, 94, 0.3)",
                              padding: "3px 8px",
                              borderRadius: 4,
                            }}
                          >
                            <CheckCircle2 size={12} />
                            Удалено с диска
                          </span>
                        )}
                      </div>
                    </div>
                  );
                })}
              </div>
            )
          ) : (
            filteredPending.length === 0 ? (
              <div style={{ textAlign: "center", padding: "40px 20px", color: "var(--text-muted)" }}>
                {query ? "Ничего не найдено по вашему запросу." : "Нет отложенных задач удаления. Все накопители синхронизированы."}
              </div>
            ) : (
              <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
                <div
                  style={{
                    padding: "10px 14px",
                    background: "rgba(59, 130, 246, 0.1)",
                    border: "1px solid rgba(59, 130, 246, 0.25)",
                    borderRadius: 8,
                    fontSize: 12.5,
                    color: "var(--text-secondary)",
                    display: "flex",
                    alignItems: "center",
                    gap: 8,
                  }}
                >
                  <AlertCircle size={16} color="#60a5fa" />
                  <span>
                    Эти задачи выполнятся автоматически в фоновом режиме, как только соответствующий диск будет подключён к компьютеру.
                  </span>
                </div>
                {filteredPending.map((p) => (
                  <div
                    key={p.id}
                    style={{
                      padding: "10px 14px",
                      background: "var(--bg-surface)",
                      border: "1px solid var(--border-subtle)",
                      borderRadius: 8,
                      display: "flex",
                      alignItems: "center",
                      justifyContent: "space-between",
                      gap: 12,
                    }}
                  >
                    <div style={{ flex: 1, minWidth: 0 }}>
                      <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                        <span
                          className="badge badge-format"
                          style={{ fontSize: 10, padding: "1px 6px" }}
                        >
                          {p.item_type === "ebook" ? "ЭЛ. КНИГА" : "АУДИОКНИГА"}
                        </span>
                        <strong
                          style={{
                            fontSize: 13.5,
                            color: "var(--text-primary)",
                            whiteSpace: "nowrap",
                            overflow: "hidden",
                            textOverflow: "ellipsis",
                          }}
                          title={p.title}
                        >
                          {p.title}
                        </strong>
                      </div>
                      <div
                        style={{
                          fontSize: 12,
                          color: "var(--text-secondary)",
                          marginTop: 3,
                          display: "flex",
                          gap: 10,
                        }}
                      >
                        <span>{p.authors || "Автор не указан"}</span>
                        {p.size > 0 && <span>· {formatBytes(p.size)}</span>}
                        <span>· Накопитель: {p.volume_name}</span>
                      </div>
                      <div
                        style={{
                          fontSize: 11,
                          color: "var(--text-muted)",
                          fontFamily: "monospace",
                          marginTop: 3,
                          whiteSpace: "nowrap",
                          overflow: "hidden",
                          textOverflow: "ellipsis",
                        }}
                        title={p.rel_path}
                      >
                        {p.rel_path}
                      </div>
                    </div>

                    <div style={{ flexShrink: 0 }}>
                      <span
                        style={{
                          display: "inline-flex",
                          alignItems: "center",
                          gap: 5,
                          fontSize: 11.5,
                          color: p.volume_online ? "#86efac" : "#fca5a5",
                          background: p.volume_online ? "rgba(34, 197, 94, 0.12)" : "rgba(239, 68, 68, 0.12)",
                          border: `1px solid ${p.volume_online ? "rgba(34, 197, 94, 0.3)" : "rgba(239, 68, 68, 0.3)"}`,
                          padding: "3px 8px",
                          borderRadius: 4,
                        }}
                      >
                        <HardDrive size={12} />
                        {p.volume_online ? "В сети (выполняется)" : "Диск офлайн"}
                      </span>
                    </div>
                  </div>
                ))}
              </div>
            )
          )}
        </div>

        <div
          style={{
            padding: "14px 22px",
            borderTop: "1px solid var(--border-subtle)",
            display: "flex",
            justifyContent: "space-between",
            alignItems: "center",
          }}
        >
          <span style={{ fontSize: 12, color: "var(--text-muted)" }}>
            Всего в архиве: {history.length} · Ожидает удаления: {pending.length}
          </span>
          <button className="btn btn-secondary" onClick={onClose}>
            Закрыть
          </button>
        </div>
      </div>
    </div>
  );
};
