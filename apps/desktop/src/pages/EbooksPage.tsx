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
  ArrowUp,
  ArrowDown,
  ArrowUpDown,
  Settings2,
  SlidersHorizontal,
  RotateCcw,
  MoreHorizontal,
  Info,
  Copy,
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
import { ContextMenu } from "../components/ContextMenu";
import { DeletionConfirmModal, type ItemToDelete } from "../components/DeletionConfirmModal";
import type { CoverSize } from "../components/BookList";

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

type EbookColumnId =
  | "title"
  | "author"
  | "series"
  | "format"
  | "size"
  | "year"
  | "volume";

interface EbookColumnDef {
  id: EbookColumnId;
  label: string;
  defaultWidth: number;
  minWidth: number;
  canHide: boolean;
  sortKey: string;
}

const ALL_EBOOK_COLUMNS: EbookColumnDef[] = [
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
    id: "series",
    label: "Серия",
    defaultWidth: 180,
    minWidth: 110,
    canHide: true,
    sortKey: "series",
  },
  {
    id: "format",
    label: "Формат",
    defaultWidth: 90,
    minWidth: 70,
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
    id: "year",
    label: "Год",
    defaultWidth: 80,
    minWidth: 65,
    canHide: true,
    sortKey: "year",
  },
  {
    id: "volume",
    label: "Накопитель",
    defaultWidth: 175,
    minWidth: 120,
    canHide: true,
    sortKey: "volume",
  },
];

