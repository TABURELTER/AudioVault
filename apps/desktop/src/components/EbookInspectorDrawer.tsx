import { useState } from "react";
import { X, ExternalLink, FolderOpen, Copy, BookOpen, CheckCircle2, Trash2 } from "lucide-react";
import type { EbookDetail } from "../api/types";
import { api, formatBytes } from "../api/client";
import {
  BookCover,
  StatusIndicator,
  LoadingRows,
  getFormatClass,
} from "./Primitives";

interface Props {
  ebook: EbookDetail | null;
  isOpen: boolean;
  onClose: () => void;
  onRevealFolder: (path: string) => void;
  onOpenFile: (path: string) => void;
}

export function EbookInspectorDrawer({
  ebook,
  isOpen,
  onClose,
  onRevealFolder,
  onOpenFile,
}: Props) {
  const [copyMessage, setCopyMessage] = useState("");
  if (!isOpen) return null;
  const path = ebook?.full_path || ebook?.rel_path || "";

  return (
    <aside className="detail-panel" aria-label="Сведения об электронной книге">
      <div className="detail-topline">
        <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
          {ebook && (
            <span
              className={`badge badge-format ${getFormatClass(ebook.format)}`}
            >
              {ebook.format.toUpperCase()}
            </span>
          )}
          <span style={{ fontWeight: 700, fontSize: 13, color: "var(--text-primary)" }}>
            Карточка книги
          </span>
        </div>
        <button
          className="action-btn"
          onClick={onClose}
          aria-label="Закрыть карточку"
          title="Закрыть · Esc"
        >
          <X size={17} />
        </button>
      </div>

      {!ebook ? (
        <LoadingRows />
      ) : (
        <>
          <div className="detail-summary" style={{ gap: 16, padding: "20px 20px 16px" }}>
            <div style={{ width: 92, height: 132, flexShrink: 0 }}>
              <BookCover
                title={ebook.title || ebook.file_name}
                thumb={ebook.cover_thumb}
                format={ebook.format}
                author={ebook.authors}
                large
              />
            </div>
            <div style={{ flex: 1, minWidth: 0 }}>
              <h2
                style={{
                  color: "#ffffff",
                  fontSize: 18,
                  fontWeight: 700,
                  lineHeight: 1.35,
                }}
              >
                {ebook.title || ebook.file_name}
              </h2>
              <p
                style={{
                  color: "var(--accent-hover)",
                  fontWeight: 650,
                  fontSize: 14.5,
                  marginTop: 6,
                }}
              >
                {ebook.authors || "Автор не указан"}
              </p>
              {ebook.series && (
                <div style={{ marginTop: 8 }}>
                  <span
                    style={{
                      color: "var(--accent-hover)",
                      fontWeight: 700,
                      fontSize: 12,
                      display: "inline-block",
                      background: "rgba(245, 158, 11, 0.16)",
                      border: "1px solid rgba(245, 158, 11, 0.35)",
                      padding: "2px 8px",
                      borderRadius: 4,
                    }}
                  >
                    {ebook.series}
                    {ebook.series_index != null && ` #${ebook.series_index}`}
                  </span>
                </div>
              )}
            </div>
          </div>

          <div
            className="detail-primary"
            style={{ display: "flex", gap: 10, padding: "0 20px 18px" }}
          >
            <button
              className="btn btn-primary"
              style={{
                flex: 1.2,
                height: 44,
                fontSize: 14,
                fontWeight: 750,
                background: "var(--accent-gradient)",
                color: "var(--text-on-accent)",
                border: "none",
                borderRadius: "8px",
                boxShadow: "0 2px 10px rgba(245, 158, 11, 0.35)",
                display: "inline-flex",
                alignItems: "center",
                justifyContent: "center",
                gap: 8,
              }}
              disabled={!ebook.volume_online || !ebook.full_path}
              onClick={() => onOpenFile(path)}
            >
              <ExternalLink size={17} />
              Открыть в читалке
            </button>
            <button
              className="btn btn-secondary"
              style={{
                flex: 1,
                height: 44,
                fontSize: 13.5,
                fontWeight: 650,
                borderRadius: "8px",
                border: "1.5px solid var(--border-card)",
                background: "var(--bg-surface)",
                color: "var(--text-primary)",
                display: "inline-flex",
                alignItems: "center",
                justifyContent: "center",
                gap: 8,
              }}
              disabled={!ebook.volume_online || !ebook.full_path}
              onClick={() => onRevealFolder(path)}
            >
              <FolderOpen size={17} />
              Показать файл
            </button>
          </div>

          <div
            style={{
              display: "flex",
              gap: 8,
              padding: "0 20px 14px",
            }}
          >
            <button
              className={`btn btn-secondary ${ebook.is_read ? "is-active" : ""}`}
              style={{
                flex: 1,
                height: 36,
                fontSize: 12.5,
                gap: 6,
                color: ebook.is_read ? "#86efac" : undefined,
              }}
              onClick={async () => {
                const next = !ebook.is_read;
                await api.setEbookRead(ebook.id, next);
                ebook.is_read = next;
                setCopyMessage(next ? "Отмечено как прочитано" : "Метка «прочитано» снята");
                setTimeout(() => setCopyMessage(""), 2000);
              }}
            >
              <CheckCircle2 size={15} />
              <span>{ebook.is_read ? "Прочитана" : "Отметить прочитанной"}</span>
            </button>

            <button
              className={`btn btn-secondary ${ebook.is_marked_for_deletion ? "is-active" : ""}`}
              style={{
                flex: 1,
                height: 36,
                fontSize: 12.5,
                gap: 6,
                color: ebook.is_marked_for_deletion ? "#f87171" : undefined,
              }}
              onClick={async () => {
                const next = !ebook.is_marked_for_deletion;
                await api.setEbookMarkedForDeletion(ebook.id, next);
                ebook.is_marked_for_deletion = next;
                setCopyMessage(next ? "Помечено как кандидат на удаление" : "Снято с удаления");
                setTimeout(() => setCopyMessage(""), 2000);
              }}
            >
              <Trash2 size={15} />
              <span>{ebook.is_marked_for_deletion ? "В кандидатах" : "На удаление"}</span>
            </button>
          </div>

          {copyMessage && (
            <div style={{ padding: "0 20px 10px", fontSize: 12, color: "var(--accent-hover)", textAlign: "center" }}>
              {copyMessage}
            </div>
          )}

          <div className="detail-content">
            <section className="copy-section">
              <div
                style={{
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "space-between",
                  marginBottom: 14,
                  paddingBottom: 10,
                  borderBottom: "1px solid var(--border-subtle)",
                }}
              >
                <StatusIndicator
                  online={ebook.volume_online}
                  label={ebook.volume_name}
                />
                <span
                  style={{
                    fontSize: 12,
                    fontWeight: 700,
                    color: ebook.volume_online ? "#86efac" : "#fca5a5",
                  }}
                >
                  {ebook.volume_online ? "Диск подключён" : "Диск отключён"}
                </span>
              </div>

              <dl className="detail-specs">
                <div>
                  <dt style={{ color: "var(--text-muted)", fontSize: 12, fontWeight: 600 }}>Формат файла</dt>
                  <dd style={{ marginTop: 2 }}>
                    <span
                      className={`badge badge-format ${getFormatClass(ebook.format)}`}
                    >
                      {ebook.format.toUpperCase()}
                    </span>
                  </dd>
                </div>
                <div>
                  <dt style={{ color: "var(--text-muted)", fontSize: 12, fontWeight: 600 }}>Размер файла</dt>
                  <dd style={{ fontWeight: 700, fontSize: 14, color: "#ffffff", marginTop: 2 }}>
                    {formatBytes(ebook.size)}
                  </dd>
                </div>
                <div>
                  <dt style={{ color: "var(--text-muted)", fontSize: 12, fontWeight: 600 }}>Год издания</dt>
                  <dd style={{ fontWeight: 650, fontSize: 13.5, color: "var(--text-secondary)", marginTop: 2 }}>
                    {ebook.year ? `${ebook.year} г.` : "Не указан"}
                  </dd>
                </div>
                <div>
                  <dt style={{ color: "var(--text-muted)", fontSize: 12, fontWeight: 600 }}>Язык</dt>
                  <dd style={{ fontWeight: 650, fontSize: 13.5, color: "var(--text-secondary)", marginTop: 2 }}>
                    {ebook.language ? ebook.language.toUpperCase() : "Не указан"}
                  </dd>
                </div>
                <div>
                  <dt style={{ color: "var(--text-muted)", fontSize: 12, fontWeight: 600 }}>Жанры</dt>
                  <dd style={{ fontWeight: 600, fontSize: 13, color: "var(--text-secondary)", marginTop: 2 }}>
                    {ebook.genres || "Не указаны"}
                  </dd>
                </div>
                <div>
                  <dt style={{ color: "var(--text-muted)", fontSize: 12, fontWeight: 600 }}>Статус в каталоге</dt>
                  <dd style={{ color: "var(--success)", fontWeight: 700, fontSize: 13.5, marginTop: 2 }}>
                    {ebook.state === "present" ? "В наличии" : ebook.state}
                  </dd>
                </div>
              </dl>

              <div
                className="detail-path"
                style={{
                  marginTop: 14,
                  background: "var(--bg-app)",
                  border: "1.5px solid var(--border-card)",
                  padding: 10,
                  borderRadius: 8,
                }}
              >
                <code style={{ fontSize: 12, color: "var(--text-secondary)" }}>{path}</code>
                <button
                  className="action-btn"
                  style={{ width: 32, height: 32 }}
                  aria-label="Скопировать путь"
                  title="Скопировать путь"
                  onClick={async () => {
                    try {
                      await navigator.clipboard.writeText(path);
                      setCopyMessage("Путь скопирован в буфер обмена");
                    } catch {
                      setCopyMessage("Не удалось скопировать путь.");
                    }
                  }}
                >
                  <Copy size={15} />
                </button>
              </div>
              {copyMessage && (
                <small
                  style={{
                    color: "var(--accent-hover)",
                    fontWeight: 700,
                    display: "block",
                    marginTop: 6,
                    fontSize: 12,
                  }}
                >
                  {copyMessage}
                </small>
              )}
              {!ebook.volume_online && (
                <p
                  style={{
                    marginTop: 12,
                    fontSize: 13,
                    color: "var(--danger)",
                    fontWeight: 650,
                  }}
                >
                  Подключите внешний накопитель {ebook.volume_name}, чтобы открыть книгу.
                </p>
              )}
            </section>

            <h3
              style={{
                paddingTop: 18,
                fontSize: 15,
                fontWeight: 750,
                color: "#ffffff",
                letterSpacing: "0.02em",
                display: "flex",
                alignItems: "center",
                gap: 8,
              }}
            >
              <BookOpen size={16} color="var(--accent-hover)" />
              Аннотация к изданию
            </h3>
            <p
              className="description"
              style={{
                fontSize: 14,
                lineHeight: 1.8,
                color: "var(--text-secondary)",
                marginTop: 8,
              }}
            >
              {ebook.description ||
                "Аннотация к данному изданию не найдена в метаданных файла."}
            </p>
          </div>
        </>
      )}
    </aside>
  );
}
