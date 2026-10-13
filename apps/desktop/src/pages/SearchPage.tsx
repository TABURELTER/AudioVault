import { useEffect, useState, useRef } from "react";
import type { Book } from "../api/types";
import { SearchField, EmptyState, LoadingRows } from "../components/Primitives";
import { BookList } from "../components/BookList";
export function SearchPage({
  onSelectBook,
  onSearch,
}: {
  onSelectBook: (id: number) => void;
  onSearch: (q: string) => Promise<Book[]>;
}) {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<Book[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const request = useRef(0);
  const [largeText] = useState(() => {
    try {
      return localStorage.getItem("audiovault-large-text") === "true";
    } catch {
      return false;
    }
  });
  // Debounced IPC keeps previous results visible until the latest request resolves.
  useEffect(() => {
    const id = ++request.current;
    if (!query.trim()) return;
    const timer = setTimeout(async () => {
      try {
        const rows = await onSearch(query.trim());
        if (id === request.current) setResults(rows);
      } catch {
        if (id === request.current)
          setError("Не удалось выполнить поиск. Попробуйте ещё раз.");
      } finally {
        if (id === request.current) setLoading(false);
      }
    }, 200);
    return () => clearTimeout(timer);
  }, [query, onSearch]);
  return (
    <div className={`collection-page ${largeText ? "large-text" : ""}`}>
      <div className="toolbar">
        <div style={{ flex: 1, maxWidth: 620 }}>
          <SearchField
            value={query}
            onChange={(value) => {
              request.current++;
              setQuery(value);
              setLoading(!!value.trim());
              setError("");
              if (!value.trim()) setResults([]);
            }}
            loading={loading}
            autoFocus
            placeholder="Название, автор, чтец или серия"
          />
        </div>
      </div>
      <div className="page-intro">
        <p>
          Ищите кириллицей или латиницей. Книги на отключённых дисках тоже
          найдутся.
        </p>
      </div>
      {error && (
        <div className="notice error" role="alert">
          {error}
        </div>
      )}
      {loading && !results.length ? (
        <LoadingRows />
      ) : results.length ? (
        <BookList
          books={results}
          onSelectBook={onSelectBook}
          largeText={largeText}
        />
      ) : (
        <EmptyState
          title={query.trim() ? "Ничего не найдено" : "Найдите следующую книгу"}
          description={
            query.trim()
              ? "Попробуйте часть названия или фамилии автора."
              : "Введите название, автора или чтеца. Результаты появятся во время ввода."
          }
        />
      )}
      <div className="collection-footer">
        <span>
          {loading
            ? "Поиск…"
            : `${results.length} результатов${results.length === 50 ? " · первые 50, уточните запрос" : ""}`}
        </span>
        <span>По всей коллекции</span>
      </div>
    </div>
  );
}