const DEFAULT_EBOOK_ORDER: EbookColumnId[] = [
  "title",
  "author",
  "series",
  "format",
  "size",
  "year",
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

function loadEbookColOrder(): EbookColumnId[] {
  try {
    const raw = localStorage.getItem("audiovault_ebook_col_order");
    if (raw) {
      const parsed = JSON.parse(raw);
      if (Array.isArray(parsed)) {
        const valid = parsed.filter((id): id is EbookColumnId =>
          ALL_EBOOK_COLUMNS.some((c) => c.id === id),
        );
        const missing = DEFAULT_EBOOK_ORDER.filter((id) => !valid.includes(id));
        return [...valid, ...missing];
      }
    }
  } catch {}
  return [...DEFAULT_EBOOK_ORDER];
}

function loadEbookColWidths(): Record<string, number> {
  try {
    const raw = localStorage.getItem("audiovault_ebook_col_widths");
    if (raw) {
      const parsed = JSON.parse(raw);
      if (typeof parsed === "object" && parsed !== null) return parsed;
    }
  } catch {}
  const defaults: Record<string, number> = {};
  for (const c of ALL_EBOOK_COLUMNS) defaults[c.id] = c.defaultWidth;
  return defaults;
}

function loadEbookHiddenCols(): EbookColumnId[] {
  try {
    const raw = localStorage.getItem("audiovault_ebook_hidden_cols");
    if (raw) {
      const parsed = JSON.parse(raw);
      if (Array.isArray(parsed)) {
        return parsed.filter(
          (id): id is EbookColumnId =>
            id !== "title" && ALL_EBOOK_COLUMNS.some((c) => c.id === id),
        );
      }
    }
  } catch {}
  return ["year"];
}

function loadEbookCoverSize(): CoverSize {
  try {
    const raw = localStorage.getItem("audiovault_ebook_cover_size") || localStorage.getItem("audiovault_cover_size");
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

function getEbookSortDirection(
  activeSort: string,
  sortKey: string,
): "asc" | "desc" | null {
  if (!activeSort) return null;
  if (sortKey === "size" || sortKey === "year") {
    if (activeSort === sortKey || activeSort === `${sortKey}_desc`) return "desc";
    if (activeSort === `${sortKey}_asc`) return "asc";
    return null;
  }
  if (activeSort === sortKey || activeSort === `${sortKey}_asc`) return "asc";
  if (activeSort === `${sortKey}_desc`) return "desc";
  return null;
}

function getNextEbookSort(currentSort: string, sortKey: string): string {
  const dir = getEbookSortDirection(currentSort, sortKey);
  if (!dir) {
    if (sortKey === "size" || sortKey === "year") return `${sortKey}_desc`;
    return `${sortKey}_asc`;
  }
  return dir === "asc" ? `${sortKey}_desc` : `${sortKey}_asc`;
}

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
  const [sort, setSort] = useState("author_asc");
  const [view, setView] = useState<"table" | "grid">("table");
  const [error, setError] = useState("");
  const [columns, setColumns] = useState(5);
  const [showPaletteModal, setShowPaletteModal] = useState(false);
  const [selectedIds, setSelectedIds] = useState<Set<number>>(new Set());
  const [deleteModalOpen, setDeleteModalOpen] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [menu, setMenu] = useState<{ book: Ebook; x: number; y: number } | null>(null);

  // Table customization states (matching BookList)
  const [columnOrder, setColumnOrder] = useState<EbookColumnId[]>(loadEbookColOrder);
  const [colWidths, setColWidths] = useState<Record<string, number>>(loadEbookColWidths);
  const [hiddenCols, setHiddenCols] = useState<EbookColumnId[]>(loadEbookHiddenCols);
  const [coverSize, setCoverSize] = useState<CoverSize>(loadEbookCoverSize);
  const [showSettingsModal, setShowSettingsModal] = useState(false);
  const [draggedCol, setDraggedCol] = useState<EbookColumnId | null>(null);
  const [dragOverCol, setDragOverCol] = useState<EbookColumnId | null>(null);

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

  const updateCoverSize = (size: CoverSize) => {
    setCoverSize(size);
    try {
      localStorage.setItem("audiovault_ebook_cover_size", size);
    } catch {}
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
    return false;
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

  // Column sorting, drag-and-drop, and resize handlers
  const handleHeaderSort = (sortKey: string) => {
    setSort(getNextEbookSort(sort, sortKey));
  };

  const handleDragStart = (e: React.DragEvent, id: EbookColumnId) => {
    setDraggedCol(id);
    e.dataTransfer.effectAllowed = "move";
    e.dataTransfer.setData("text/plain", id);
  };

  const handleDragOver = (e: React.DragEvent, id: EbookColumnId) => {
    e.preventDefault();
    e.dataTransfer.dropEffect = "move";
    if (dragOverCol !== id) setDragOverCol(id);
  };

  const handleDrop = (e: React.DragEvent, targetId: EbookColumnId) => {
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
        localStorage.setItem("audiovault_ebook_col_order", JSON.stringify(current));
      } catch {}
      return current;
    });
    setDraggedCol(null);
  };

  const handleDragEnd = () => {
    setDraggedCol(null);
    setDragOverCol(null);
  };

  const handleResizeStart = (e: React.MouseEvent, colId: EbookColumnId) => {
    e.preventDefault();
    e.stopPropagation();
    const startX = e.clientX;
    const colDef = ALL_EBOOK_COLUMNS.find((c) => c.id === colId);
    const startWidth = colWidths[colId] || colDef?.defaultWidth || 150;
    const minWidth = colDef?.minWidth || 70;

    const onMouseMove = (moveEv: MouseEvent) => {
      const delta = moveEv.clientX - startX;
      const newWidth = Math.max(minWidth, Math.min(800, startWidth + delta));
      setColWidths((prev) => {
        const next = { ...prev, [colId]: newWidth };
        try {
          localStorage.setItem("audiovault_ebook_col_widths", JSON.stringify(next));
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

  const handleResizeReset = (colId: EbookColumnId) => {
    const def = ALL_EBOOK_COLUMNS.find((c) => c.id === colId);
    if (!def) return;
    setColWidths((prev) => {
      const next = { ...prev, [colId]: def.defaultWidth };
      try {
        localStorage.setItem("audiovault_ebook_col_widths", JSON.stringify(next));
      } catch {}
      return next;
    });
  };

  const moveColumn = (colId: EbookColumnId, direction: -1 | 1) => {
    setColumnOrder((prev) => {
      const list = [...prev];
      const idx = list.indexOf(colId);
      const targetIdx = idx + direction;
      if (targetIdx < 0 || targetIdx >= list.length) return prev;
      const [item] = list.splice(idx, 1);
      list.splice(targetIdx, 0, item);
      try {
        localStorage.setItem("audiovault_ebook_col_order", JSON.stringify(list));
      } catch {}
      return list;
    });
  };

  const toggleColumnHide = (colId: EbookColumnId) => {
    if (colId === "title") return;
    setHiddenCols((prev) => {
      const next = prev.includes(colId)
        ? prev.filter((c) => c !== colId)
        : [...prev, colId];
      try {
        localStorage.setItem("audiovault_ebook_hidden_cols", JSON.stringify(next));
      } catch {}
      return next;
    });
  };

  const resetAllToDefault = () => {
    setColumnOrder([...DEFAULT_EBOOK_ORDER]);
    setHiddenCols(["year"]);
    const defWidths: Record<string, number> = {};
    for (const c of ALL_EBOOK_COLUMNS) defWidths[c.id] = c.defaultWidth;
    setColWidths(defWidths);
    updateCoverSize("medium");
    try {
      localStorage.removeItem("audiovault_ebook_col_order");
      localStorage.removeItem("audiovault_ebook_col_widths");
      localStorage.removeItem("audiovault_ebook_hidden_cols");
      localStorage.removeItem("audiovault_ebook_cover_size");
    } catch {}
  };

  const visibleColumns = useMemo(() => {
    return columnOrder
      .filter((id) => !hiddenCols.includes(id))
      .map((id) => ALL_EBOOK_COLUMNS.find((c) => c.id === id)!)
      .filter(Boolean);
  }, [columnOrder, hiddenCols]);

  const gridTemplate = useMemo(() => {
    const parts = visibleColumns.map(
      (col) => `${colWidths[col.id] || col.defaultWidth}px`,
    );
    return `40px ${parts.join(" ")} 230px`;
  }, [visibleColumns, colWidths]);

  const totalWidth = useMemo(() => {
    const colsTotal = visibleColumns.reduce(
      (sum, col) => sum + (colWidths[col.id] || col.defaultWidth),
      0,
    );
    return colsTotal + visibleColumns.length * 12 + 280;
  }, [visibleColumns, colWidths]);

  const rowHeight = (largeText ? ROW_HEIGHT_BY_COVER_LARGE : ROW_HEIGHT_BY_COVER)[coverSize];
  const closeMenu = useCallback(() => setMenu(null), []);

  const rowActions = (b: Ebook) => (
    <span className="row-actions-group" role="cell">
      <button
        type="button"
        className={`btn-listened-quick ${b.is_read ? "is-active" : ""}`}
        title={b.is_read ? "Снять отметку «Прочитано»" : "Отметить книгу как прочитанную"}
        onClick={(e) => {
          e.stopPropagation();
          handleToggleRead(b);
        }}
      >
        <CheckCircle2 size={14} />
        <span>Прочитано</span>
      </button>
      <button
        type="button"
        className={`btn-mark-del-quick ${b.is_marked_for_deletion ? "is-active" : ""}`}
        title={
          b.is_marked_for_deletion
            ? "Снять пометку на удаление"
            : "Пометить книгу на удаление"
        }
        onClick={(e) => {
          e.stopPropagation();
          handleToggleCandidate(b);
        }}
      >
        <Trash2 size={14} />
      </button>
      <button
        type="button"
        className="action-btn action-open"
        title="Открыть книгу в читалке"
        aria-label={`Открыть ${b.title || b.file_name}`}
        disabled={!b.volume_online || !b.full_path}
        onClick={(e) => {
          e.stopPropagation();
          if (b.full_path) onOpenFile(b.full_path);
        }}
      >
        <ExternalLink size={15} />
      </button>
      <button
        type="button"
        className="plain-icon table-action-btn"
        aria-label={`Действия: ${b.title || b.file_name}`}
        title="Дополнительные действия"
        onClick={(e) => {
          e.stopPropagation();
          setMenu({ book: b, x: e.clientX, y: e.clientY });
        }}
      >
        <MoreHorizontal size={16} />
      </button>
    </span>
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
              ["author_asc", "По автору (А → Я)"],
              ["author_desc", "По автору (Я → А)"],
              ["title_asc", "По названию (А → Я)"],
              ["title_desc", "По названию (Я → А)"],
              ["series_asc", "По серии"],
              ["year_desc", "По году (новые)"],
              ["year_asc", "По году (старые)"],
              ["size_desc", "По размеру (крупные)"],
              ["size_asc", "По размеру (компактные)"],
              ["format_asc", "По формату"],
              ["volume_asc", "По накопителю"],
              ["added_desc", "По дате добавления"],
            ].map(([id, label]) => (
              <option key={id} value={id}>
                {label}
              </option>
            ))}
          </select>

          {view === "table" && (
            <div
              className="segmented cover-size-toolbar"
              role="group"
              aria-label="Размер обложек в списке"
              title="Размер обложек в списке (S / M / L / XL)"
            >
              {(
                [
                  ["compact", "S"],
                  ["medium", "M"],
                  ["large", "L"],
                  ["xlarge", "XL"],
                ] as const
              ).map(([sz, label]) => (
                <button
                  key={sz}
                  type="button"
                  aria-pressed={coverSize === sz}
                  onClick={() => updateCoverSize(sz)}
                >
                  {label}
                </button>
              ))}
            </div>
          )}

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
          <button
            type="button"
            className="btn btn-secondary"
            onClick={() => setShowSettingsModal(true)}
            title="Настройка колонок и отображения таблицы"
            style={{ minHeight: "34px", padding: "0 10px" }}
          >
            <Settings2 size={16} />
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
        <div className={`book-list ${largeText ? "large-text" : ""}`} role="table" aria-label="Электронные книги">
          <VirtualList
            key={`${viewMode}-${view}-${sort}-${selectedFormats.join(",")}-${debounced}-${largeText}-${coverSize}`}
            count={
              view === "table" ? books.length : Math.ceil(books.length / columns)
            }
            rowHeight={view === "table" ? rowHeight : (largeText ? 380 : 330)}
            onEnd={books.length < total ? more : undefined}
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
                  <div
                    className="col-header-select"
                    role="columnheader"
                    style={{ display: "flex", alignItems: "center", justifyContent: "center" }}
                  >
                    <input
                      type="checkbox"
                      aria-label="Выбрать все книги на странице"
                      checked={books.length > 0 && books.every((b) => selectedIds.has(b.id))}
                      onChange={toggleSelectAll}
                    />
                  </div>
                  {visibleColumns.map((col) => {
                    const dir = getEbookSortDirection(sort, col.sortKey);
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
                          dir ? (dir === "asc" ? "ascending" : "descending") : "none"
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
                          {dir === "asc" && <ArrowUp size={13} className="sort-arrow" />}
                          {dir === "desc" && <ArrowDown size={13} className="sort-arrow" />}
                          {!dir && <ArrowUpDown size={12} className="sort-arrow-idle" />}
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
              view === "table" ? (
                (() => {
                  const b = books[index];
                  const isSelected = selectedIds.has(b.id);
                  return (
                    <div
                      role="row"
                      className={`book-columns book-row cover-${coverSize} ${isSelected ? "selected" : ""}`}
                      style={{
                        gridTemplateColumns: gridTemplate,
                        minWidth: `${totalWidth}px`,
                        height: `${rowHeight}px`,
                      }}
                      onContextMenu={(e) => {
                        e.preventDefault();
                        setMenu({ book: b, x: e.clientX, y: e.clientY });
                      }}
                    >
                      <span
                        className="book-select-cell col-check"
                        role="cell"
                        onClick={(e) => {
                          if (e.target === e.currentTarget) toggleSelect(b.id);
                        }}
                      >
                        <input
                          type="checkbox"
                          aria-label={`Выбрать ${b.title || b.file_name}`}
                          checked={isSelected}
                          onChange={() => toggleSelect(b.id)}
                          onClick={(e) => e.stopPropagation()}
                        />
                      </span>

                      {visibleColumns.map((col) => {
                        switch (col.id) {
                          case "title":
                            return (
                              <span key="title" className="book-title-cell" role="cell">
                                <BookCover
                                  title={b.title || b.file_name}
                                  thumb={b.cover_thumb}
                                  format={b.format}
                                  author={b.authors}
                                  size={coverSize}
                                />
                                <span className="title-texts">
                                  <button
                                    className="title-button"
                                    title={b.title || b.file_name}
                                    onClick={() => onSelectEbook(b.id)}
                                  >
                                    {b.title || b.file_name}
                                  </button>
                                  <small style={{ display: "flex", alignItems: "center", gap: 6, flexWrap: "wrap" }}>
                                    <span>
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
                                    </span>
                                    {b.is_read && (
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
                                        ✓ Прочитано
                                      </span>
                                    )}
                                    {b.is_marked_for_deletion && (
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
                                className={!b.authors ? "muted" : ""}
                                title={b.authors}
                              >
                                {b.authors || "Не указан"}
                              </span>
                            );
                          case "series":
                            return (
                              <span
                                key="series"
                                role="cell"
                                className={!b.series ? "muted" : ""}
                                title={b.series || ""}
                              >
                                {b.series
                                  ? `${b.series}${b.series_index != null ? ` #${b.series_index}` : ""}`
                                  : "—"}
                              </span>
                            );
                          case "format":
                            return (
                              <span key="format" role="cell">
                                <span className={`badge badge-format ${getFormatClass(b.format)}`}>
                                  {b.format.toUpperCase()}
                                </span>
                              </span>
                            );
                          case "size":
                            return (
                              <span key="size" role="cell" className="numeric">
                                {formatBytes(b.size)}
                              </span>
                            );
                          case "year":
                            return (
                              <span key="year" role="cell" className="numeric">
                                {b.year ? `${b.year}` : "—"}
                              </span>
                            );
                          case "volume":
                            return (
                              <span key="volume" role="cell" className="book-storage">
                                <StatusIndicator
                                  online={b.volume_online}
                                  label={b.volume_name}
                                />
                                <small style={{ color: b.volume_online ? "#86efac" : "#fca5a5", fontWeight: 650 }}>
                                  {b.volume_online ? "Подключён" : "Отключён"}
                                </small>
                              </span>
                            );
                          default:
                            return null;
                        }
                      })}
                      {rowActions(b)}
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
                          onContextMenu={(e) => {
                            e.preventDefault();
                            setMenu({ book: b, x: e.clientX, y: e.clientY });
                          }}
                        >
                          <div
                            style={{
                              position: "absolute",
                              top: 8,
                              left: 8,
                              zIndex: 10,
                              background: "rgba(0,0,0,0.6)",
                              borderRadius: 6,
                              padding: "4px",
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
                            {rowActions(b)}
                          </div>
                        </div>
                      );
                    })}
                </div>
              )
            }
          />
        </div>
      )}

      {/* Context Menu */}
      {menu && (
        <ContextMenu x={menu.x} y={menu.y} onClose={closeMenu}>
          <p>{menu.book.title || menu.book.file_name}</p>
          <button
            role="menuitem"
            onClick={() => {
              onSelectEbook(menu.book.id);
              closeMenu();
            }}
          >
            <Info size={14} />
            Открыть сведения
          </button>
          <button
            role="menuitem"
            disabled={!menu.book.volume_online || !menu.book.full_path}
            onClick={() => {
              if (menu.book.full_path) onOpenFile(menu.book.full_path);
              closeMenu();
            }}
          >
            <ExternalLink size={14} />
            Открыть книгу в читалке
          </button>
          <button
            role="menuitem"
            disabled={!menu.book.volume_online || !menu.book.full_path}
            onClick={() => {
              if (menu.book.full_path) onRevealFolder(menu.book.full_path);
              closeMenu();
            }}
          >
            <FolderOpen size={14} />
            Показать в Проводнике
          </button>
          <button
            role="menuitem"
            onClick={async () => {
              const path = menu.book.full_path || menu.book.rel_path;
              if (path) await navigator.clipboard.writeText(path);
              closeMenu();
            }}
          >
            <Copy size={14} />
            Скопировать путь
          </button>
          <button
            role="menuitem"
            onClick={() => {
              handleToggleRead(menu.book);
              closeMenu();
            }}
          >
            <CheckCircle2 size={14} />
            {menu.book.is_read ? "Снять метку «прочитано»" : "Отметить как прочитанную"}
          </button>
          <button
            role="menuitem"
            onClick={() => {
              handleToggleCandidate(menu.book);
              closeMenu();
            }}
          >
            <Trash2 size={14} />
            {menu.book.is_marked_for_deletion ? "Снять статус кандидата" : "Кандидат на удаление"}
          </button>
        </ContextMenu>
      )}

      {/* Table & Columns Settings Modal (shared fixed layout with BookList) */}
      {showSettingsModal && (
        <div
          className="modal-backdrop"
          onClick={() => setShowSettingsModal(false)}
          role="dialog"
          aria-modal="true"
          aria-label="Настройка таблицы электронных книг"
        >
          <div
            className="modal table-settings-modal"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="table-settings-header">
              <div className="table-settings-header-title">
                <SlidersHorizontal size={20} className="header-icon" />
                <div>
                  <h3>Настройка таблицы электронных книг</h3>
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
              <div className="table-settings-pane table-settings-pane-left">
                <div className="settings-section">
                  <div className="section-header-row">
                    <label className="settings-section-title">
                      Размер иконки книги (обложки)
                    </label>
                  </div>
                  <p className="settings-section-desc">
                    Масштаб строк и обложек в таблице электронных книг
                  </p>
                  <div className="cover-size-buttons">
                    {(
                      [
                        { id: "compact", label: "S · Компактный", sub: "34 px" },
                        { id: "medium", label: "M · Стандартный", sub: "48 px" },
                        { id: "large", label: "L · Крупный", sub: "68 px" },
                        { id: "xlarge", label: "XL · Огромный", sub: "90 px" },
                      ] as const
                    ).map((opt) => (
                      <button
                        key={opt.id}
                        type="button"
                        className={`size-pill-btn ${coverSize === opt.id ? "active" : ""}`}
                        onClick={() => updateCoverSize(opt.id)}
                      >
                        <span className="size-pill-name">{opt.label}</span>
                        <span className="size-pill-meta">{opt.sub}</span>
                      </button>
                    ))}
                  </div>
                </div>

                <div className="settings-section">
                  <div className="section-header-row">
                    <label className="settings-section-title">
                      Шрифт и доступность
                    </label>
                    <span className="section-hint">
                      {largeText ? "125% ВКЛ" : "100%"}
                    </span>
                  </div>
                  <p className="settings-section-desc">
                    Увеличенный шрифт интерфейса для комфортного чтения
                  </p>
                  <div style={{ marginTop: "4px" }}>
                    <button
                      type="button"
                      className={`size-pill-btn font-zoom-pill ${largeText ? "active" : ""}`}
                      onClick={toggleLargeText}
                    >
                      <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
                        <Type size={16} />
                        <span className="size-pill-name">Крупный шрифт</span>
                      </div>
                      <span className="size-pill-meta" style={{ margin: 0, fontWeight: 700 }}>
                        {largeText ? "Включён (125%)" : "Выключен (100%)"}
                      </span>
                    </button>
                  </div>
                </div>
              </div>

              <div className="table-settings-pane table-settings-pane-right">
                <div className="settings-section">
                  <div className="section-header-row">
                    <label className="settings-section-title">
                      Колонки таблицы (порядок и видимость)
                    </label>
                    <span className="section-hint">
                      {visibleColumns.length} из {ALL_EBOOK_COLUMNS.length} включено
                    </span>
                  </div>
                  <p className="settings-section-desc">
                    Используйте стрелки ▲ ▼ для перемещения. Снимите отметку, чтобы скрыть колонку.
                  </p>

                  <div className="columns-manager-list">
                    {columnOrder.map((colId, idx) => {
                      const def = ALL_EBOOK_COLUMNS.find((c) => c.id === colId);
                      if (!def) return null;
                      const isHidden = hiddenCols.includes(colId);
                      const width = colWidths[colId] || def.defaultWidth;

                      return (
                        <div
                          key={colId}
                          className={`col-manager-item ${isHidden ? "is-hidden" : ""}`}
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
                            >
                              <ArrowUp size={14} />
                            </button>
                            <button
                              type="button"
                              className="plain-icon btn-col-move"
                              disabled={idx === columnOrder.length - 1}
                              onClick={() => moveColumn(colId, 1)}
                              title="Переместить ниже / правее"
                            >
                              <ArrowDown size={14} />
                            </button>
                            <button
                              type="button"
                              className="plain-icon btn-col-reset"
                              onClick={() => handleResizeReset(colId)}
                              title="Сбросить ширину по умолчанию"
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
