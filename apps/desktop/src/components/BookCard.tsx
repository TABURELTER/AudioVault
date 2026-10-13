import { memo } from "react";
import type { Book } from "../api/types";
import { formatDuration } from "../api/client";
import { BookCover, StatusIndicator } from "./Primitives";
export const BookCard = memo(function BookCard({
  book,
  onClick,
}: {
  book: Book;
  onClick: () => void;
}) {
  return (
    <button className="book-card" onClick={onClick}>
      <BookCover title={book.title} thumb={book.cover_thumb} large />
      <strong title={book.title}>{book.title || "Без названия"}</strong>
      <span title={book.authors}>{book.authors || "Автор не указан"}</span>
      <div>
        <small>
          {book.duration_ms
            ? formatDuration(book.duration_ms)
            : "Время не указано"}
        </small>
        <small className="badge badge-format">{book.format}</small>
      </div>
      <StatusIndicator online={book.online} label={book.volume_name} />
    </button>
  );
});
