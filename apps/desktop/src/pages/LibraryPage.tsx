import { useMemo, useState } from "react";
import {
  List,
  LayoutGrid,
  SlidersHorizontal,
  X,
  Type,
  Headphones,
  CheckCircle2,
  Trash2,
  CheckSquare,
} from "lucide-react";
import type { Book, Volume } from "../api/types";
import { api } from "../api/client";
import { BookList } from "../components/BookList";
import { EmptyState, LoadingRows } from "../components/Primitives";
import { DeletionConfirmModal, type ItemToDelete } from "../components/DeletionConfirmModal";

interface Props {
  books: Book[];
  onSelectBook: (id: number) => void;
  activeFilter: string;
  onFilterChange: (f: string) => void;
  activeSort: string;
  onSortChange: (s: string) => void;
  loading?: boolean;
  searchQuery?: string;
  onAddFolder?: () => void;
  volumes?: Volume[];
  selectedBookId?: number | null;
  onFindDuplicates?: () => void;
  volumeFilter?: string;
  onVolumeFilterChange?: (v: string) => void;
  onLoadMore?: () => void;
  hasMore?: boolean;
  onRefresh?: () => void;
}

export function LibraryPage({
  books,
  onSelectBook,
  activeFilter,
  onFilterChange,
  activeSort,
  onSortChange,
  loading,
  searchQuery,
  onAddFolder,
  volumes = [],
  selectedBookId,
  onFindDuplicates,
  volumeFilter = "",
  onVolumeFilterChange,
  onLoadMore,
  hasMore,
  onRefresh,
}: Props) {
  const [view, setView] = useState<"table" | "grid">("table");
  const [filtersOpen, setFiltersOpen] = useState(false);
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

  const visible = useMemo(
    () =>
      books.filter(
        (b) =>
          !volumeFilter ||
          (activeFilter === "online"
            ? b.online
            : activeFilter === "offline"
              ? !b.online
              : activeFilter === "multiple"
                ? b.copies_count > 1
                : true),
      ),
    [books, volumeFilter, activeFilter],
  );

  const filterCount =
    Number(activeFilter !== "all" && activeFilter !== "listened" && activeFilter !== "candidates") +
    Number(!!volumeFilter);

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
    if (visible.length > 0 && visible.every((b) => selectedIds.has(b.id))) {
      setSelectedIds(new Set());
    } else {
      setSelectedIds(new Set(visible.map((b) => b.id)));
    }
  };

  // Bulk actions
  const handleBulkMarkListened = async (listened: boolean) => {
    const ids = Array.from(selectedIds);
    try {
      await Promise.all(ids.map((id) => api.setBookListened(id, listened)));
      setSelectedIds(new Set());
      onRefresh?.();
    } catch (e) {
      console.error("handleBulkMarkListened error:", e);
    }
  };

  const handleBulkMarkCandidate = async (candidate: boolean) => {
    const ids = Array.from(selectedIds);
    try {
      await api.setMultipleBooksMarkedForDeletion(ids, candidate);
      setSelectedIds(new Set());
      onRefresh?.();
    } catch (e) {
      console.error("handleBulkMarkCandidate error:", e);
    }
  };

  const handleConfirmDeletion = async () => {
    const ids = Array.from(selectedIds);
    setDeleting(true);
    try {
      await api.executeDeletion(ids, []);
      setSelectedIds(new Set());
      setDeleteModalOpen(false);
      onRefresh?.();
    } catch (e) {
      console.error("executeDeletion error:", e);
    } finally {
      setDeleting(false);
    }
  };

  const selectedItemsToDelete: ItemToDelete[] = useMemo(() => {
    return visible
      .filter((b) => selectedIds.has(b.id))
      .map((b) => ({
        id: b.id,
        type: "book" as const,
        title: b.title,
        authors: b.authors,
        volumeName: b.volume_name,
        volumeOnline: b.online,
        size: b.size,
      }));
  }, [visible, selectedIds]);

  return (
    <div className={`collection-page ${largeText ? "large-text" : ""}`}>
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
            className={`btn ${activeFilter === "all" ? "btn-primary" : "btn-secondary"}`}
            onClick={() => {
              onFilterChange("all");
              setSelectedIds(new Set());
            }}
            style={{ fontSize: "13px", height: "34px" }}
          >
            <Headphones size={15} />
            <span>Фонотека (каталог)</span>
          </button>
          <button
            className={`btn ${activeFilter === "listened" ? "btn-primary" : "btn-secondary"}`}
            onClick={() => {
              onFilterChange("listened");
              setSelectedIds(new Set());
            }}
            style={{ fontSize: "13px", height: "34px" }}
          >
            <CheckCircle2 size={15} />
            <span>Прослушано</span>
          </button>
          <button
            className={`btn ${activeFilter === "candidates" ? "btn-danger" : "btn-secondary"}`}
            onClick={() => {
              onFilterChange("candidates");
              setSelectedIds(new Set());
            }}
            style={{ fontSize: "13px", height: "34px" }}
          >
            <Trash2 size={15} />
            <span>Кандидаты на удаление</span>
          </button>
        </div>

        {activeFilter === "candidates" && visible.length > 0 && (
          <button
            className="btn btn-danger"
            onClick={() => {
              setSelectedIds(new Set(visible.map((b) => b.id)));
              setDeleteModalOpen(true);
            }}
            style={{ fontSize: "13px", height: "34px", gap: 7 }}
          >
            <Trash2 size={15} />
            <span>Удалить все кандидаты ({visible.length})</span>
          </button>
        )}
      </div>

      <div className="toolbar" style={{ marginTop: 4 }}>
        <div className="toolbar-left">
          <select
            aria-label="Фильтр книг"
            value={activeFilter}
            onChange={(e) => {
              onFilterChange(e.target.value);
              setSelectedIds(new Set());
              if (
                e.target.value === "duplicates" ||
                e.target.value === "multiple"
              )
                onVolumeFilterChange?.("");
            }}
          >
            <option value="all">Все активные книги</option>
            <option value="listened">Прослушанные книги</option>
            <option value="candidates">Кандидаты на удаление</option>
            <option value="online">Доступные книги</option>
            <option value="offline">Недоступные книги</option>
            <option value="multiple">Несколько записей</option>
            <option value="duplicates">Возможные дубликаты</option>
          </select>
          <button
            className={`btn btn-quiet ${filtersOpen ? "is-active" : ""}`}
            onClick={() => setFiltersOpen(!filtersOpen)}
            aria-expanded={filtersOpen}
          >
            <SlidersHorizontal size={14} />
            Фильтры
            {filterCount > 0 && (
              <small className="filter-count">{filterCount}</small>
            )}
          </button>
          {filterCount > 0 && (
            <button
              className="plain-icon"
              title="Сбросить фильтры"
              onClick={() => {
                onFilterChange("all");
                onVolumeFilterChange?.("");
              }}
            >
              <X size={14} />
            </button>
          )}
        </div>
        <div className="toolbar-right">
          <select
            aria-label="Сортировка книг"
            value={activeSort}
            onChange={(e) => onSortChange(e.target.value)}
          >
            <option value="author_asc">Автор (А → Я)</option>
            <option value="author_desc">Автор (Я → А)</option>
            <option value="title_asc">Название (А → Я)</option>
            <option value="title_desc">Название (Я → А)</option>
            <option value="narrator_asc">Чтец (А → Я)</option>
            <option value="narrator_desc">Чтец (Я → А)</option>
            <option value="duration_desc">Время (сначала длинные)</option>
            <option value="duration_asc">Время (сначала короткие)</option>
            <option value="size_desc">Размер (сначала большие)</option>
            <option value="size_asc">Размер (сначала малые)</option>
            <option value="format_asc">Формат (A → Z)</option>
            <option value="format_desc">Формат (Z → A)</option>
            <option value="volume_asc">Накопитель (А → Я)</option>
            <option value="volume_desc">Накопитель (Я → А)</option>
            <option value="recent">Недавно добавленные</option>
          </select>
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
          <div className="segmented" aria-label="Вид коллекции">
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
              <LayoutGrid size={15} />
            </button>
          </div>
        </div>
      </div>
      {filtersOpen && (
        <div className="filter-panel">
          <label>
            Накопитель
            <select
              value={volumeFilter}
              onChange={(e) => {
                onVolumeFilterChange?.(e.target.value);
                if (
                  activeFilter === "duplicates" ||
                  activeFilter === "multiple"
                )
                  onFilterChange("all");
              }}
            >
              <option value="">Все накопители</option>
              {volumes.map((v) => (
                <option key={v.id} value={String(v.id)}>
                  {v.label}
                </option>
              ))}
            </select>
          </label>
        </div>
      )}
      {loading && books.length === 0 ? (
        <LoadingRows />
      ) : visible.length ? (
        <BookList
          key={`${activeFilter}-${activeSort}-${searchQuery}-${volumeFilter}-${largeText}`}
          books={visible}
          view={view}
          onSelectBook={onSelectBook}
          selectedBookId={selectedBookId}
          onFindDuplicates={onFindDuplicates}
          onLoadMore={hasMore ? onLoadMore : undefined}
          activeSort={activeSort}
          onSortChange={onSortChange}
          largeText={largeText}
          onToggleLargeText={toggleLargeText}
          selectedIds={selectedIds}
          onToggleSelect={toggleSelect}
          onToggleSelectAll={toggleSelectAll}
          onBookStatusChange={onRefresh}
        />
      ) : (
        <EmptyState
          title={
            activeFilter === "listened"
              ? "Нет прослушанных аудиокниг"
              : activeFilter === "candidates"
              ? "Нет кандидатов на удаление"
              : searchQuery || filterCount
              ? "Ничего не найдено"
              : "Ваша фонотека начинается здесь"
          }
          description={
            activeFilter === "listened"
              ? "Отмечайте аудиокниги как прослушанные через контекстное меню или карточку книги."
              : activeFilter === "candidates"
              ? "Ненужные аудиокниги можно пометить как кандидаты на удаление для последующей пакетной очистки."
              : searchQuery || filterCount
              ? "Попробуйте другое название, автора или измените фильтры."
              : "Добавьте папку с аудиокнигами. Каталог сохранится даже после отключения диска."
          }
          action={
            !searchQuery &&
            !filterCount &&
            activeFilter === "all" && (
              <button className="btn btn-primary" onClick={onAddFolder}>
                Добавить папку
              </button>
            )
          }
        />
      )}

      {/* Floating Bulk Action Bar */}
      {selectedIds.size > 0 && (
        <div className="floating-bulk-bar">
          <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
            <CheckSquare size={16} color="var(--accent-hover)" />
            <strong style={{ fontSize: "13.5px" }}>Выбрано аудиокниг: {selectedIds.size}</strong>
          </div>

          <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
            {activeFilter !== "listened" ? (
              <button
                className="btn btn-secondary"
                onClick={() => handleBulkMarkListened(true)}
                style={{ fontSize: "12.5px", gap: "6px" }}
              >
                <CheckCircle2 size={14} color="#86efac" />
                <span>В прослушанное</span>
              </button>
            ) : (
              <button
                className="btn btn-secondary"
                onClick={() => handleBulkMarkListened(false)}
                style={{ fontSize: "12.5px", gap: "6px" }}
              >
                <span>Вернуть в фонотеку</span>
              </button>
            )}

            {activeFilter !== "candidates" ? (
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

      {/* Deletion confirm modal */}
      <DeletionConfirmModal
        isOpen={deleteModalOpen}
        items={selectedItemsToDelete}
        busy={deleting}
        onConfirm={handleConfirmDeletion}
        onCancel={() => setDeleteModalOpen(false)}
      />

      <div className="collection-footer">
        <span>
          {loading
            ? "Обновление каталога…"
            : `${visible.length.toLocaleString("ru-RU")} книг${hasMore ? " загружено" : ""}`}
          {volumeFilter &&
            ` · ${volumes.find((v) => String(v.id) === volumeFilter)?.label || "Накопитель"}`}
        </span>
        {hasMore ? (
          <button
            className="btn btn-quiet"
            onClick={onLoadMore}
            disabled={loading}
          >
            Загрузить ещё
          </button>
        ) : (
          <span>↑ ↓ выбор · Enter сведения · Ctrl K поиск</span>
        )}
      </div>
    </div>
  );
}
