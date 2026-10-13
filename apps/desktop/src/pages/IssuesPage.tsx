import { useCallback, useEffect, useRef, useState } from "react";
import { RefreshCw, ArrowRight } from "lucide-react";
import { api } from "../api/client";
import { issueLabels, issueDescriptions } from "../api/presentation";
import type { Issue } from "../api/types";
import { EmptyState, LoadingRows, VirtualList } from "../components/Primitives";
export function IssuesPage({
  onOpenBook,
}: {
  onOpenBook: (id: number) => void;
}) {
  const [issues, setIssues] = useState<Issue[]>([]);
  const [loading, setLoading] = useState(true);
  const [kind, setKind] = useState("all");
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [hasMore, setHasMore] = useState(false);
  const nextOffset = useRef(0);
  const pending = useRef(false);
  const revision = useRef(0);
  const load = useCallback(
    async (append = false) => {
      if (append && pending.current) return;
      const id = append ? revision.current : ++revision.current;
      const offset = append ? nextOffset.current : 0;
      pending.current = true;
      setLoading(true);
      setError("");
      try {
        const data = await api.getIssues(kind, offset);
        if (id !== revision.current) return;
        nextOffset.current = offset + data.length;
        setIssues((previous) => (append ? [...previous, ...data] : data));
        setHasMore(data.length === 100);
      } catch {
        if (id === revision.current)
          setError("Не удалось загрузить замечания. Попробуйте обновить.");
      } finally {
        if (id === revision.current) {
          pending.current = false;
          setLoading(false);
        }
      }
    },
    [kind],
  );
  useEffect(() => {
    // Retain existing rows until the requested inbox category resolves over IPC.
    // eslint-disable-next-line react/set-state-in-effect
    load();
    const activeRevision = revision.current;
    return () => {
      revision.current = activeRevision + 1;
      pending.current = false;
    };
  }, [load]);
  const loadMore = useCallback(() => {
    if (hasMore) load(true);
  }, [hasMore, load]);
  const visible = issues;
  const ignore = async (ids: number[]) => {
    setBusy(true);
    setError("");
    const done: number[] = [];
    try {
      for (const id of ids) {
        await api.ignoreIssue(id);
        done.push(id);
      }
    } catch {
      setError("Часть замечаний не удалось скрыть. Повторите действие.");
    } finally {
      setIssues((p) => p.filter((i) => !done.includes(i.id)));
      setSelected(new Set());
      if (done.length) await load();
      setBusy(false);
    }
  };
  return (
    <div className="collection-page">
      <div className="toolbar">
        <div className="toolbar-left">
          <select
            disabled={busy}
            aria-label="Категория замечаний"
            value={kind}
            onChange={(e) => {
              setKind(e.target.value);
              setSelected(new Set());
            }}
          >
            <option value="all">Все замечания</option>
            {Object.keys(issueLabels).map((k) => (
              <option key={k} value={k}>
                {issueLabels[k] ||
                  issues.find((i) => i.kind === k)?.kind_label ||
                  k}
              </option>
            ))}
          </select>
          <label
            className="muted"
            style={{ fontSize: 11, display: "flex", gap: 7 }}
          >
            <input
              type="checkbox"
              disabled={busy}
              aria-label="Выбрать все замечания"
              checked={
                visible.length > 0 && visible.every((i) => selected.has(i.id))
              }
              onChange={(e) =>
                setSelected(
                  e.target.checked
                    ? new Set(visible.map((i) => i.id))
                    : new Set(),
                )
              }
            />
            Загруженные
          </label>
        </div>
        <div className="toolbar-right">
          {selected.size > 0 && (
            <button
              className="btn btn-secondary"
              disabled={busy}
              onClick={() => ignore([...selected])}
            >
              Скрыть выбранные · {selected.size}
            </button>
          )}
          <button
            className="btn btn-quiet"
            disabled={loading}
            onClick={() => {
              setLoading(true);
              setError("");
              load();
            }}
          >
            <RefreshCw size={14} className={loading ? "spin" : ""} />
            Обновить
          </button>
        </div>
      </div>
      <div className="page-intro">
        <p>
          Замечания помогают привести коллекцию в порядок. Откройте книгу, чтобы
          проверить метаданные.
        </p>
      </div>
      {error && (
        <div className="notice error" role="alert">
          {error}
        </div>
      )}
      {loading && !issues.length ? (
        <LoadingRows />
      ) : !visible.length ? (
        <EmptyState
          title="Нет замечаний для проверки"
          description="Новые замечания появятся после сканирования коллекции."
        />
      ) : (
        <VirtualList
          key={kind}
          count={visible.length}
          onEnd={hasMore ? loadMore : undefined}
          rowHeight={100}
          renderRow={(index) => {
            const issue = visible[index];
            return (
              <div className="issue-row" style={{ height: 100 }}>
                <input
                  type="checkbox"
                  disabled={busy}
                  aria-label={`Выбрать ${issue.book_title}`}
                  checked={selected.has(issue.id)}
                  onChange={(e) =>
                    setSelected((prev) => {
                      const next = new Set(prev);
                      if (e.target.checked) next.add(issue.id);
                      else next.delete(issue.id);
                      return next;
                    })
                  }
                />
                <div>
                  <span className="issue-kind">
                    {issueLabels[issue.kind] || issue.kind_label || issue.kind}
                  </span>
                  <h3 title={issue.book_title}>
                    {issue.book_title || "Без названия"}{" "}
                    <small className="muted">
                      · {issue.book_authors || "Автор не указан"}
                    </small>
                  </h3>
                  <p title={issue.message}>
                    {issueDescriptions[issue.kind] || issue.message}
                  </p>
                  {issue.proposed_val && (
                    <div className="issue-diff">
                      <del>{issue.current_val || "(пусто)"}</del>
                      <ArrowRight size={12} />
                      <ins>{issue.proposed_val}</ins>
                    </div>
                  )}
                </div>
                <button
                  className="btn btn-secondary"
                  onClick={() => onOpenBook(issue.book_id)}
                >
                  Открыть
                </button>
                <button
                  className="btn btn-quiet"
                  disabled={busy}
                  onClick={() => ignore([issue.id])}
                >
                  Скрыть
                </button>
              </div>
            );
          }}
        />
      )}
      <div className="collection-footer">
        <span>{visible.length} замечаний загружено</span>
        {hasMore && (
          <button
            className="btn btn-quiet"
            disabled={loading || busy}
            onClick={loadMore}
          >
            {loading ? "Загрузка…" : "Загрузить ещё"}
          </button>
        )}
        <span>
          {selected.size
            ? `Выбрано ${selected.size}`
            : "Файлы остаются без изменений"}
        </span>
      </div>
    </div>
  );
}
