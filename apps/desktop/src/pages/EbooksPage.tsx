import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { FolderOpen, ExternalLink, List, LayoutGrid, Type, Palette, X } from "lucide-react";
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

interface Props {
  catalogRevision?: number;
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
  onSelectEbook,
  onRevealFolder,
  onOpenFile,
}: Props) {
  const [books, setBooks] = useState<Ebook[]>([]);
  const [total, setTotal] = useState(0);
  const [stats, setStats] = useState<EbookStats | null>(null);
  const [loading, setLoading] = useState(true);
  const [query, setQuery] = useState("");
  const [debounced, setDebounced] = useState("");
  const [format, setFormat] = useState("all");
  const [sort, setSort] = useState("author");
  const [view, setView] = useState<"table" | "grid">("table");
  const [error, setError] = useState("");
  const [columns, setColumns] = useState(5);
  const [showPaletteModal, setShowPaletteModal] = useState(false);
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
          format,
          sort,
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
    [debounced, format, sort],
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

  const actions = (b: Ebook) => (
    <div className="storage-actions">
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
      <div className="toolbar" style={{ flexWrap: "wrap", gap: "12px" }}>
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
              const isActive = format === f.id;
              const fmtCls = f.id !== "all" ? getFormatClass(f.id === "archives" ? "zip" : f.id) : "";
              const count = getTabCount(f.id);
              return (
                <button
                  key={f.id}
                  role="tab"
                  aria-selected={isActive}
                  className={`format-chip ${isActive ? `active ${fmtCls}` : ""}`}
                  onClick={() => setFormat(f.id)}
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
                  Каждый формат имеет свой оттенок переплёта для мгновенного визуального распознавания на книжной полке:
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
                        setFormat(item.id);
                        setShowPaletteModal(false);
                      }}
                      title={`Нажмите, чтобы отфильтровать по ${item.name}`}
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
            query || format !== "all"
              ? "Ничего не найдено"
              : "Электронных книг пока нет"
          }
          description={
            query || format !== "all"
              ? "Попробуйте другое название или выберите другой формат."
              : "Добавьте папки с книгами в разделе «Хранилища» и запустите сканирование."
          }
        />
      ) : (
        <VirtualList
          key={`${view}-${sort}-${format}-${debounced}-${largeText}`}
          count={
            view === "table" ? books.length : Math.ceil(books.length / columns)
          }
          rowHeight={view === "table" ? (largeText ? 76 : 64) : (largeText ? 380 : 330)}
          onEnd={books.length < total ? more : undefined}
          header={
            view === "table" && (
              <div className="ebook-columns table-head">
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
                return (
                  <div className="ebook-columns book-row">
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
                  .map((b) => (
                    <div className="ebook-shelf-item" key={b.id}>
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
                  ))}
              </div>
            )
          }
        />
      )}
      <div className="collection-footer">
        <span style={{ fontWeight: 650, color: "var(--text-secondary)", fontSize: "12px" }}>
          {total.toLocaleString("ru-RU")} книг в библиотеке ·{" "}
          {books.length.toLocaleString("ru-RU")} загружено
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
