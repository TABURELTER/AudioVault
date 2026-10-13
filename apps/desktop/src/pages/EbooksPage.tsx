import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  FolderOpen,
  ExternalLink,
  List,
  LayoutGrid,
  Type,
  Palette,
  X,
  CheckCircle2,
  Trash2,
  BookOpen,
  CheckSquare,
  BookmarkCheck,
} from "lucide-react";
import type { Ebook, EbookStats } from "../api/types";
import { api, formatBytes } from "../api/client";
import {
  BookCover,
  EmptyState,
  LoadingRows,
  SearchField,
  StatusIndicator,
  VirtualList,
  getFormatClass,
} from "../components/Primitives";
import { DeletionConfirmModal, type ItemToDelete } from "../components/DeletionConfirmModal";

interface Props {
  catalogRevision?: number;
  initialViewMode?: "catalog" | "read" | "candidates";
  onSelectEbook: (id: number) => void;
  onRevealFolder: (path: string) => void;
  onOpenFile: (path: string) => void;
}

const PAGE_SIZE = 100;

interface FormatTab {
  id: string;
  label: string;
}

const FORMAT_TABS: FormatTab[] = [
  { id: "all", label: "Все" },
  { id: "fb2", label: "FB2" },
  { id: "pdf", label: "PDF" },
  { id: "epub", label: "EPUB" },
  { id: "djvu", label: "DJVU" },
  { id: "mobi", label: "MOBI" },
  { id: "archives", label: "Архивы" },
];

