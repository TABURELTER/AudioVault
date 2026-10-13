import { useState } from "react";
import { X, Play, FolderOpen, Copy, Clock } from "lucide-react";
import type { BookDetail } from "../api/types";
import { formatBytes, formatDuration } from "../api/client";
import {
  BookCover,
  StatusIndicator,
  LoadingRows,
  VirtualList,
} from "./Primitives";
interface DrawerProps {
  book: BookDetail | null;
  isOpen: boolean;
  onClose: () => void;
  onOptimizeCopy: (id: number, format?: string) => void;
  onRevealFolder: (path: string) => void;
  onPlayTrack?: (name: string, path: string) => void;
}
export function BookInspectorDrawer({
  book,
  isOpen,
  onClose,
  onOptimizeCopy,
  onRevealFolder,
  onPlayTrack,
}: DrawerProps) {
  const [tab, setTab] = useState("copies");
  const [copied, setCopied] = useState("");
  if (!isOpen) return null;
  const first = book?.copies.find((c) => c.volume_online && c.full_path);
  const playable =
    book?.copies[0]?.volume_online && book?.media.find((m) => m.full_path);
  return (
    <aside className="detail-panel" aria-label="Сведения о книге">
      <div className="detail-topline">
        <span>Сведения о книге</span>
        <button
          className="plain-icon"
          onClick={onClose}
          aria-label="Закрыть сведения"
          title="Закрыть · Esc"
        >
          <X size={16} />
        </button>
      </div>
      {!book ? (
        <LoadingRows />
      ) : (
        <>
          <div className="detail-summary">
            <BookCover title={book.title} thumb={book.cover_thumb} large />
            <div>
              <h2>{book.title || "Без названия"}</h2>
              <p>{book.authors || "Автор не указан"}</p>
              {book.series && (
                <small>
                  {book.series}
                  {book.series_index != null && ` · ${book.series_index}`}
                </small>
              )}
            </div>
          </div>
          <div className="detail-facts">
            <span>{book.narrators || "Чтец не указан"}</span>
            <span>
              <Clock size={12} />{" "}
              {book.duration_ms
                ? formatDuration(book.duration_ms)
                : "Время не указано"}
            </span>
          </div>
          <div className="detail-primary">
            <button
              className="btn btn-primary"
              disabled={!playable}
              onClick={() =>
                playable &&
                onPlayTrack?.(playable.file_name, playable.full_path!)
              }
            >
              <Play size={13} />
              Слушать
            </button>
            <button
              className="btn btn-secondary"
              disabled={!first}
              onClick={() => first && onRevealFolder(first.full_path!)}
            >
              <FolderOpen size={13} />
              Папка
            </button>
          </div>
          <div
            className="detail-tabs"
            role="tablist"
            aria-label="Разделы книги"
          >
            {[
              ["copies", `Копии · ${book.copies.length}`],
              ["tracks", `Файлы · ${book.media.length}`],
              ["desc", "Аннотация"],
            ].map(([id, label]) => (
              <button
                role="tab"
                id={`book-tab-${id}`}
                aria-controls="book-tab-content"
                aria-selected={tab === id}
                key={id}
                onClick={() => setTab(id)}
              >
                {label}
              </button>
            ))}
          </div>
          <div
            className="detail-content"
            id="book-tab-content"
            role="tabpanel"
            aria-labelledby={`book-tab-${tab}`}
            style={
              tab === "tracks"
                ? { display: "flex", flexDirection: "column" }
                : undefined
            }
          >
            {tab === "copies" &&
              book.copies.map((c) => (
                <section className="copy-section" key={c.id}>
                  <h3>
                    <StatusIndicator
                      online={c.volume_online}
                      label={c.volume_name}
                    />
                    <span className="badge badge-format">{c.format}</span>
                  </h3>
                  {!c.volume_online && (
                    <p className="muted" style={{ fontSize: 11 }}>
                      Подключите накопитель, чтобы слушать или конвертировать
                      эту копию.
                    </p>
                  )}
                  <dl className="detail-specs">
                    <div>
                      <dt>Кодек / битрейт</dt>
                      <dd>
                        {c.codec || "—"}
                        {c.bitrate_kbps ? ` · ${c.bitrate_kbps} кбит/с` : ""}
                      </dd>
                    </div>
                    <div>
                      <dt>Размер</dt>
                      <dd>{formatBytes(c.total_size)}</dd>
                    </div>
                    <div>
                      <dt>Файлы</dt>
                      <dd>
                        {c.audio_file_count} ·{" "}
                        {c.is_single_file ? "Один файл" : "По главам"}
                      </dd>
                    </div>
                    <div>
                      <dt>Оценка качества</dt>
                      <dd>{c.quality_score}/100</dd>
                    </div>
                  </dl>
                  {c.folder_name && (
                    <div style={{ marginBottom: "6px", fontSize: "14px", fontWeight: 600, color: "var(--accent, #6366f1)" }}>
                      📁 {c.folder_name}
                    </div>
                  )}
                  <div className="detail-path">
                    <code>{c.full_path || c.rel_path}</code>
                    <button
                      className="plain-icon"
                      aria-label="Скопировать путь"
                      title="Скопировать путь"
                      onClick={async () => {
                        try {
                          await navigator.clipboard.writeText(
                            c.full_path || c.rel_path,
                          );
                          setCopied(c.rel_path);
                        } catch {
                          setCopied("error");
                        }
                      }}
                    >
                      <Copy size={13} />
                    </button>
                  </div>
                  {copied === c.rel_path && (
                    <small className="muted">Путь скопирован</small>
                  )}
                  {copied === "error" && (
                    <small className="muted">
                      Не удалось скопировать. Выделите путь вручную.
                    </small>
                  )}
                  <p className="conversion-label">Конвертация этой копии</p>
                  <div className="conversion-actions">
                    <button
                      className="btn btn-secondary"
                      disabled={!c.volume_online}
                      onClick={() => onOptimizeCopy(c.id, "m4b")}
                    >
                      Собрать M4B
                    </button>
                    <button
                      className="btn btn-secondary"
                      disabled={!c.volume_online}
                      onClick={() => onOptimizeCopy(c.id, "mp3")}
                    >
                      Собрать MP3
                    </button>
                  </div>
                </section>
              ))}
            {tab === "tracks" && (
              <VirtualList
                count={book.media.length}
                rowHeight={60}
                renderRow={(i) => {
                  const m = book.media[i];
                  return (
                    <div className="track-row">
                      <button
                        className="plain-icon"
                        disabled={
                          !book.copies[0]?.volume_online || !m.full_path
                        }
                        title="Слушать файл"
                        aria-label={`Слушать ${m.file_name}`}
                        onClick={() =>
                          m.full_path && onPlayTrack?.(m.file_name, m.full_path)
                        }
                      >
                        <Play size={13} />
                      </button>
                      <div>
                        <strong title={m.title || m.file_name}>
                          {m.title || m.file_name}
                        </strong>
                        <small>
                          {formatBytes(m.size)} ·{" "}
                          {m.duration_ms
                            ? formatDuration(m.duration_ms)
                            : "Время не указано"}
                        </small>
                      </div>
                      <button
                        className="plain-icon"
                        disabled={
                          !book.copies[0]?.volume_online || !m.full_path
                        }
                        title="Показать файл в Проводнике"
                        aria-label="Показать файл в Проводнике"
                        onClick={() =>
                          m.full_path && onRevealFolder(m.full_path)
                        }
                      >
                        <FolderOpen size={13} />
                      </button>
                    </div>
                  );
                }}
              />
            )}
            {tab === "desc" && (
              <p className="description">
                {book.description ||
                  "Аннотации пока нет. При сканировании AudioVault использует метаданные и сопровождающие FB2-файлы."}
              </p>
            )}
          </div>
        </>
      )}
    </aside>
  );
}
