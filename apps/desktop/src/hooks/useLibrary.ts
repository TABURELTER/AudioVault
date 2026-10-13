import { useCallback, useEffect, useRef, useState } from "react";
import type { Book } from "../api/types";
import { api } from "../api/client";
const PAGE_SIZE = 250;
export function useLibrary(
  filter: string,
  sort: string,
  query: string,
  volumeId: string,
) {
  const [books, setBooks] = useState<Book[]>([]);
  const [loading, setLoading] = useState(true);
  const [hasMore, setHasMore] = useState(false);
  const [error, setError] = useState("");
  const [debounced, setDebounced] = useState("");
  const revision = useRef(0);
  const offset = useRef(0);
  const fetching = useRef(false);
  useEffect(() => {
    const timer = setTimeout(() => setDebounced(query), 180);
    return () => clearTimeout(timer);
  }, [query]);
  const load = useCallback(
    async (append = false) => {
      if (append && fetching.current) return;
      const id = append ? revision.current : ++revision.current;
      fetching.current = true;
      setLoading(true);
      setError("");
      const start = append ? offset.current : 0;
      try {
        // Existing library search filters within its requested window; preserve that behavior.
        const limit = debounced.trim() ? 5000 : PAGE_SIZE;
        const rows = await api.getLibrary(
          volumeId ? `volume:${volumeId}` : filter,
          sort,
          debounced,
          limit,
          start,
        );
        if (id !== revision.current) return;
        offset.current = start + limit;
        setBooks((previous) => (append ? [...previous, ...rows] : rows));
        setHasMore(!debounced.trim() && rows.length === limit);
      } catch {
        if (id === revision.current)
          setError(
            "Не удалось загрузить каталог. Проверьте доступность базы и попробуйте обновить.",
          );
      } finally {
        if (id === revision.current) {
          fetching.current = false;
          setLoading(false);
        }
      }
    },
    [filter, sort, debounced, volumeId],
  );
  useEffect(() => {
    load();
    const activeRevision = revision.current;
    return () => {
      revision.current = activeRevision + 1;
      fetching.current = false;
    };
  }, [load]);
  const reload = useCallback(() => load(false), [load]);
  const loadMore = useCallback(() => {
    if (hasMore) load(true);
  }, [hasMore, load]);
  return {
    books,
    loading: loading || query !== debounced,
    hasMore,
    error,
    reload,
    loadMore,
  };
}
