import { useEffect, useRef, useState, useCallback, useMemo } from "react";
import {
  MoreHorizontal,
  FolderOpen,
  Copy,
  Info,
  Files,
  ArrowUp,
  ArrowDown,
  ArrowUpDown,
  Settings2,
  X,
  RotateCcw,
  SlidersHorizontal,
  Type,
  CheckCircle2,
  Trash2,
} from "lucide-react";
import type { Book } from "../api/types";
import { api, formatBytes, formatDuration } from "../api/client";
import { BookCover, StatusIndicator, VirtualList, getFormatClass } from "./Primitives";
import { BookCard } from "./BookCard";
import { ContextMenu } from "./ContextMenu";

export type ColumnId =
  | "title"
  | "author"
  | "narrator"
  | "duration"
  | "format"
  | "size"
  | "volume";

export type CoverSize = "compact" | "medium" | "large" | "xlarge";

export interface ColumnDef {
  id: ColumnId;
  label: string;
  defaultWidth: number;
  minWidth: number;
  canHide: boolean;
  sortKey: string;
}

export const ALL_COLUMNS: ColumnDef[] = [
  {
    id: "title",
    label: "Название / серия",
    defaultWidth: 340,
    minWidth: 200,
    canHide: false,
    sortKey: "title",
  },
  {
    id: "author",
    label: "Автор",
    defaultWidth: 210,
    minWidth: 120,
    canHide: true,
    sortKey: "author",
  },
  {
    id: "narrator",
    label: "Чтец",
    defaultWidth: 190,
    minWidth: 110,
    canHide: true,
    sortKey: "narrator",
  },
  {
    id: "duration",
    label: "Время",
    defaultWidth: 100,
    minWidth: 75,
    canHide: true,
    sortKey: "duration",
  },
  {
    id: "format",
    label: "Формат",
    defaultWidth: 85,
    minWidth: 65,
    canHide: true,
    sortKey: "format",
  },
  {
    id: "size",
    label: "Размер",
    defaultWidth: 95,
    minWidth: 75,
    canHide: true,
    sortKey: "size",
  },
  {
    id: "volume",
    label: "Накопитель",
    defaultWidth: 180,
    minWidth: 120,
    canHide: true,
    sortKey: "volume",
  },
];

const DEFAULT_ORDER: ColumnId[] = [
  "title",
  "author",
  "narrator",
  "duration",
  "format",
  "size",
  "volume",
];

const ROW_HEIGHT_BY_COVER: Record<CoverSize, number> = {
  compact: 48,
  medium: 64,
  large: 82,
  xlarge: 106,
};

const ROW_HEIGHT_BY_COVER_LARGE: Record<CoverSize, number> = {
  compact: 60,
  medium: 78,
  large: 98,
  xlarge: 122,
};

function loadSavedLargeText(): boolean {
  try {
    return localStorage.getItem("audiovault-large-text") === "true";
  } catch {
    return false;
  }
}

function loadSavedOrder(): ColumnId[] {
  try {
    const raw = localStorage.getItem("audiovault_col_order");
    if (raw) {
      const parsed = JSON.parse(raw);
      if (Array.isArray(parsed)) {
        const valid = parsed.filter((id): id is ColumnId =>
          ALL_COLUMNS.some((c) => c.id === id)
        );
        const missing = DEFAULT_ORDER.filter((id) => !valid.includes(id));
        return [...valid, ...missing];
      }
    }
  } catch {}
  return [...DEFAULT_ORDER];
}

function loadSavedWidths(): Record<string, number> {
  try {
    const raw = localStorage.getItem("audiovault_col_widths");
    if (raw) {
      const parsed = JSON.parse(raw);
      if (typeof parsed === "object" && parsed !== null) return parsed;
    }
  } catch {}
  const defaults: Record<string, number> = {};
  for (const c of ALL_COLUMNS) defaults[c.id] = c.defaultWidth;
  return defaults;
}

function loadSavedHidden(): ColumnId[] {
  try {
    const raw = localStorage.getItem("audiovault_hidden_cols");
    if (raw) {
      const parsed = JSON.parse(raw);
      if (Array.isArray(parsed)) {
        return parsed.filter(
          (id): id is ColumnId =>
            id !== "title" && ALL_COLUMNS.some((c) => c.id === id)
        );
      }
    }
  } catch {}
  return [];
}

function loadSavedCoverSize(): CoverSize {
  try {
    const raw = localStorage.getItem("audiovault_cover_size");
    if (
      raw === "compact" ||
      raw === "medium" ||
      raw === "large" ||
      raw === "xlarge"
    ) {
      return raw;
    }
  } catch {}
  return "medium";
}

