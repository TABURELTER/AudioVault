import { useMemo, useState } from "react";
import { List, LayoutGrid, SlidersHorizontal, X, Type } from "lucide-react";
import type { Book, Volume } from "../api/types";
import { BookList } from "../components/BookList";
import { EmptyState, LoadingRows } from "../components/Primitives";
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
}: Props) {
  const [view, setView] = useState<"table" | "grid">("table");
  const [filtersOpen, setFiltersOpen] = useState(false);
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
  const filterCount = Number(activeFilter !== "all") + Number(!!volumeFilter);
  return (
    <div className={`collection-page ${largeText ? "large-text" : ""}`}>
      <div className="toolbar">
        <div className="toolbar-left">
          <select
            aria-label="Доступность книг"
            value={activeFilter}
            onChange={(e) => {
              onFilterChange(e.target.value);
              if (
                e.target.value === "duplicates" ||
                e.target.value === "multiple"
              )
                onVolumeFilterChange?.("");
            }}
          >
            <option value="all">Все аудиокниги</option>
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
            {activeSort === "author" && <option value="author">Автор (А → Я)</option>}
            {activeSort === "title" && <option value="title">Название (А → Я)</option>}
            {activeSort === "duration" && <option value="duration">Время (сначала длинные)</option>}
            {activeSort === "size" && <option value="size">Размер (сначала большие)</option>}
            {activeSort === "narrator" && <option value="narrator">Чтец (А → Я)</option>}
            {activeSort === "format" && <option value="format">Формат (A → Z)</option>}
            {activeSort === "volume" && <option value="volume">Накопитель (А → Я)</option>}
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
        />
      ) : (
        <EmptyState
          title={
            searchQuery || filterCount
              ? "Ничего не найдено"
              : "Ваша фонотека начинается здесь"
          }
          description={
            searchQuery || filterCount
              ? "Попробуйте другое название, автора или измените фильтры."
              : "Добавьте папку с аудиокнигами. Каталог сохранится даже после отключения диска."
          }
          action={
            !searchQuery &&
            !filterCount && (
              <button className="btn btn-primary" onClick={onAddFolder}>
                Добавить папку
              </button>
            )
          }
        />
      )}
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
