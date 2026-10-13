import { useCallback, useEffect, useRef, useState, useMemo } from "react";
import {
  RefreshCw,
  ArrowRight,
  Trash2,
  EyeOff,
  Filter,
} from "lucide-react";
import { api } from "../api/client";
import { issueLabels, issueDescriptions } from "../api/presentation";
import type { Issue } from "../api/types";
import { EmptyState, LoadingRows, VirtualList } from "../components/Primitives";
import { DeletionConfirmModal, type ItemToDelete } from "../components/DeletionConfirmModal";

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
  const [notice, setNotice] = useState("");
  const [hasMore, setHasMore] = useState(false);
  const [deleteModalOpen, setDeleteModalOpen] = useState(false);

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

  // Selection helpers
  const toggleSelectAllVisible = () => {
    if (visible.length > 0 && visible.every((i) => selected.has(i.id))) {
      setSelected(new Set());
    } else {
      setSelected(new Set(visible.map((i) => i.id)));
    }
  };

  // Ignore selected issues
  const handleIgnoreSelected = async () => {
    if (selected.size === 0 || busy) return;
    setBusy(true);
    setError("");
    const ids = Array.from(selected);
    try {
      await api.ignoreIssues(ids);
      setIssues((p) => p.filter((i) => !selected.has(i.id)));
      setSelected(new Set());
      setNotice(`Скрыто ${ids.length} замечаний.`);
      setTimeout(() => setNotice(""), 3000);
    } catch {
      setError("Не удалось скрыть выбранные замечания.");
    } finally {
      setBusy(false);
    }
  };

  // Ignore all issues of current kind
  const handleIgnoreAllOfKind = async () => {
    const kindLabel = issueLabels[kind] || kind;
    if (
      !window.confirm(
        kind === "all"
          ? "Игнорировать (удалить из списка) ВСЕ замечания в библиотеке?"
          : `Игнорировать все замечания категории «${kindLabel}»?`,
      )
    )
      return;

    setBusy(true);
    setError("");
    try {
      const count = await api.ignoreIssuesByKind(kind);
      setSelected(new Set());
      setNotice(`Скрыто замечаний: ${count}.`);
      setTimeout(() => setNotice(""), 3000);
      load();
    } catch {
      setError("Не удалось скрыть замечания данной категории.");
    } finally {
      setBusy(false);
    }
  };

  // Mark books as candidates for deletion
  const handleMarkBooksCandidate = async () => {
    if (selected.size === 0 || busy) return;
    setBusy(true);
    const bookIds = Array.from(
      new Set(
        issues
          .filter((i) => selected.has(i.id))
          .map((i) => i.book_id)
          .filter(Boolean),
      ),
    );
    try {
      await api.setMultipleBooksMarkedForDeletion(bookIds, true);
      setNotice(
        `Отмечено как кандидаты на удаление: ${bookIds.length} аудиокниг. Они доступны на вкладке «Кандидаты на удаление» в фонотеке.`,
      );
      setTimeout(() => setNotice(""), 4000);
    } catch {
      setError("Не удалось отметить книги на удаление.");
    } finally {
      setBusy(false);
    }
  };

  const selectedItemsToDelete: ItemToDelete[] = useMemo(() => {
    const map = new Map<number, ItemToDelete>();
    for (const i of issues) {
      if (selected.has(i.id) && !map.has(i.book_id)) {
        map.set(i.book_id, {
          id: i.book_id,
          type: "book",
          title: i.book_title || "Аудиокнига",
          authors: i.book_authors,
        });
      }
    }
    return Array.from(map.values());
  }, [issues, selected]);

  const handleConfirmDeleteBooks = async () => {
    const bookIds = selectedItemsToDelete.map((b) => b.id);
    setBusy(true);
    try {
      await api.executeDeletion(bookIds, []);
      // Also ignore their issues
      const issueIds = Array.from(selected);
      await api.ignoreIssues(issueIds);
      setIssues((p) => p.filter((i) => !selected.has(i.id)));
      setSelected(new Set());
      setDeleteModalOpen(false);
      setNotice(`Удалено ${bookIds.length} книг с проблемами.`);
      setTimeout(() => setNotice(""), 3000);
    } catch (e) {
      console.error("handleConfirmDeleteBooks error:", e);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="collection-page">
      <div className="toolbar" style={{ flexWrap: "wrap", gap: 10 }}>
        <div className="toolbar-left" style={{ flexWrap: "wrap", gap: 10 }}>
          <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
            <Filter size={15} color="var(--text-muted)" />
            <select
              disabled={busy}
              aria-label="Категория замечаний"
              value={kind}
              onChange={(e) => {
                setKind(e.target.value);
                setSelected(new Set());
              }}
            >
              <option value="all">Все категории замечаний</option>
              {Object.keys(issueLabels).map((k) => (
                <option key={k} value={k}>
                  {issueLabels[k] ||
                    issues.find((i) => i.kind === k)?.kind_label ||
                    k}
                </option>
              ))}
            </select>
          </div>

          <label
            className="muted"
            style={{
              fontSize: 12,
              display: "flex",
              alignItems: "center",
              gap: 7,
              cursor: "pointer",
              userSelect: "none",
            }}
          >
            <input
              type="checkbox"
              disabled={busy || visible.length === 0}
              aria-label="Выбрать все замечания"
              checked={
                visible.length > 0 && visible.every((i) => selected.has(i.id))
              }
              onChange={toggleSelectAllVisible}
            />
            <span>Выбрать все ({visible.length})</span>
          </label>
        </div>

        <div className="toolbar-right" style={{ flexWrap: "wrap", gap: 8 }}>
          {selected.size > 0 && (
            <>
              <button
                className="btn btn-secondary"
                disabled={busy}
                onClick={handleIgnoreSelected}
                title="Скрыть выбранные замечания из списка"
                style={{ fontSize: 12.5, gap: 5 }}
              >
                <EyeOff size={14} />
                <span>Игнорировать ({selected.size})</span>
              </button>

              <button
                className="btn btn-secondary"
                disabled={busy}
                onClick={handleMarkBooksCandidate}
                title="Пометить книги выбранных замечаний как кандидаты на удаление"
                style={{ fontSize: 12.5, gap: 5 }}
              >
                <Trash2 size={14} color="#f87171" />
                <span>В кандидаты на удаление</span>
              </button>

              <button
                className="btn btn-danger"
                disabled={busy}
                onClick={() => setDeleteModalOpen(true)}
                title="Удалить файлы и каталоги книг с выбранными проблемами"
                style={{ fontSize: 12.5, gap: 5 }}
              >
                <Trash2 size={14} />
                <span>Удалить книги</span>
              </button>
            </>
          )}

          {visible.length > 0 && (
            <button
              className="btn btn-quiet"
              disabled={busy}
              onClick={handleIgnoreAllOfKind}
              title="Игнорировать все замечания выбранного типа"
              style={{ fontSize: 12 }}
            >
              Скрыть все категории
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
          Замечания помогают выявить опечатки, низкое качество или пропущенные обложки.
          Вы можете исправить метаданные в карточке книги, скрыть ненужные замечания или пометить книги на удаление.
        </p>
      </div>

      {notice && (
        <div className="notice success" role="status" style={{ background: "rgba(34, 197, 94, 0.12)", borderColor: "rgba(34, 197, 94, 0.3)" }}>
          {notice}
        </div>
      )}

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
          rowHeight={104}
          renderRow={(index) => {
            const issue = visible[index];
            const isSelected = selected.has(issue.id);
            return (
              <div
                className={`issue-row ${isSelected ? "selected" : ""}`}
                style={{ height: 104, display: "flex", alignItems: "center", gap: 12, padding: "10px 14px" }}
              >
                <input
                  type="checkbox"
                  disabled={busy}
                  aria-label={`Выбрать ${issue.book_title}`}
                  checked={isSelected}
                  onChange={(e) =>
                    setSelected((prev) => {
                      const next = new Set(prev);
                      if (e.target.checked) next.add(issue.id);
                      else next.delete(issue.id);
                      return next;
                    })
                  }
                  style={{ width: 16, height: 16, cursor: "pointer", flexShrink: 0 }}
                />
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                    <span className="issue-kind">
                      {issueLabels[issue.kind] || issue.kind_label || issue.kind}
                    </span>
                    <h3
                      title={issue.book_title}
                      style={{
                        margin: 0,
                        fontSize: 14,
                        fontWeight: 700,
                        whiteSpace: "nowrap",
                        overflow: "hidden",
                        textOverflow: "ellipsis",
                      }}
                    >
                      {issue.book_title || "Без названия"}
                      <small className="muted" style={{ fontWeight: 400, marginLeft: 6 }}>
                        · {issue.book_authors || "Автор не указан"}
                      </small>
                    </h3>
                  </div>
                  <p
                    title={issue.message}
                    style={{
                      margin: "3px 0 0",
                      fontSize: 12.5,
                      color: "var(--text-secondary)",
                      whiteSpace: "nowrap",
                      overflow: "hidden",
                      textOverflow: "ellipsis",
                    }}
                  >
                    {issueDescriptions[issue.kind] || issue.message}
                  </p>
                  {issue.proposed_val && (
                    <div className="issue-diff" style={{ marginTop: 4 }}>
                      <del>{issue.current_val || "(пусто)"}</del>
                      <ArrowRight size={12} />
                      <ins>{issue.proposed_val}</ins>
                    </div>
                  )}
                </div>

                <div style={{ display: "flex", alignItems: "center", gap: 8, flexShrink: 0 }}>
                  <button
                    className="btn btn-secondary"
                    onClick={() => onOpenBook(issue.book_id)}
                    style={{ fontSize: 12, height: 32 }}
                  >
                    Открыть
                  </button>
                  <button
                    className="btn btn-quiet"
                    disabled={busy}
                    onClick={() => {
                      api.ignoreIssues([issue.id]).then(() => {
                        setIssues((p) => p.filter((i) => i.id !== issue.id));
                        setSelected((s) => {
                          const n = new Set(s);
                          n.delete(issue.id);
                          return n;
                        });
                      });
                    }}
                    title="Скрыть замечание"
                    style={{ fontSize: 12, height: 32 }}
                  >
                    Скрыть
                  </button>
                </div>
              </div>
            );
          }}
        />
      )}

      {/* Deletion Confirm Modal for books associated with issues */}
      <DeletionConfirmModal
        isOpen={deleteModalOpen}
        items={selectedItemsToDelete}
        busy={busy}
        onConfirm={handleConfirmDeleteBooks}
        onCancel={() => setDeleteModalOpen(false)}
      />

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
            ? `Выбрано ${selected.size} замечаний`
            : "Файлы остаются без изменений"}
        </span>
      </div>
    </div>
  );
}