function getSortDirection(
  activeSort: string,
  sortKey: string
): "asc" | "desc" | null {
  if (!activeSort) return null;
  if (sortKey === "duration") {
    if (activeSort === "duration" || activeSort === "duration_desc") return "desc";
    if (activeSort === "duration_asc") return "asc";
    return null;
  }
  if (sortKey === "size") {
    if (activeSort === "size" || activeSort === "size_desc") return "desc";
    if (activeSort === "size_asc") return "asc";
    return null;
  }
  if (sortKey === "title") {
    if (activeSort === "title" || activeSort === "title_asc") return "asc";
    if (activeSort === "title_desc") return "desc";
    return null;
  }
  if (sortKey === "author") {
    if (activeSort === "author" || activeSort === "author_asc") return "asc";
    if (activeSort === "author_desc") return "desc";
    return null;
  }
  if (sortKey === "narrator") {
    if (activeSort === "narrator" || activeSort === "narrator_asc") return "asc";
    if (activeSort === "narrator_desc") return "desc";
    return null;
  }
  if (sortKey === "format") {
    if (activeSort === "format" || activeSort === "format_asc") return "asc";
    if (activeSort === "format_desc") return "desc";
    return null;
  }
  if (sortKey === "volume") {
    if (
      activeSort === "volume" ||
      activeSort === "volume_asc" ||
      activeSort === "storage" ||
      activeSort === "storage_asc"
    )
      return "asc";
    if (activeSort === "volume_desc" || activeSort === "storage_desc")
      return "desc";
    return null;
  }
  return null;
}

function getNextSort(currentSort: string, sortKey: string): string {
  const dir = getSortDirection(currentSort, sortKey);
  if (!dir) {
    if (sortKey === "duration" || sortKey === "size") {
      return `${sortKey}_desc`;
    }
    return `${sortKey}_asc`;
  }
  return dir === "asc" ? `${sortKey}_desc` : `${sortKey}_asc`;
}

