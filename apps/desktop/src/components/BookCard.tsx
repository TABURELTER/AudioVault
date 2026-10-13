import { memo } from "react";
import { CheckCircle2, Trash2 } from "lucide-react";
import type { Book } from "../api/types";
import { api, formatDuration } from "../api/client";
import { BookCover, StatusIndicator } from "./Primitives";

export const BookCard = memo(function BookCard({
  book,
  onClick,
  onStatusChange,
}: {
  book: Book;
  onClick: () => void;
  onStatusChange?: () => void;
}) {
  return (
    <div
      className="book-card"
      role="button"
      tabIndex={0}
      onClick={onClick}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onClick();
        }
      }}
      style={{ position: "relative" }}
    >
      <BookCover title={book.title} thumb={book.cover_thumb} large />
      <strong title={book.title}>{book.title || "Без названия"}</strong>
      <span title={book.authors}>{book.authors || "Автор не указан"}</span>
      <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: 6 }}>
        <small>
          {book.duration_ms
            ? formatDuration(book.duration_ms)
            : "Время не указано"}
        </small>
        <small className="badge badge-format">{book.format}</small>
      </div>
      <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: 6, marginTop: 4 }}>
        <StatusIndicator online={book.online} label={book.volume_name} />
        <div style={{ display: "flex", alignItems: "center", gap: 4 }} onClick={(e) => e.stopPropagation()}>
          <button
            type="button"
            className={`btn-listened-quick ${book.is_listened ? "is-active" : ""}`}
            style={{ height: 26, padding: "0 8px", fontSize: 11 }}
            title={book.is_listened ? "Снять метку «Прослушано»" : "Отметить как прослушанную"}
            onClick={async (e) => {
              e.stopPropagation();
              await api.setBookListened(book.id, !book.is_listened);
              onStatusChange?.();
            }}
          >
            <CheckCircle2 size={13} />
            <span>{book.is_listened ? "Прослушано" : "Прослушано?"}</span>
          </button>
          <button
            type="button"
            className={`btn-mark-del-quick ${book.is_marked_for_deletion ? "is-active" : ""}`}
            style={{ width: 26, height: 26, minWidth: 26 }}
            title={book.is_marked_for_deletion ? "Снять метку на удаление" : "Пометить на удаление"}
            onClick={async (e) => {
              e.stopPropagation();
              await api.setBookMarkedForDeletion(book.id, !book.is_marked_for_deletion);
              onStatusChange?.();
            }}
          >
            <Trash2 size={13} />
          </button>
        </div>
      </div>
    </div>
  );
});