export function EbooksPage({
  catalogRevision = 0,
  initialViewMode = "catalog",
  onSelectEbook,
  onRevealFolder,
  onOpenFile,
}: Props) {
  const [viewMode, setViewMode] = useState<"catalog" | "read" | "candidates">(initialViewMode);
  const [books, setBooks] = useState<Ebook[]>([]);
  const [total, setTotal] = useState(0);
  const [stats, setStats] = useState<EbookStats | null>(null);
  const [loading, setLoading] = useState(true);
  const [query, setQuery] = useState("");
  const [debounced, setDebounced] = useState("");
  const [selectedFormats, setSelectedFormats] = useState<string[]>([]);
  const [sort, setSort] = useState("author");
  const [view, setView] = useState<"table" | "grid">("table");
  const [error, setError] = useState("");
  const [columns, setColumns] = useState(5);
  const [showPaletteModal, setShowPaletteModal] = useState(false);
  const [selectedIds, setSelectedIds] = useState<Set<number>>(new Set());
  const [deleteModalOpen, setDeleteModalOpen] = useState(false);
  const [deleting, setDeleting] = useState(false);

  const [largeText, setLargeText] = useState(() => {
    try {
      return localStorage.getItem("audiovault-large-text") === "true";
    } catch {
      return false;
    }
  });

  const toggleLargeText = () => {
    setLargeText((prev) => {
      const next = !prev;
      try {
        localStorage.setItem("audiovault-large-text", String(next));
      } catch {}
      return next;
    });
  };

  const container = useRef<HTMLDivElement>(null);
  const revision = useRef(0);
  const offset = useRef(0);
  const pending = useRef(false);

  // Sync initialViewMode if prop changes
  useEffect(() => {
    setViewMode(initialViewMode);
  }, [initialViewMode]);

  // Load e-book catalog stats for tab counters
  useEffect(() => {
    api.getEbookStats().then((res) => {
      if (res) setStats(res);
    });
  }, [catalogRevision]);

  const formatCounts = useMemo(() => {
    const map: Record<string, number> = {};
    if (stats?.formats) {
      for (const [fmt, count] of stats.formats) {
        map[fmt.toLowerCase()] = count;
      }
    }
    return map;
  }, [stats]);

  const getTabCount = (tabId: string): number | null => {
    if (tabId === "all") return stats?.total_count ?? (total > 0 ? total : null);
    if (tabId === "archives") {
      const archivesCount =
        (formatCounts["fb2.zip"] || 0) + (formatCounts["zip"] || 0);
      return archivesCount > 0 ? archivesCount : null;
    }
    return formatCounts[tabId] ?? null;
  };

  const toggleFormat = (fmtId: string) => {
    if (fmtId === "all") {
      setSelectedFormats([]);
      return;
    }
    setSelectedFormats((prev) => {
      if (prev.includes(fmtId)) {
        return prev.filter((f) => f !== fmtId);
      }
      return [...prev, fmtId];
    });
  };

  const formatParam = useMemo(() => {
    if (selectedFormats.length === 0) return "all";
    return selectedFormats
      .map((f) => (f === "archives" ? "zip,fb2.zip" : f))
      .join(",");
  }, [selectedFormats]);

  useEffect(() => {
    const timer = setTimeout(() => setDebounced(query), 180);
    return () => clearTimeout(timer);
  }, [query]);

  useEffect(() => {
    const el = container.current;
    if (!el) return;
    const observer = new ResizeObserver(() =>
      setColumns(Math.max(2, Math.floor(el.clientWidth / (largeText ? 220 : 185)))),
    );
    observer.observe(el);
    return () => observer.disconnect();
  }, [largeText]);

  const readFilter = useMemo(() => {
    if (viewMode === "read") return true;
    if (viewMode === "candidates") return undefined;
    return false; // active catalog
  }, [viewMode]);

  const candidateFilter = useMemo(() => {
    if (viewMode === "candidates") return true;
    return undefined;
  }, [viewMode]);

  const load = useCallback(
    async (append = false) => {
      if (append && pending.current) return;
      const id = append ? revision.current : ++revision.current;
      pending.current = true;
      setLoading(true);
      setError("");
      const start = append ? offset.current : 0;
      try {
        const result = await api.searchEbooks(
          debounced,
          formatParam,
          sort,
          readFilter,
          candidateFilter,
          PAGE_SIZE,
          start,
        );
        if (id !== revision.current) return;
        setBooks((previous) =>
          append ? [...previous, ...result.items] : result.items,
        );
        offset.current = start + result.items.length;
        setTotal(result.total_count);
      } catch {
        if (id === revision.current)
          setError(
            "Не удалось загрузить электронные книги. Попробуйте обновить каталог.",
          );
      } finally {
        if (id === revision.current) {
          pending.current = false;
          setLoading(false);
        }
      }
    },
    [debounced, formatParam, sort, readFilter, candidateFilter],
  );

  useEffect(() => {
    load();
    const activeRevision = revision.current;
    return () => {
      revision.current = activeRevision + 1;
      pending.current = false;
    };
  }, [load, catalogRevision]);

  const more = useCallback(() => {
    if (offset.current < total) load(true);
  }, [total, load]);

  // Selection handlers
  const toggleSelect = (id: number) => {
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const toggleSelectAll = () => {
    if (books.length > 0 && books.every((b) => selectedIds.has(b.id))) {
      setSelectedIds(new Set());
    } else {
      setSelectedIds(new Set(books.map((b) => b.id)));
    }
  };

  // Status toggle handlers
  const handleToggleRead = async (b: Ebook) => {
    const nextVal = !b.is_read;
    try {
      await api.setEbookRead(b.id, nextVal);
      // Remove from current view if filtering by read status
      if (viewMode === "catalog" && nextVal) {
        setBooks((prev) => prev.filter((item) => item.id !== b.id));
        setTotal((t) => Math.max(0, t - 1));
      } else if (viewMode === "read" && !nextVal) {
        setBooks((prev) => prev.filter((item) => item.id !== b.id));
        setTotal((t) => Math.max(0, t - 1));
      } else {
        setBooks((prev) =>
          prev.map((item) => (item.id === b.id ? { ...item, is_read: nextVal } : item)),
        );
      }
    } catch (e) {
      console.error("handleToggleRead error:", e);
    }
  };

  const handleToggleCandidate = async (b: Ebook) => {
    const nextVal = !b.is_marked_for_deletion;
    try {
      await api.setEbookMarkedForDeletion(b.id, nextVal);
      if (viewMode === "catalog" && nextVal) {
        setBooks((prev) => prev.filter((item) => item.id !== b.id));
        setTotal((t) => Math.max(0, t - 1));
      } else if (viewMode === "candidates" && !nextVal) {
        setBooks((prev) => prev.filter((item) => item.id !== b.id));
        setTotal((t) => Math.max(0, t - 1));
      } else {
        setBooks((prev) =>
          prev.map((item) =>
            item.id === b.id ? { ...item, is_marked_for_deletion: nextVal } : item,
          ),
        );
      }
    } catch (e) {
      console.error("handleToggleCandidate error:", e);
    }
  };

  // Bulk actions
  const handleBulkMarkRead = async (read: boolean) => {
    const ids = Array.from(selectedIds);
    try {
      await Promise.all(ids.map((id) => api.setEbookRead(id, read)));
      setSelectedIds(new Set());
      load();
    } catch (e) {
      console.error("handleBulkMarkRead error:", e);
    }
  };

  const handleBulkMarkCandidate = async (candidate: boolean) => {
    const ids = Array.from(selectedIds);
    try {
      await api.setMultipleEbooksMarkedForDeletion(ids, candidate);
      setSelectedIds(new Set());
      load();
    } catch (e) {
      console.error("handleBulkMarkCandidate error:", e);
    }
  };

  const handleConfirmDeletion = async () => {
    const ids = Array.from(selectedIds);
    setDeleting(true);
    try {
      await api.executeDeletion([], ids);
      setSelectedIds(new Set());
      setDeleteModalOpen(false);
      load();
    } catch (e) {
      console.error("executeDeletion error:", e);
    } finally {
      setDeleting(false);
    }
  };

  const selectedItemsToDelete: ItemToDelete[] = useMemo(() => {
    return books
      .filter((b) => selectedIds.has(b.id))
      .map((b) => ({
        id: b.id,
        type: "ebook" as const,
        title: b.title || b.file_name,
        authors: b.authors,
        volumeName: b.volume_name,
        volumeOnline: b.volume_online,
        size: b.size,
        path: b.full_path || b.rel_path,
      }));
  }, [books, selectedIds]);

  const actions = (b: Ebook) => (
    <div className="storage-actions">
      <button
        className={`action-btn ${b.is_read ? "is-active" : ""}`}
        title={b.is_read ? "Отметить как непрочитанную" : "Отметить как прочитанную"}
        aria-label="Прочитано"
        onClick={(e) => {
          e.stopPropagation();
          handleToggleRead(b);
        }}
        style={{ color: b.is_read ? "#86efac" : undefined }}
      >
        <CheckCircle2 size={16} />
      </button>
      <button
        className={`action-btn ${b.is_marked_for_deletion ? "is-active" : ""}`}
        title={
          b.is_marked_for_deletion
            ? "Снять статус кандидата на удаление"
            : "Отметить как кандидат на удаление"
        }
        aria-label="Кандидат на удаление"
        onClick={(e) => {
          e.stopPropagation();
          handleToggleCandidate(b);
        }}
        style={{ color: b.is_marked_for_deletion ? "#f87171" : undefined }}
      >
        <Trash2 size={16} />
      </button>
      <button
        className="action-btn action-open"
        title="Открыть книгу в читалке"
        aria-label={`Открыть ${b.title || b.file_name}`}
        disabled={!b.volume_online || !b.full_path}
        onClick={(e) => {
          e.stopPropagation();
          b.full_path && onOpenFile(b.full_path);
        }}
      >
        <ExternalLink size={16} />
      </button>
      <button
        className="action-btn action-folder"
        title="Показать файл в Проводнике"
        aria-label="Показать в Проводнике"
        disabled={!b.volume_online || !b.full_path}
        onClick={(e) => {
          e.stopPropagation();
          b.full_path && onRevealFolder(b.full_path);
        }}
      >
        <FolderOpen size={16} />
      </button>
    </div>
  );

  return (
    <div
      className={`collection-page ${largeText ? "large-text" : ""}`}
      ref={container}
    >
      {/* Top Section View Tabs */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          gap: "10px",
          flexWrap: "wrap",
          padding: "4px 0 2px",
        }}
      >
        <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
          <button
            className={`btn ${viewMode === "catalog" ? "btn-primary" : "btn-secondary"}`}
            onClick={() => {
              setViewMode("catalog");
              setSelectedIds(new Set());
            }}
            style={{ fontSize: "13px", height: "34px" }}
          >
            <BookOpen size={15} />
            <span>Каталог книг</span>
          </button>
          <button
            className={`btn ${viewMode === "read" ? "btn-primary" : "btn-secondary"}`}
            onClick={() => {
              setViewMode("read");
              setSelectedIds(new Set());
            }}
            style={{ fontSize: "13px", height: "34px" }}
          >
            <BookmarkCheck size={15} />
            <span>Прочитано</span>
          </button>
          <button
            className={`btn ${viewMode === "candidates" ? "btn-danger" : "btn-secondary"}`}
            onClick={() => {
              setViewMode("candidates");
              setSelectedIds(new Set());
            }}
            style={{ fontSize: "13px", height: "34px" }}
          >
            <Trash2 size={15} />
            <span>Кандидаты на удаление</span>
          </button>
        </div>

        {viewMode === "candidates" && books.length > 0 && (
          <button
            className="btn btn-danger"
            onClick={() => {
              setSelectedIds(new Set(books.map((b) => b.id)));
              setDeleteModalOpen(true);
            }}
            style={{ fontSize: "13px", height: "34px", gap: 7 }}
          >
            <Trash2 size={15} />
            <span>Удалить все кандидаты ({total})</span>
          </button>
        )}
      </div>

      <div className="toolbar" style={{ flexWrap: "wrap", gap: "12px", marginTop: 4 }}>
        <div className="toolbar-left" style={{ flex: "1 1 360px", flexWrap: "wrap" }}>
          <div style={{ width: "min(320px, 100%)" }}>
            <SearchField
              value={query}
              onChange={setQuery}
              loading={loading}
              placeholder="Название, автор или серия..."
            />
          </div>
          <div className="format-chips" role="tablist" aria-label="Фильтр по форматам книг">
            {FORMAT_TABS.map((f) => {
              const isAll = f.id === "all";
              const isActive = isAll
                ? selectedFormats.length === 0
                : selectedFormats.includes(f.id);
              const fmtCls = !isAll
                ? getFormatClass(f.id === "archives" ? "zip" : f.id)
                : "";
              const count = getTabCount(f.id);
              return (
                <button
                  key={f.id}
                  role="tab"
                  aria-selected={isActive}
                  className={`format-chip ${isActive ? `active ${fmtCls}` : ""}`}
                  onClick={() => toggleFormat(f.id)}
                  title={
                    isAll
                      ? "Все форматы"
                      : `Нажмите для выбора нескольких форматов (${f.label})`
                  }
                >
                  <span>{f.label}</span>
                  {count != null && count > 0 && (
                    <span className="chip-count">
                      {count.toLocaleString("ru-RU")}
                    </span>
                  )}
                </button>
              );
            })}
          </div>
        </div>
        <div className="toolbar-right">
          <select
            aria-label="Сортировка электронных книг"
            value={sort}
            onChange={(e) => setSort(e.target.value)}
          >
            {[
              ["author", "По автору"],
              ["title", "По названию"],
              ["series", "По серии"],
              ["year", "По году"],
              ["size", "По размеру"],
              ["added", "По дате добавления"],
            ].map(([id, label]) => (
              <option key={id} value={id}>
                {label}
              </option>
            ))}
          </select>
          <div style={{ position: "relative" }}>
            <button
              className={`btn btn-secondary ${showPaletteModal ? "is-active" : ""}`}
              onClick={() => setShowPaletteModal((prev) => !prev)}
              title="Справка по цветам книжных переплётов"
              style={{ fontSize: "13px", gap: "7px", minHeight: "34px" }}
            >
              <Palette size={15} />
              <span>Цвета обложек</span>
            </button>
            {showPaletteModal && (
              <div
                className="cover-color-legend"
                style={{
                  position: "absolute",
                  top: "100%",
                  right: 0,
                  marginTop: "8px",
                  zIndex: 200,
                  width: "360px",
                }}
              >
                <div className="cover-color-legend-header">
                  <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
                    <Palette size={16} color="var(--accent)" />
                    <strong style={{ fontSize: "15px" }}>Цвета книжных переплётов</strong>
                  </div>
                  <button
                    className="plain-icon"
                    onClick={() => setShowPaletteModal(false)}
                    title="Закрыть подсказку"
                  >
                    <X size={15} />
                  </button>
                </div>
                <p style={{ fontSize: "12.5px", color: "var(--text-secondary)", margin: "4px 0 8px" }}>
                  Каждый формат имеет свой оттенок переплёта для мгновенного распознавания на полке:
                </p>
                <div style={{ display: "flex", flexDirection: "column", gap: "6px" }}>
                  {[
                    {
                      id: "fb2",
                      name: "FB2",
                      title: "Изумрудный переплёт",
                      desc: "Классический формат художественной литературы",
                      bg: "linear-gradient(145deg, #064e3b 0%, #059669 55%, #022c22 100%)",
                      spine: "#10b981",
                    },
                    {
                      id: "epub",
                      name: "EPUB",
                      title: "Аметистовый бархат",
                      desc: "Международный стандарт электронных книг",
                      bg: "linear-gradient(145deg, #3b0764 0%, #6d28d9 55%, #1e1b4b 100%)",
                      spine: "#7c3aed",
                    },
                    {
                      id: "pdf",
                      name: "PDF",
                      title: "Рубиновый сафьян",
                      desc: "Документы, иллюстрации, журналы и статьи",
                      bg: "linear-gradient(145deg, #450a0a 0%, #b91c1c 55%, #2a0808 100%)",
                      spine: "#dc2626",
                    },
                    {
                      id: "djvu",
                      name: "DJVU",
                      title: "Античный янтарь",
                      desc: "Сканированные книги, схемы и редкие труды",
                      bg: "linear-gradient(145deg, #451a03 0%, #b45309 55%, #260e02 100%)",
                      spine: "#d97706",
                    },
                    {
                      id: "mobi",
                      name: "MOBI",
                      title: "Сапфировый индиго",
                      desc: "Формат Kindle и компактных читалок",
                      bg: "linear-gradient(145deg, #082f49 0%, #0284c7 55%, #0c1c2e 100%)",
                      spine: "#0369a1",
                    },
                    {
                      id: "archives",
                      name: "ZIP / FB2.ZIP",
                      title: "Пурпурный архив",
                      desc: "Сжатые сборники и полные архивы",
                      bg: "linear-gradient(145deg, #4a044e 0%, #a21caf 55%, #2e0854 100%)",
                      spine: "#c026d3",
                    },
                  ].map((item) => (
                    <div
                      key={item.id}
                      className="cover-color-item"
                      onClick={() => {
                        toggleFormat(item.id);
                        setShowPaletteModal(false);
                      }}
                      title={`Нажмите, чтобы добавить ${item.name} в фильтр`}
                    >
                      <div
                        className="cover-color-swatch"
                        style={{ background: item.bg, borderLeftColor: item.spine }}
                      />
                      <div style={{ flex: 1, minWidth: 0 }}>
                        <div style={{ display: "flex", alignItems: "center", gap: "6px" }}>
                          <strong>{item.name}</strong>
                          <span style={{ fontSize: "12px", opacity: 0.85 }}>· {item.title}</span>
                        </div>
                        <span style={{ fontSize: "11.5px", color: "var(--text-muted)" }}>{item.desc}</span>
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            )}
          </div>
          <button
            className={`zoom-toggle ${largeText ? "is-active" : ""}`}
            onClick={toggleLargeText}
            title={
              largeText
                ? "Вернуть стандартный размер шрифта"
                : "Включить крупный шрифт и высокую контрастность для слабовидящих"
            }
            aria-pressed={largeText}
          >
            <Type size={16} />
            <span>{largeText ? "Шрифт 125%" : "Крупный шрифт"}</span>
          </button>
          <div className="segmented">
            <button
              aria-label="Список"
              aria-pressed={view === "table"}
              onClick={() => setView("table")}
            >
              <List size={16} />
            </button>
            <button
              aria-label="Обложки"
              aria-pressed={view === "grid"}
              onClick={() => setView("grid")}
            >
              <LayoutGrid size={16} />
            </button>
          </div>
        </div>
      </div>

      {error && (
        <div className="notice error" role="alert">
          {error}
          <button className="btn" onClick={() => load()}>
            Повторить
          </button>
        </div>
      )}

      {loading && !books.length ? (
        <LoadingRows />
      ) : !books.length ? (
        <EmptyState
          title={
            viewMode === "read"
              ? "Нет прочитанных книг"
              : viewMode === "candidates"
              ? "Нет кандидатов на удаление"
              : query || selectedFormats.length > 0
              ? "Ничего не найдено"
              : "Электронных книг пока нет"
          }
          description={
            viewMode === "read"
              ? "Отмечайте книги как прочитанные в каталоге, и они появятся в этой вкладке."
              : viewMode === "candidates"
              ? "Вы можете отметить ненужные книги значком корзины для последующего массового удаления."
              : query || selectedFormats.length > 0
              ? "Попробуйте другое название или измените выбор форматов."
              : "Добавьте папки с книгами в разделе «Хранилища» и запустите сканирование."
          }
        />
      ) : (
        <VirtualList
          key={`${viewMode}-${view}-${sort}-${selectedFormats.join(",")}-${debounced}-${largeText}`}
          count={
            view === "table" ? books.length : Math.ceil(books.length / columns)
          }
          rowHeight={view === "table" ? (largeText ? 76 : 64) : (largeText ? 380 : 330)}
          onEnd={books.length < total ? more : undefined}
          header={
            view === "table" && (
              <div
                className="ebook-columns table-head"
                style={{ gridTemplateColumns: "36px minmax(280px, 2fr) minmax(140px, 1.2fr) 90px 95px 160px 130px" }}
              >
                <span style={{ display: "flex", alignItems: "center", justifyContent: "center" }}>
                  <input
                    type="checkbox"
                    aria-label="Выбрать все книги на странице"
                    checked={books.length > 0 && books.every((b) => selectedIds.has(b.id))}
                    onChange={toggleSelectAll}
                  />
                </span>
                <span>Название / серия</span>
                <span>Автор</span>
                <span>Формат</span>
                <span>Размер</span>
                <span>Накопитель</span>
                <span style={{ textAlign: "right" }}>Действия</span>
              </div>
            )
          }
          renderRow={(index) =>
            view === "table" ? (
              (() => {
                const b = books[index];
                const isSelected = selectedIds.has(b.id);
                return (
                  <div
                    className={`ebook-columns book-row ${isSelected ? "selected" : ""}`}
                    style={{
                      gridTemplateColumns: "36px minmax(280px, 2fr) minmax(140px, 1.2fr) 90px 95px 160px 130px",
                    }}
                  >
                    <span style={{ display: "flex", alignItems: "center", justifyContent: "center" }}>
                      <input
                        type="checkbox"
                        aria-label={`Выбрать ${b.title || b.file_name}`}
                        checked={isSelected}
                        onChange={() => toggleSelect(b.id)}
                        onClick={(e) => e.stopPropagation()}
                      />
                    </span>
                    <span className="book-title-cell">
                      <BookCover
                        title={b.title || b.file_name}
                        thumb={b.cover_thumb}
                        format={b.format}
                        author={b.authors}
                      />
                      <span>
                        <button
                          className="title-button"
                          title={b.title || b.file_name}
                          onClick={() => onSelectEbook(b.id)}
                        >
                          {b.title || b.file_name}
                        </button>
                        <small>
                          {b.series ? (
                            <span style={{ color: "var(--accent-hover)", fontWeight: 700 }}>
                              {b.series}
                              {b.series_index != null ? ` #${b.series_index}` : ""}
                            </span>
                          ) : b.year ? (
                            `${b.year} г.`
                          ) : (
                            "Электронная книга"
                          )}
                        </small>
                      </span>
                    </span>
                    <span
                      title={b.authors}
                      style={{ color: "var(--text-secondary)", fontWeight: 600, fontSize: "13.5px" }}
                    >
                      {b.authors || "—"}
                    </span>
                    <span>
                      <span className={`badge badge-format ${getFormatClass(b.format)}`}>
                        {b.format.toUpperCase()}
                      </span>
                    </span>
                    <span className="numeric">{formatBytes(b.size)}</span>
                    <span className="book-storage">
                      <StatusIndicator
                        online={b.volume_online}
                        label={b.volume_name}
                      />
                      <small style={{ color: b.volume_online ? "#86efac" : "#fca5a5", fontWeight: 650 }}>
                        {b.volume_online ? "Подключён" : "Отключён"}
                      </small>
                    </span>
                    <span>{actions(b)}</span>
                  </div>
                );
              })()
            ) : (
              <div
                className="shelf-row"
                style={{
                  height: largeText ? 380 : 330,
                  gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))`,
                }}
              >
                {books
                  .slice(index * columns, (index + 1) * columns)
                  .map((b) => {
                    const isSelected = selectedIds.has(b.id);
                    return (
                      <div
                        className={`ebook-shelf-item ${isSelected ? "selected" : ""}`}
                        key={b.id}
                        style={{ position: "relative" }}
                      >
                        <div
                          style={{
                            position: "absolute",
                            top: 8,
                            left: 8,
                            zIndex: 10,
                            background: "rgba(0,0,0,0.6)",
                            borderRadius: 4,
                            padding: "3px 4px",
                            display: "flex",
                          }}
                        >
                          <input
                            type="checkbox"
                            aria-label={`Выбрать ${b.title || b.file_name}`}
                            checked={isSelected}
                            onChange={() => toggleSelect(b.id)}
                            onClick={(e) => e.stopPropagation()}
                          />
                        </div>
                        <button
                          className="book-card"
                          onClick={() => onSelectEbook(b.id)}
                        >
                          <BookCover
                            title={b.title || b.file_name}
                            thumb={b.cover_thumb}
                            format={b.format}
                            author={b.authors}
                            large
                          />
                          <strong title={b.title || b.file_name}>
                            {b.title || b.file_name}
                          </strong>
                          <span title={b.authors}>{b.authors || "Автор не указан"}</span>
                          <div className="book-card-meta">
                            <span className={`badge badge-format ${getFormatClass(b.format)}`}>
                              {b.format.toUpperCase()}
                            </span>
                            <span style={{ fontWeight: 650, color: "var(--text-primary)" }}>
                              {formatBytes(b.size)}
                            </span>
                          </div>
                        </button>
                        <div className="storage-actions">
                          <StatusIndicator
                            online={b.volume_online}
                            label={b.volume_name}
                          />
                          {actions(b)}
                        </div>
                      </div>
                    );
                  })}
              </div>
            )
          }
        />
      )}

      {/* Floating Bulk Action Bar */}
      {selectedIds.size > 0 && (
        <div className="floating-bulk-bar">
          <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
            <CheckSquare size={16} color="var(--accent-hover)" />
            <strong style={{ fontSize: "13.5px" }}>Выбрано: {selectedIds.size}</strong>
          </div>

          <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
            {viewMode !== "read" ? (
              <button
                className="btn btn-secondary"
                onClick={() => handleBulkMarkRead(true)}
                style={{ fontSize: "12.5px", gap: "6px" }}
              >
                <CheckCircle2 size={14} color="#86efac" />
                <span>В прочитанные</span>
              </button>
            ) : (
              <button
                className="btn btn-secondary"
                onClick={() => handleBulkMarkRead(false)}
                style={{ fontSize: "12.5px", gap: "6px" }}
              >
                <span>Вернуть в каталог</span>
              </button>
            )}

            {viewMode !== "candidates" ? (
              <button
                className="btn btn-secondary"
                onClick={() => handleBulkMarkCandidate(true)}
                style={{ fontSize: "12.5px", gap: "6px" }}
              >
                <Trash2 size={14} color="#f87171" />
                <span>В кандидаты</span>
              </button>
            ) : (
              <button
                className="btn btn-secondary"
                onClick={() => handleBulkMarkCandidate(false)}
                style={{ fontSize: "12.5px", gap: "6px" }}
              >
                <span>Снять статус кандидата</span>
              </button>
            )}

            <button
              className="btn btn-danger"
              onClick={() => setDeleteModalOpen(true)}
              style={{ fontSize: "12.5px", gap: "6px" }}
            >
              <Trash2 size={14} />
              <span>Удалить ({selectedIds.size})</span>
            </button>

            <button
              className="plain-icon"
              onClick={() => setSelectedIds(new Set())}
              title="Снять выбор"
            >
              <X size={16} />
            </button>
          </div>
        </div>
      )}

      {/* Deletion confirmation modal */}
      <DeletionConfirmModal
        isOpen={deleteModalOpen}
        items={selectedItemsToDelete}
        busy={deleting}
        onConfirm={handleConfirmDeletion}
        onCancel={() => setDeleteModalOpen(false)}
      />

      <div className="collection-footer">
        <span style={{ fontWeight: 650, color: "var(--text-secondary)", fontSize: "12px" }}>
          {total.toLocaleString("ru-RU")} книг в разделе ·{" "}
          {books.length.toLocaleString("ru-RU")} загружено
          {selectedFormats.length > 0 && ` · Форматы: ${selectedFormats.join(", ").toUpperCase()}`}
        </span>
        {books.length < total && (
          <button className="btn btn-quiet" disabled={loading} onClick={more}>
            {loading ? "Загрузка…" : "Загрузить ещё"}
          </button>
        )}
      </div>
    </div>
  );
}