export function BookList({
  books,
  onSelectBook,
  view = "table",
  onLoadMore,
  selectedBookId,
  onFindDuplicates,
  activeSort = "",
  onSortChange,
  coverSize: externalCoverSize,
  onCoverSizeChange: externalOnCoverSizeChange,
  largeText: externalLargeText,
  onToggleLargeText: externalOnToggleLargeText,
  selectedIds,
  onToggleSelect,
  onToggleSelectAll,
  onBookStatusChange,
}: {
  books: Book[];
  onSelectBook: (id: number) => void;
  view?: "table" | "grid";
  onLoadMore?: () => void;
  selectedBookId?: number | null;
  onFindDuplicates?: () => void;
  activeSort?: string;
  onSortChange?: (s: string) => void;
  coverSize?: CoverSize;
  onCoverSizeChange?: (size: CoverSize) => void;
  largeText?: boolean;
  onToggleLargeText?: () => void;
  selectedIds?: Set<number>;
  onToggleSelect?: (id: number) => void;
  onToggleSelectAll?: () => void;
  onBookStatusChange?: () => void;
}) {
  const container = useRef<HTMLDivElement>(null);
  const [gridColumns, setGridColumns] = useState(5);
  const [focused, setFocused] = useState(0);
  const [menu, setMenu] = useState<{ book: Book; x: number; y: number } | null>(
    null
  );
  const [actionError, setActionError] = useState("");

  // Column configuration states
  const [columnOrder, setColumnOrder] = useState<ColumnId[]>(loadSavedOrder);
  const [colWidths, setColWidths] = useState<Record<string, number>>(loadSavedWidths);
  const [hiddenCols, setHiddenCols] = useState<ColumnId[]>(loadSavedHidden);
  const [localCoverSize, setLocalCoverSize] = useState<CoverSize>(loadSavedCoverSize);
  const [localLargeText, setLocalLargeText] = useState<boolean>(loadSavedLargeText);
  const [internalSort, setInternalSort] = useState<string>("");
  const [showSettingsModal, setShowSettingsModal] = useState(false);

  // Drag and drop states for headers
  const [draggedCol, setDraggedCol] = useState<ColumnId | null>(null);
  const [dragOverCol, setDragOverCol] = useState<ColumnId | null>(null);

  const effectiveCoverSize = externalCoverSize ?? localCoverSize;
  const isLargeText = externalLargeText ?? localLargeText;
  const currentSort = activeSort || internalSort;

  const toggleLargeText = useCallback(() => {
    if (externalOnToggleLargeText) {
      externalOnToggleLargeText();
    } else {
      setLocalLargeText((prev) => {
        const next = !prev;
        try {
          localStorage.setItem("audiovault-large-text", String(next));
        } catch {}
        return next;
      });
    }
  }, [externalOnToggleLargeText]);

  const updateCoverSize = useCallback(
    (size: CoverSize) => {
      setLocalCoverSize(size);
      try {
        localStorage.setItem("audiovault_cover_size", size);
      } catch {}
      externalOnCoverSizeChange?.(size);
    },
    [externalOnCoverSizeChange]
  );

  useEffect(() => {
    const el = container.current;
    if (!el) return;
    const observer = new ResizeObserver(() =>
      setGridColumns(Math.max(2, Math.floor(el.clientWidth / 170)))
    );
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  const closeMenu = useCallback(() => setMenu(null), []);

  const fileAction = async (copy: boolean) => {
    const book = menu?.book;
    closeMenu();
    if (!book) return;
    try {
      const detail = await api.getBookDetails(book.id);
      const path = copy
        ? detail?.copies[0]?.full_path || detail?.copies[0]?.rel_path
        : detail?.copies.find((c) => c.volume_online && c.full_path)?.full_path;
      if (!path) throw new Error("offline");
      if (copy) await navigator.clipboard.writeText(path);
      else await api.revealFolder(path);
    } catch {
      setActionError(
        "Не удалось открыть путь. Проверьте подключение накопителя."
      );
    }
  };

  // Header click sorting
  const handleHeaderSort = (sortKey: string) => {
    const next = getNextSort(currentSort, sortKey);
    if (onSortChange) {
      onSortChange(next);
    } else {
      setInternalSort(next);
    }
  };

  // Drag and drop column reordering
  const handleDragStart = (e: React.DragEvent, id: ColumnId) => {
    setDraggedCol(id);
    e.dataTransfer.effectAllowed = "move";
    e.dataTransfer.setData("text/plain", id);
  };

  const handleDragOver = (e: React.DragEvent, id: ColumnId) => {
    e.preventDefault();
    e.dataTransfer.dropEffect = "move";
    if (dragOverCol !== id) setDragOverCol(id);
  };

  const handleDrop = (e: React.DragEvent, targetId: ColumnId) => {
    e.preventDefault();
    setDragOverCol(null);
    if (!draggedCol || draggedCol === targetId) {
      setDraggedCol(null);
      return;
    }
    setColumnOrder((prev) => {
      const current = [...prev];
      const fromIdx = current.indexOf(draggedCol);
      const toIdx = current.indexOf(targetId);
      if (fromIdx < 0 || toIdx < 0) return prev;
      current.splice(fromIdx, 1);
      current.splice(toIdx, 0, draggedCol);
      try {
        localStorage.setItem("audiovault_col_order", JSON.stringify(current));
      } catch {}
      return current;
    });
    setDraggedCol(null);
  };

  const handleDragEnd = () => {
    setDraggedCol(null);
    setDragOverCol(null);
  };

  // Column Resizer
  const handleResizeStart = (e: React.MouseEvent, colId: ColumnId) => {
    e.preventDefault();
    e.stopPropagation();
    const startX = e.clientX;
    const colDef = ALL_COLUMNS.find((c) => c.id === colId);
    const startWidth = colWidths[colId] || colDef?.defaultWidth || 150;
    const minWidth = colDef?.minWidth || 70;

    const onMouseMove = (moveEv: MouseEvent) => {
      const delta = moveEv.clientX - startX;
      const newWidth = Math.max(minWidth, Math.min(800, startWidth + delta));
      setColWidths((prev) => {
        const next = { ...prev, [colId]: newWidth };
        try {
          localStorage.setItem("audiovault_col_widths", JSON.stringify(next));
        } catch {}
        return next;
      });
    };

    const onMouseUp = () => {
      window.removeEventListener("mousemove", onMouseMove);
      window.removeEventListener("mouseup", onMouseUp);
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
    };

    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";
    window.addEventListener("mousemove", onMouseMove);
    window.addEventListener("mouseup", onMouseUp);
  };

  const handleResizeReset = (colId: ColumnId) => {
    const def = ALL_COLUMNS.find((c) => c.id === colId);
    if (!def) return;
    setColWidths((prev) => {
      const next = { ...prev, [colId]: def.defaultWidth };
      try {
        localStorage.setItem("audiovault_col_widths", JSON.stringify(next));
      } catch {}
      return next;
    });
  };

  // Move Column Up/Down in settings
  const moveColumn = (colId: ColumnId, direction: -1 | 1) => {
    setColumnOrder((prev) => {
      const list = [...prev];
      const idx = list.indexOf(colId);
      const targetIdx = idx + direction;
      if (targetIdx < 0 || targetIdx >= list.length) return prev;
      const [item] = list.splice(idx, 1);
      list.splice(targetIdx, 0, item);
      try {
        localStorage.setItem("audiovault_col_order", JSON.stringify(list));
      } catch {}
      return list;
    });
  };

  // Toggle Column Visibility
  const toggleColumnHide = (colId: ColumnId) => {
    if (colId === "title") return; // Title is mandatory
    setHiddenCols((prev) => {
      const next = prev.includes(colId)
        ? prev.filter((c) => c !== colId)
        : [...prev, colId];
      try {
        localStorage.setItem("audiovault_hidden_cols", JSON.stringify(next));
      } catch {}
      return next;
    });
  };

  // Reset all table customizations to defaults
  const resetAllToDefault = () => {
    setColumnOrder([...DEFAULT_ORDER]);
    setHiddenCols([]);
    const defWidths: Record<string, number> = {};
    for (const c of ALL_COLUMNS) defWidths[c.id] = c.defaultWidth;
    setColWidths(defWidths);
    updateCoverSize("medium");
    try {
      localStorage.removeItem("audiovault_col_order");
      localStorage.removeItem("audiovault_col_widths");
      localStorage.removeItem("audiovault_hidden_cols");
      localStorage.removeItem("audiovault_cover_size");
    } catch {}
  };

  // Compute visible columns and grid layout
  const visibleColumns = useMemo(() => {
    return columnOrder
      .filter((id) => !hiddenCols.includes(id))
      .map((id) => ALL_COLUMNS.find((c) => c.id === id)!)
      .filter(Boolean);
  }, [columnOrder, hiddenCols]);

  const hasSelection = Boolean(onToggleSelect);
  const gridTemplate = useMemo(() => {
    const parts = visibleColumns.map(
      (col) => `${colWidths[col.id] || col.defaultWidth}px`
    );
    return `${hasSelection ? "40px " : ""}${parts.join(" ")} 192px`;
  }, [visibleColumns, colWidths, hasSelection]);

  const totalWidth = useMemo(() => {
    const colsTotal = visibleColumns.reduce(
      (sum, col) => sum + (colWidths[col.id] || col.defaultWidth),
      0
    );
    // Gap: 12px between columns + 192px action column + row padding
    return colsTotal + visibleColumns.length * 12 + 212 + (hasSelection ? 40 : 0);
  }, [visibleColumns, colWidths, hasSelection]);

  const rowHeight = (isLargeText ? ROW_HEIGHT_BY_COVER_LARGE : ROW_HEIGHT_BY_COVER)[effectiveCoverSize];
  const gridRowHeight = isLargeText ? 330 : 280;

  // In-memory sort fallback when onSortChange is not provided (e.g. SearchPage)
  const displayBooks = useMemo(() => {
    if (onSortChange || !internalSort) return books;
    const list = [...books];
    const dir = internalSort.endsWith("_desc") ? -1 : 1;
    const key = internalSort.replace(/_(asc|desc)$/, "");
    list.sort((a, b) => {
      switch (key) {
        case "title":
          return dir * (a.title || "").localeCompare(b.title || "", "ru");
        case "author":
          return dir * (a.authors || "").localeCompare(b.authors || "", "ru");
        case "narrator":
          return dir * (a.narrators || "").localeCompare(b.narrators || "", "ru");
        case "duration":
          return dir * ((a.duration_ms || 0) - (b.duration_ms || 0));
        case "size":
          return dir * ((a.size || 0) - (b.size || 0));
        case "format":
          return dir * (a.format || "").localeCompare(b.format || "", "ru");
        case "volume":
          return dir * (a.volume_name || "").localeCompare(b.volume_name || "", "ru");
        default:
          return 0;
      }
    });
    return list;
  }, [books, onSortChange, internalSort]);

  return (
    <div
      ref={container}
      className={`book-list ${isLargeText ? "large-text" : ""}`}
      role="table"
      aria-label="Книги"
      aria-rowcount={displayBooks.length + 1}
      tabIndex={0}
      onKeyDown={(e) => {
        if ((e.target as HTMLElement).closest(".context-menu")) return;
        if ((e.target as HTMLElement).closest(".modal-backdrop")) return;
        if (["ArrowDown", "ArrowUp", "Home", "End"].includes(e.key)) {
          e.preventDefault();
          const next = Math.max(
            0,
            Math.min(
              displayBooks.length - 1,
              e.key === "Home"
                ? 0
                : e.key === "End"
                  ? displayBooks.length - 1
                  : focused + (e.key === "ArrowDown" ? 1 : -1)
            )
          );
          setFocused(next);
          const scroll = container.current?.querySelector(".virtual-list");
          if (scroll) {
            const top =
              view === "table"
                ? next * rowHeight
                : Math.floor(next / gridColumns) * gridRowHeight;
            if (
              top < scroll.scrollTop ||
              top + rowHeight > scroll.scrollTop + scroll.clientHeight
            )
              scroll.scrollTop = top;
          }
          requestAnimationFrame(() =>
            container.current
              ?.querySelector<HTMLButtonElement>(
                `[data-book-id="${displayBooks[next]?.id}"]`
              )
              ?.focus({ preventScroll: true })
          );
        }
        if (
          e.key === "Enter" &&
          e.target === e.currentTarget &&
          displayBooks[focused]
        )
          onSelectBook(displayBooks[focused].id);
      }}
    >
      {actionError && (
        <div className="notice" role="alert">
          {actionError}
          <button className="btn" onClick={() => setActionError("")}>
            Закрыть
          </button>
        </div>
      )}

      <VirtualList
        key={`${view}-${effectiveCoverSize}-${isLargeText ? "large" : "normal"}`}
        count={
          view === "table"
            ? displayBooks.length
            : Math.ceil(displayBooks.length / gridColumns)
        }
        rowHeight={view === "table" ? rowHeight : gridRowHeight}
        onEnd={onLoadMore}
        header={
          view === "table" && (
            <div
              className="book-columns table-head"
              role="row"
              style={{
                gridTemplateColumns: gridTemplate,
                minWidth: `${totalWidth}px`,
              }}
            >
              {hasSelection && (
                <div
                  className="col-header-select"
                  role="columnheader"
                  style={{ display: "flex", alignItems: "center", justifyContent: "center" }}
                >
                  <input
                    type="checkbox"
                    aria-label="Выбрать все книги на странице"
                    checked={
                      displayBooks.length > 0 &&
                      displayBooks.every((b) => selectedIds?.has(b.id))
                    }
                    onChange={onToggleSelectAll}
                  />
                </div>
              )}
              {visibleColumns.map((col) => {
                const dir = getSortDirection(currentSort, col.sortKey);
                const isDragging = draggedCol === col.id;
                const isOver = dragOverCol === col.id;

                return (
                  <div
                    key={col.id}
                    className={`col-header-cell ${isDragging ? "col-dragging" : ""} ${
                      isOver ? "col-drag-over" : ""
                    }`}
                    draggable
                    onDragStart={(e) => handleDragStart(e, col.id)}
                    onDragOver={(e) => handleDragOver(e, col.id)}
                    onDrop={(e) => handleDrop(e, col.id)}
                    onDragEnd={handleDragEnd}
                    role="columnheader"
                    aria-sort={
                      dir
                        ? dir === "asc"
                          ? "ascending"
                          : "descending"
                        : "none"
                    }
                  >
                    <button
                      type="button"
                      className={`col-sort-btn ${dir ? "is-sorted" : ""}`}
                      onClick={() => handleHeaderSort(col.sortKey)}
                      title={`Сортировать по «${col.label}» (${
                        dir === "asc" ? "по убыванию" : "по возрастанию"
                      })`}
                    >
                      <span className="col-label">{col.label}</span>
                      {dir === "asc" && (
                        <ArrowUp size={13} className="sort-arrow" />
                      )}
                      {dir === "desc" && (
                        <ArrowDown size={13} className="sort-arrow" />
                      )}
                      {!dir && (
                        <ArrowUpDown size={12} className="sort-arrow-idle" />
                      )}
                    </button>
                    <div
                      className="col-resizer"
                      onMouseDown={(e) => handleResizeStart(e, col.id)}
                      onDoubleClick={() => handleResizeReset(col.id)}
                      title="Потяните для изменения ширины (двойной клик — сброс)"
                    />
                  </div>
                );
              })}

              {/* Table Settings Button in Action Column */}
              <div className="col-header-action" role="columnheader">
                <button
                  type="button"
                  className="plain-icon col-settings-btn"
                  title="Настройка колонок и размера обложек"
                  aria-label="Настройка колонок и размера обложек"
                  onClick={() => setShowSettingsModal(true)}
                >
                  <Settings2 size={16} />
                </button>
              </div>
            </div>
          )
        }
        renderRow={(index) =>
          view === "grid" ? (
            <div
              className="shelf-row"
              style={{
                gridTemplateColumns: `repeat(${gridColumns}, minmax(0, 1fr))`,
              }}
            >
              {displayBooks
                .slice(index * gridColumns, (index + 1) * gridColumns)
                .map((book) => (
                  <div key={book.id} style={{ position: "relative" }}>
                    {hasSelection && (
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
                          aria-label={`Выбрать ${book.title}`}
                          checked={selectedIds?.has(book.id) ?? false}
                          onChange={() => onToggleSelect?.(book.id)}
                          onClick={(e) => e.stopPropagation()}
                        />
                      </div>
                    )}
                    <BookCard
                      book={book}
                      onClick={() => onSelectBook(book.id)}
                      onStatusChange={onBookStatusChange}
                    />
                  </div>
                ))}
            </div>
          ) : (
            (() => {
              const book = displayBooks[index];
              return (
                <div
                  role="row"
                  aria-rowindex={index + 2}
                  className={`book-columns book-row cover-${effectiveCoverSize} ${
                    selectedBookId === book.id ? "selected" : ""
                  }`}
                  style={{
                    gridTemplateColumns: gridTemplate,
                    minWidth: `${totalWidth}px`,
                    height: `${rowHeight}px`,
                  }}
                  onContextMenu={(e) => {
                    e.preventDefault();
                    setMenu({ book, x: e.clientX, y: e.clientY });
                  }}
                >
                  {hasSelection && (
                    <span
                      className="book-select-cell col-check"
                      role="cell"
                      onClick={(e) => {
                        if (e.target === e.currentTarget) {
                          onToggleSelect?.(book.id);
                        }
                      }}
                    >
                      <input
                        type="checkbox"
                        aria-label={`Выбрать ${book.title}`}
                        checked={selectedIds?.has(book.id) ?? false}
                        onChange={() => onToggleSelect?.(book.id)}
                        onClick={(e) => e.stopPropagation()}
                      />
                    </span>
                  )}
                  {visibleColumns.map((col) => {
                    switch (col.id) {
                      case "title":
                        return (
                          <span
                            key="title"
                            className="book-title-cell"
                            role="cell"
                          >
                            <BookCover
                              title={book.title}
                              thumb={book.cover_thumb}
                              size={effectiveCoverSize}
                            />
                            <span className="title-texts">
                              <button
                                data-book-id={book.id}
                                className="title-button"
                                onFocus={() => setFocused(index)}
                                onClick={() => onSelectBook(book.id)}
                                title={book.title}
                              >
                                {book.title || "Без названия"}
                              </button>
                              <small
                                title={book.series || ""}
                                style={{ display: "flex", alignItems: "center", gap: 6, flexWrap: "wrap" }}
                              >
                                <span>
                                  {book.series
                                    ? `${book.series}${
                                        book.series_index != null
                                          ? ` · ${book.series_index}`
                                          : ""
                                      }`
                                    : book.copies_count > 1
                                      ? `${book.copies_count} копии`
                                      : "Аудиокнига"}
                                </span>
                                {book.is_listened && (
                                  <span
                                    style={{
                                      fontSize: 10,
                                      padding: "1px 6px",
                                      borderRadius: 4,
                                      background: "rgba(34, 197, 94, 0.16)",
                                      color: "#4ade80",
                                      fontWeight: 600,
                                    }}
                                  >
                                    ✓ Прослушано
                                  </span>
                                )}
                                {book.is_marked_for_deletion && (
                                  <span
                                    style={{
                                      fontSize: 10,
                                      padding: "1px 6px",
                                      borderRadius: 4,
                                      background: "rgba(239, 68, 68, 0.16)",
                                      color: "#f87171",
                                      fontWeight: 600,
                                    }}
                                  >
                                    На удаление
                                  </span>
                                )}
                              </small>
                            </span>
                          </span>
                        );
                      case "author":
                        return (
                          <span
                            key="author"
                            role="cell"
                            className={!book.authors ? "muted" : ""}
                            title={book.authors}
                          >
                            {book.authors || "Не указан"}
                          </span>
                        );
                      case "narrator":
                        return (
                          <span
                            key="narrator"
                            role="cell"
                            className={!book.narrators ? "muted" : ""}
                            title={book.narrators}
                          >
                            {book.narrators || "—"}
                          </span>
                        );
                      case "duration":
                        return (
                          <span
                            key="duration"
                            role="cell"
                            className="numeric"
                          >
                            {book.duration_ms
                              ? formatDuration(book.duration_ms)
                              : "—"}
                          </span>
                        );
                      case "format":
                        return (
                          <span key="format" role="cell">
                            <span
                              className={`badge badge-format ${getFormatClass(
                                book.format
                              )}`}
                            >
                              {book.format || "—"}
                            </span>
                          </span>
                        );
                      case "size":
                        return (
                          <span
                            key="size"
                            role="cell"
                            className="numeric"
                          >
                            {formatBytes(book.size)}
                          </span>
                        );
                      case "volume":
                        return (
                          <span
                            key="volume"
                            role="cell"
                            className="book-storage"
                            title={`${book.volume_name} · ${
                              book.online ? "Доступна" : "Недоступна"
                            }`}
                          >
                            <StatusIndicator
                              online={book.online}
                              label={book.volume_name || "Архив"}
                            />
                            <small>
                              {book.online ? "Доступна" : "Недоступна"}
                            </small>
                          </span>
                        );
                      default:
                        return null;
                    }
                  })}
                  <span className="row-actions-group" role="cell">
                    <button
                      type="button"
                      className={`btn-listened-quick ${book.is_listened ? "is-active" : ""}`}
                      title={
                        book.is_listened
                          ? "Снять отметку «Прослушано»"
                          : "Отметить книгу как прослушанную"
                      }
                      onClick={async (e) => {
                        e.stopPropagation();
                        await api.setBookListened(book.id, !book.is_listened);
                        onBookStatusChange?.();
                      }}
                    >
                      <CheckCircle2 size={14} />
                      <span>Прослушано</span>
                    </button>
                    <button
                      type="button"
                      className={`btn-mark-del-quick ${book.is_marked_for_deletion ? "is-active" : ""}`}
                      title={
                        book.is_marked_for_deletion
                          ? "Снять пометку на удаление"
                          : "Пометить книгу на удаление"
                      }
                      onClick={async (e) => {
                        e.stopPropagation();
                        await api.setBookMarkedForDeletion(
                          book.id,
                          !book.is_marked_for_deletion
                        );
                        onBookStatusChange?.();
                      }}
                    >
                      <Trash2 size={14} />
                    </button>
                    <button
                      type="button"
                      className="plain-icon table-action-btn"
                      aria-label={`Действия: ${book.title}`}
                      title="Дополнительные действия"
                      onClick={(e) =>
                        setMenu({ book, x: e.clientX, y: e.clientY })
                      }
                    >
                      <MoreHorizontal size={16} />
                    </button>
                  </span>
                </div>
              );
            })()
          )
        }
      />

      {/* Context Menu */}
      {menu && (
        <ContextMenu x={menu.x} y={menu.y} onClose={closeMenu}>
          <p>{menu.book.title}</p>
          <button
            role="menuitem"
            onClick={() => {
              onSelectBook(menu.book.id);
              closeMenu();
            }}
          >
            <Info size={14} />
            Открыть сведения
          </button>
          <button
            role="menuitem"
            disabled={!menu.book.online}
            onClick={() => fileAction(false)}
          >
            <FolderOpen size={14} />
            Показать в Проводнике
          </button>
          <button role="menuitem" onClick={() => fileAction(true)}>
            <Copy size={14} />
            Скопировать путь
          </button>
          {onFindDuplicates && (
            <button
              role="menuitem"
              onClick={() => {
                onFindDuplicates();
                closeMenu();
              }}
            >
              <Files size={14} />
              Проверить дубликаты
            </button>
          )}
          <button
            role="menuitem"
            onClick={async () => {
              await api.setBookListened(menu.book.id, !menu.book.is_listened);
              onBookStatusChange?.();
              closeMenu();
            }}
          >
            <CheckCircle2 size={14} />
            {menu.book.is_listened ? "Снять метку «прослушано»" : "Отметить как прослушанную"}
          </button>
          <button
            role="menuitem"
            onClick={async () => {
              await api.setBookMarkedForDeletion(menu.book.id, !menu.book.is_marked_for_deletion);
              onBookStatusChange?.();
              closeMenu();
            }}
          >
            <Trash2 size={14} />
            {menu.book.is_marked_for_deletion ? "Снять статус кандидата" : "Кандидат на удаление"}
          </button>
        </ContextMenu>
      )}

      {/* Table & Columns Settings Modal */}
      {showSettingsModal && (
        <div
          className="modal-backdrop"
          onClick={() => setShowSettingsModal(false)}
          role="dialog"
          aria-modal="true"
          aria-label="Настройка таблицы"
        >
          <div
            className="modal table-settings-modal"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="table-settings-header">
              <div className="table-settings-header-title">
                <SlidersHorizontal size={20} className="header-icon" />
                <div>
                  <h3>Настройка колонок и отображения</h3>
                  <p>
                    Настройте размер обложек, порядок и видимость колонок таблицы
                  </p>
                </div>
              </div>
              <button
                type="button"
                className="plain-icon close-btn"
                aria-label="Закрыть"
                onClick={() => setShowSettingsModal(false)}
              >
                <X size={18} />
              </button>
            </div>

            <div className="table-settings-body">
              {/* Left Pane: Visual & Display Settings */}
              <div className="table-settings-pane table-settings-pane-left">
                {/* Cover Size Section */}
                <div className="settings-section">
                  <div className="section-header-row">
                    <label className="settings-section-title">
                      Размер иконки книги (обложки)
                    </label>
                  </div>
                  <p className="settings-section-desc">
                    Масштаб строк и обложек в таблице фонотеки
                  </p>
                  <div className="cover-size-buttons">
                    {(
                      [
                        {
                          id: "compact",
                          label: "S · Компактный",
                          sub: "34 px",
                        },
                        {
                          id: "medium",
                          label: "M · Стандартный",
                          sub: "48 px",
                        },
                        {
                          id: "large",
                          label: "L · Крупный",
                          sub: "68 px",
                        },
                        {
                          id: "xlarge",
                          label: "XL · Огромный",
                          sub: "90 px",
                        },
                      ] as const
                    ).map((opt) => (
                      <button
                        key={opt.id}
                        type="button"
                        className={`size-pill-btn ${
                          effectiveCoverSize === opt.id ? "active" : ""
                        }`}
                        onClick={() => updateCoverSize(opt.id)}
                      >
                        <span className="size-pill-name">{opt.label}</span>
                        <span className="size-pill-meta">{opt.sub}</span>
                      </button>
                    ))}
                  </div>
                </div>

                {/* Accessibility Font Size Section */}
                <div className="settings-section">
                  <div className="section-header-row">
                    <label className="settings-section-title">
                      Шрифт и доступность
                    </label>
                    <span className="section-hint">
                      {isLargeText ? "125% ВКЛ" : "100%"}
                    </span>
                  </div>
                  <p className="settings-section-desc">
                    Увеличенный шрифт интерфейса для людей с плохим зрением
                  </p>
                  <div style={{ marginTop: "4px" }}>
                    <button
                      type="button"
                      className={`size-pill-btn font-zoom-pill ${isLargeText ? "active" : ""}`}
                      onClick={toggleLargeText}
                    >
                      <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
                        <Type size={16} />
                        <span className="size-pill-name">Крупный шрифт</span>
                      </div>
                      <span className="size-pill-meta" style={{ margin: 0, fontWeight: 700 }}>
                        {isLargeText ? "Включён (125%)" : "Выключен (100%)"}
                      </span>
                    </button>
                  </div>
                </div>
              </div>

              {/* Right Pane: Columns Order & Visibility */}
              <div className="table-settings-pane table-settings-pane-right">
                <div className="settings-section">
                  <div className="section-header-row">
                    <label className="settings-section-title">
                      Колонки таблицы (порядок и видимость)
                    </label>
                    <span className="section-hint">
                      {visibleColumns.length} из {ALL_COLUMNS.length} включено
                    </span>
                  </div>
                  <p className="settings-section-desc">
                    Используйте стрелки ▲ ▼ для перемещения. Снимите отметку, чтобы скрыть колонку.
                  </p>

                  <div className="columns-manager-list">
                    {columnOrder.map((colId, idx) => {
                      const def = ALL_COLUMNS.find((c) => c.id === colId);
                      if (!def) return null;
                      const isHidden = hiddenCols.includes(colId);
                      const width = colWidths[colId] || def.defaultWidth;

                      return (
                        <div
                          key={colId}
                          className={`col-manager-item ${
                            isHidden ? "is-hidden" : ""
                          }`}
                        >
                          <div className="col-manager-left">
                            <span className="col-manager-num">{idx + 1}</span>
                            <label className="col-checkbox-label">
                              <input
                                type="checkbox"
                                checked={!isHidden}
                                disabled={!def.canHide}
                                onChange={() => toggleColumnHide(colId)}
                              />
                              <span className="col-name">{def.label}</span>
                              {!def.canHide && (
                                <span className="badge-tag">Обязательная</span>
                              )}
                            </label>
                          </div>

                          <div className="col-manager-right">
                            <span className="col-width-tag">{width} px</span>
                            <button
                              type="button"
                              className="plain-icon btn-col-move"
                              disabled={idx === 0}
                              onClick={() => moveColumn(colId, -1)}
                              title="Переместить выше / левее"
                              aria-label={`Переместить ${def.label} выше`}
                            >
                              <ArrowUp size={14} />
                            </button>
                            <button
                              type="button"
                              className="plain-icon btn-col-move"
                              disabled={idx === columnOrder.length - 1}
                              onClick={() => moveColumn(colId, 1)}
                              title="Переместить ниже / правее"
                              aria-label={`Переместить ${def.label} ниже`}
                            >
                              <ArrowDown size={14} />
                            </button>
                            <button
                              type="button"
                              className="plain-icon btn-col-reset"
                              onClick={() => handleResizeReset(colId)}
                              title="Сбросить ширину по умолчанию"
                              aria-label={`Сбросить ширину ${def.label}`}
                            >
                              <RotateCcw size={13} />
                            </button>
                          </div>
                        </div>
                      );
                    })}
                  </div>
                </div>
              </div>
            </div>

            <div className="table-settings-footer">
              <button
                type="button"
                className="btn btn-quiet"
                onClick={resetAllToDefault}
              >
                <RotateCcw size={14} />
                Сбросить по умолчанию
              </button>
              <button
                type="button"
                className="btn btn-primary"
                onClick={() => setShowSettingsModal(false)}
              >
                Готово
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
