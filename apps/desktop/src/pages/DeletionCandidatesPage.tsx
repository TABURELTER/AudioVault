import { useCallback, useEffect, useMemo, useState } from "react";
import {
  Trash2,
  RotateCcw,
  HardDrive,
  Headphones,
  BookOpen,
  Clock,
  FolderOpen,
  Info,
  CheckSquare,
  X,
  History,
  RefreshCw,
  ShieldAlert,
} from "lucide-react";
import type { Book, Ebook, PendingDeletion, DeletionCounts } from "../api/types";
import { api, formatBytes, formatDuration } from "../api/client";
import {
  BookCover,
  EmptyState,
  LoadingRows,
  SearchField,
  StatusIndicator,
  getFormatClass,
} from "../components/Primitives";
import { DeletionConfirmModal, type ItemToDelete } from "../components/DeletionConfirmModal";

interface Props {
  catalogRevision?: number;
  onSelectBook: (id: number) => void;
  onSelectEbook: (id: number) => void;
  onRevealFolder: (path: string) => void;
  onOpenHistory: () => void;
  onChanged?: () => void;
}

interface UnifiedCandidate {
  key: string;
  id: number;
  kind: "book" | "ebook";
  title: string;
  authors: string;
  narrators?: string;
  series?: string | null;
  format: string;
  size: number;
  durationMs?: number;
  volumeName: string;
  online: boolean;
  path: string;
  coverThumb?: string | null;
}

type TabMode = "all" | "books" | "ebooks" | "pending";
type SortMode = "size_desc" | "title_asc" | "author_asc" | "volume_asc" | "type_asc";

export function DeletionCandidatesPage({
  catalogRevision = 0,
  onSelectBook,
  onSelectEbook,
  onRevealFolder,
  onOpenHistory,
  onChanged,
}: Props) {
  const [tab, setTab] = useState<TabMode>("all");
  const [books, setBooks] = useState<Book[]>([]);
  const [ebooks, setEbooks] = useState<Ebook[]>([]);
  const [pendingList, setPendingList] = useState<PendingDeletion[]>([]);
  const [summary, setSummary] = useState<DeletionCounts | null>(null);
  const [loading, setLoading] = useState(true);
  const [query, setQuery] = useState("");
  const [volumeFilter, setVolumeFilter] = useState("all");
  const [sortMode, setSortMode] = useState<SortMode>("size_desc");
  const [selectedKeys, setSelectedKeys] = useState<Set<string>>(new Set());
  const [deleteModalOpen, setDeleteModalOpen] = useState(false);
  const [itemsToDelete, setItemsToDelete] = useState<ItemToDelete[]>([]);
  const [busy, setBusy] = useState(false);

  const loadData = useCallback(async () => {
    setLoading(true);
    try {
      const [markedBooks, markedEbooksRes, pendingRes, summaryRes] = await Promise.all([
        api.getLibrary("candidates", "size_desc", "", 2000, 0),
        api.searchEbooks("", "all", "size_desc", undefined, true, 2000, 0),
        api.getPendingDeletions(),
        api.getDeletionCounts(),
      ]);
      setBooks(markedBooks);
      setEbooks(markedEbooksRes.items);
      setPendingList(pendingRes);
      setSummary(summaryRes);
    } catch (e) {
      console.error("Failed to load deletion candidates:", e);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    // eslint-disable-next-line react/set-state-in-effect
    loadData();
  }, [loadData, catalogRevision]);

  const candidates = useMemo<UnifiedCandidate[]>(() => {
    const list: UnifiedCandidate[] = [];
    for (const b of books) {
      if (!b.is_marked_for_deletion) continue;
      list.push({
        key: `book:${b.id}`,
        id: b.id,
        kind: "book",
        title: b.title || "Без названия",
        authors: b.authors || "Не указан",
        narrators: b.narrators,
        series: b.series,
        format: b.format || "AUDIO",
        size: b.size || 0,
        durationMs: b.duration_ms,
        volumeName: b.volume_name || "Архив",
        online: b.online,
        path: b.series || b.volume_name || "",
        coverThumb: b.cover_thumb,
      });
    }
    for (const e of ebooks) {
      if (!e.is_marked_for_deletion) continue;
      list.push({
        key: `ebook:${e.id}`,
        id: e.id,
        kind: "ebook",
        title: e.title || e.file_name,
        authors: e.authors || "Не указан",
        series: e.series,
        format: e.format || "FB2",
        size: e.size || 0,
        volumeName: e.volume_name || "Архив",
        online: e.volume_online,
        path: e.full_path || e.rel_path,
        coverThumb: e.cover_thumb,
      });
    }
    return list;
  }, [books, ebooks]);

  const availableVolumes = useMemo(() => {
    const vols = new Set<string>();
    for (const c of candidates) {
      if (c.volumeName) vols.add(c.volumeName);
    }
    for (const p of pendingList) {
      if (p.volume_name) vols.add(p.volume_name);
    }
    return Array.from(vols).sort((a, b) => a.localeCompare(b, "ru"));
  }, [candidates, pendingList]);

  const filteredCandidates = useMemo(() => {
    const q = query.trim().toLowerCase();
    const list = candidates.filter((c) => {
      if (tab === "books" && c.kind !== "book") return false;
      if (tab === "ebooks" && c.kind !== "ebook") return false;
      if (volumeFilter !== "all" && c.volumeName !== volumeFilter) return false;
      if (q) {
        const hay = `${c.title} ${c.authors} ${c.narrators || ""} ${c.format} ${c.volumeName} ${c.path}`.toLowerCase();
        if (!hay.includes(q)) return false;
      }
      return true;
    });

    list.sort((a, b) => {
      switch (sortMode) {
        case "size_desc":
          return b.size - a.size;
        case "title_asc":
          return a.title.localeCompare(b.title, "ru");
        case "author_asc":
          return a.authors.localeCompare(b.authors, "ru");
        case "volume_asc":
          return a.volumeName.localeCompare(b.volumeName, "ru");
        case "type_asc":
          return a.kind.localeCompare(b.kind) || b.size - a.size;
        default:
          return 0;
      }
    });

    return list;
  }, [candidates, tab, volumeFilter, query, sortMode]);

  const filteredPending = useMemo(() => {
    const q = query.trim().toLowerCase();
    return pendingList.filter((p) => {
      if (volumeFilter !== "all" && p.volume_name !== volumeFilter) return false;
      if (q) {
        const hay = `${p.title} ${p.authors} ${p.volume_name} ${p.rel_path}`.toLowerCase();
        if (!hay.includes(q)) return false;
      }
      return true;
    });
  }, [pendingList, volumeFilter, query]);

  const totalMarkedSize = useMemo(
    () => candidates.reduce((acc, c) => acc + c.size, 0),
    [candidates],
  );

  const toggleSelectKey = (key: string) => {
    setSelectedKeys((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  };

  const toggleSelectAll = () => {
    if (
      filteredCandidates.length > 0 &&
      filteredCandidates.every((c) => selectedKeys.has(c.key))
    ) {
      setSelectedKeys(new Set());
    } else {
      setSelectedKeys(new Set(filteredCandidates.map((c) => c.key)));
    }
  };

  // Unmark single item (return to library)
  const handleUnmarkSingle = async (item: UnifiedCandidate) => {
    try {
      if (item.kind === "book") {
        await api.setBookMarkedForDeletion(item.id, false);
      } else {
        await api.setEbookMarkedForDeletion(item.id, false);
      }
      setSelectedKeys((prev) => {
        const next = new Set(prev);
        next.delete(item.key);
        return next;
      });
      await loadData();
      onChanged?.();
    } catch (e) {
      console.error("handleUnmarkSingle error:", e);
    }
  };

  // Bulk unmark selected items
  const handleBulkUnmark = async (keysToUnmark?: string[]) => {
    const targetKeys = keysToUnmark || Array.from(selectedKeys);
    if (!targetKeys.length) return;
    setBusy(true);
    try {
      const bookIds = targetKeys
        .filter((k) => k.startsWith("book:"))
        .map((k) => Number(k.slice(5)));
      const ebookIds = targetKeys
        .filter((k) => k.startsWith("ebook:"))
        .map((k) => Number(k.slice(6)));

      if (bookIds.length > 0) {
        await api.setMultipleBooksMarkedForDeletion(bookIds, false);
      }
      if (ebookIds.length > 0) {
        await api.setMultipleEbooksMarkedForDeletion(ebookIds, false);
      }
      setSelectedKeys(new Set());
      await loadData();
      onChanged?.();
    } catch (e) {
      console.error("handleBulkUnmark error:", e);
    } finally {
      setBusy(false);
    }
  };

  // Open deletion confirmation modal for selected or all candidates
  const openDeleteConfirmation = (targetCandidates: UnifiedCandidate[]) => {
    if (!targetCandidates.length) return;
    const mapped: ItemToDelete[] = targetCandidates.map((c) => ({
      id: c.id,
      type: c.kind,
      title: c.title,
      authors: c.authors,
      volumeName: c.volumeName,
      volumeOnline: c.online,
      size: c.size,
      path: c.path,
    }));
    setItemsToDelete(mapped);
    setDeleteModalOpen(true);
  };

  const handleConfirmDeletion = async () => {
    if (!itemsToDelete.length) return;
    setBusy(true);
    try {
      const bookIds = itemsToDelete
        .filter((i) => i.type === "book")
        .map((i) => i.id);
      const ebookIds = itemsToDelete
        .filter((i) => i.type === "ebook")
        .map((i) => i.id);

      await api.executeDeletion(bookIds, ebookIds);
      setDeleteModalOpen(false);
      setSelectedKeys(new Set());
      await loadData();
      onChanged?.();
    } catch (e) {
      console.error("handleConfirmDeletion error:", e);
    } finally {
      setBusy(false);
    }
  };

  const handleCancelPendingTask = async (pendingId: number) => {
    setBusy(true);
    try {
      await api.cancelPendingDeletion(pendingId);
      await loadData();
      onChanged?.();
    } catch (e) {
      console.error("handleCancelPendingTask error:", e);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="collection-page">
      {/* Top Summary Banner & Quick Actions */}
      <div
        style={{
          display: "grid",
          gridTemplateColumns: "repeat(auto-fit, minmax(220px, 1fr))",
          gap: 12,
        }}
      >
        <div
          style={{
            background: "var(--bg-surface)",
            border: "1px solid var(--border-card)",
            borderRadius: 10,
            padding: "14px 16px",
            display: "flex",
            flexDirection: "column",
            gap: 4,
          }}
        >
          <span style={{ fontSize: 12, color: "var(--text-muted)", fontWeight: 600 }}>
            Всего помечено на удаление
          </span>
          <strong style={{ fontSize: 22, color: "var(--text-primary)" }}>
            {candidates.length} файлов
          </strong>
          <small style={{ color: "#86efac", fontWeight: 600 }}>
            Освободится: {formatBytes(totalMarkedSize)}
          </small>
        </div>

        <div
          style={{
            background: "var(--bg-surface)",
            border: "1px solid var(--border-card)",
            borderRadius: 10,
            padding: "14px 16px",
            display: "flex",
            flexDirection: "column",
            gap: 4,
          }}
        >
          <span style={{ fontSize: 12, color: "var(--text-muted)", fontWeight: 600 }}>
            По типам медиа
          </span>
          <strong style={{ fontSize: 18, color: "var(--text-primary)" }}>
            {books.length} аудио · {ebooks.length} эл. книг
          </strong>
          <small style={{ color: "var(--text-secondary)" }}>
            Вложенные .fb2 в аудиопапках защищены
          </small>
        </div>

        <div
          style={{
            background: "var(--bg-surface)",
            border: "1px solid var(--border-card)",
            borderRadius: 10,
            padding: "14px 16px",
            display: "flex",
            flexDirection: "column",
            gap: 4,
          }}
        >
          <span style={{ fontSize: 12, color: "var(--text-muted)", fontWeight: 600 }}>
            Очередь офлайн-накопителей
          </span>
          <strong style={{ fontSize: 18, color: pendingList.length > 0 ? "#fbbf24" : "var(--text-primary)" }}>
            {pendingList.length} задач в очереди
          </strong>
          <small style={{ color: "var(--text-secondary)" }}>
            Всего удалено за всё время: {summary?.deleted_history_count ?? 0}
          </small>
        </div>
      </div>

      {/* Mode Tabs & Top Action Buttons */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          gap: 10,
          flexWrap: "wrap",
          marginTop: 4,
        }}
      >
        <div style={{ display: "flex", alignItems: "center", gap: 8, flexWrap: "wrap" }}>
          <button
            type="button"
            className={`btn ${tab === "all" ? "btn-primary" : "btn-secondary"}`}
            onClick={() => setTab("all")}
            style={{ height: 34, fontSize: 13 }}
          >
            <Trash2 size={14} />
            <span>Все помеченные ({candidates.length})</span>
          </button>
          <button
            type="button"
            className={`btn ${tab === "books" ? "btn-primary" : "btn-secondary"}`}
            onClick={() => setTab("books")}
            style={{ height: 34, fontSize: 13 }}
          >
            <Headphones size={14} />
            <span>Аудиокниги ({books.length})</span>
          </button>
          <button
            type="button"
            className={`btn ${tab === "ebooks" ? "btn-primary" : "btn-secondary"}`}
            onClick={() => setTab("ebooks")}
            style={{ height: 34, fontSize: 13 }}
          >
            <BookOpen size={14} />
            <span>Электронные книги ({ebooks.length})</span>
          </button>
          <button
            type="button"
            className={`btn ${tab === "pending" ? "btn-primary" : "btn-secondary"}`}
            onClick={() => setTab("pending")}
            style={{ height: 34, fontSize: 13 }}
          >
            <Clock size={14} />
            <span>Очередь офлайн-дисков ({pendingList.length})</span>
          </button>
        </div>

        <div style={{ display: "flex", alignItems: "center", gap: 8, flexWrap: "wrap" }}>
          <button
            type="button"
            className="btn btn-secondary"
            onClick={onOpenHistory}
            style={{ height: 34, fontSize: 13 }}
          >
            <History size={14} />
            <span>Журнал удалений</span>
          </button>
          {tab !== "pending" && candidates.length > 0 && (
            <>
              <button
                type="button"
                className="btn btn-secondary"
                disabled={busy}
                onClick={() => handleBulkUnmark(candidates.map((c) => c.key))}
                style={{ height: 34, fontSize: 13 }}
                title="Снять пометку со всех книг и вернуть их в каталог"
              >
                <RotateCcw size={14} />
                <span>Вернуть все в каталог</span>
              </button>
              <button
                type="button"
                className="btn btn-danger"
                disabled={busy}
                onClick={() => openDeleteConfirmation(filteredCandidates)}
                style={{ height: 34, fontSize: 13 }}
              >
                <Trash2 size={14} />
                <span>
                  Удалить все ({filteredCandidates.length} · {formatBytes(
                    filteredCandidates.reduce((s, c) => s + c.size, 0),
                  )})
                </span>
              </button>
            </>
          )}
        </div>
      </div>

      {/* Search, Volume Filter & Sort Toolbar */}
      <div className="toolbar" style={{ flexWrap: "wrap", gap: 12 }}>
        <div className="toolbar-left" style={{ flex: "1 1 320px" }}>
          <div style={{ width: "min(360px, 100%)" }}>
            <SearchField
              value={query}
              onChange={setQuery}
              loading={loading}
              placeholder="Поиск по названию, автору, формату или диску..."
            />
          </div>
        </div>

        <div className="toolbar-right" style={{ flexWrap: "wrap", gap: 8 }}>
          <select
            aria-label="Фильтр по накопителю"
            value={volumeFilter}
            onChange={(e) => setVolumeFilter(e.target.value)}
          >
            <option value="all">Все накопители</option>
            {availableVolumes.map((v) => (
              <option key={v} value={v}>
                {v}
              </option>
            ))}
          </select>

          {tab !== "pending" && (
            <select
              aria-label="Сортировка кандидатов на удаление"
              value={sortMode}
              onChange={(e) => setSortMode(e.target.value as SortMode)}
            >
              <option value="size_desc">По размеру (сначала крупные)</option>
              <option value="title_asc">По названию (А → Я)</option>
              <option value="author_asc">По автору (А → Я)</option>
              <option value="volume_asc">По накопителю</option>
              <option value="type_asc">По типу (аудио / эл. книги)</option>
            </select>
          )}

          <button
            type="button"
            className="btn btn-secondary"
            onClick={loadData}
            disabled={loading}
            title="Обновить список"
          >
            <RefreshCw size={15} className={loading ? "spin" : ""} />
          </button>
        </div>
      </div>

      {/* Main Content Area */}
      {loading ? (
        <LoadingRows />
      ) : tab === "pending" ? (
        filteredPending.length === 0 ? (
          <EmptyState
            title="Очередь отложенных удалений пуста"
            description="Когда вы удаляете книги, находящиеся на отключённом внешнем диске, задачи попадают сюда и автоматически выполняются при подключении диска."
          />
        ) : (
          <div className="book-list" style={{ overflowY: "auto", padding: "8px 0" }}>
            <div
              className="protected-scenario-banner"
              style={{ marginBottom: 12 }}
            >
              <ShieldAlert size={18} style={{ flexShrink: 0 }} />
              <span>
                Эти файлы находятся на отключённых внешних накопителях. Как только вы подключите соответствующий диск, AudioVault автоматически удалит их. Вы можете отменить любую задачу до подключения диска.
              </span>
            </div>
            <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
              {filteredPending.map((p) => (
                <div
                  key={p.id}
                  style={{
                    padding: "12px 16px",
                    background: "var(--bg-surface)",
                    border: "1px solid var(--border-subtle)",
                    borderRadius: 10,
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "space-between",
                    gap: 14,
                  }}
                >
                  <div style={{ flex: 1, minWidth: 0 }}>
                    <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                      <span className="badge badge-format">
                        {p.item_type === "ebook" ? "ЭЛ. КНИГА" : "АУДИОКНИГА"}
                      </span>
                      <strong style={{ fontSize: 14 }}>{p.title}</strong>
                    </div>
                    <div
                      style={{
                        fontSize: 12.5,
                        color: "var(--text-secondary)",
                        marginTop: 4,
                        display: "flex",
                        gap: 10,
                        flexWrap: "wrap",
                      }}
                    >
                      <span>{p.authors || "Автор не указан"}</span>
                      {p.size > 0 && <span>· {formatBytes(p.size)}</span>}
                      <span>· Накопитель: {p.volume_name}</span>
                    </div>
                    <div
                      style={{
                        fontSize: 11.5,
                        color: "var(--text-muted)",
                        fontFamily: "monospace",
                        marginTop: 4,
                      }}
                    >
                      {p.rel_path}
                    </div>
                  </div>

                  <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
                    <span
                      style={{
                        display: "inline-flex",
                        alignItems: "center",
                        gap: 5,
                        fontSize: 12,
                        color: p.volume_online ? "#86efac" : "#fca5a5",
                      }}
                    >
                      <HardDrive size={14} />
                      {p.volume_online ? "Диск онлайн" : "Диск отключён"}
                    </span>
                    <button
                      type="button"
                      className="btn btn-secondary"
                      disabled={busy}
                      onClick={() => handleCancelPendingTask(p.id)}
                      style={{ fontSize: 12.5, height: 32 }}
                    >
                      <RotateCcw size={13} />
                      <span>Отменить удаление</span>
                    </button>
                  </div>
                </div>
              ))}
            </div>
          </div>
        )
      ) : filteredCandidates.length === 0 ? (
        <EmptyState
          title="Нет файлов, помеченных на удаление"
          description="Помечайте ненужные аудиокниги или электронные книги кнопкой корзины в «Фонотеке» или «Электронных книгах», чтобы централизованно проверить и удалить их здесь."
        />
      ) : (
        <div className="book-list" role="table" aria-label="Помеченные на удаление">
          <div
            className="book-columns table-head"
            role="row"
            style={{
              gridTemplateColumns:
                "44px minmax(280px, 2.2fr) minmax(150px, 1.2fr) 110px 100px 160px 230px",
            }}
          >
            <div
              className="col-header-select"
              style={{ display: "flex", alignItems: "center", justifyContent: "center" }}
            >
              <input
                type="checkbox"
                aria-label="Выбрать все файлы"
                checked={
                  filteredCandidates.length > 0 &&
                  filteredCandidates.every((c) => selectedKeys.has(c.key))
                }
                onChange={toggleSelectAll}
              />
            </div>
            <span>Название / серия</span>
            <span>Автор / чтец</span>
            <span>Тип / формат</span>
            <span>Размер</span>
            <span>Накопитель</span>
            <span style={{ textAlign: "right", paddingRight: 8 }}>Управление</span>
          </div>

          <div style={{ flex: 1, overflowY: "auto" }}>
            {filteredCandidates.map((item) => {
              const isSelected = selectedKeys.has(item.key);
              return (
                <div
                  key={item.key}
                  role="row"
                  className={`book-columns book-row cover-medium ${
                    isSelected ? "selected" : ""
                  }`}
                  style={{
                    gridTemplateColumns:
                      "44px minmax(280px, 2.2fr) minmax(150px, 1.2fr) 110px 100px 160px 230px",
                    minHeight: 64,
                  }}
                >
                  <span
                    className="book-select-cell col-check"
                    role="cell"
                    onClick={(e) => {
                      if (e.target === e.currentTarget) toggleSelectKey(item.key);
                    }}
                  >
                    <input
                      type="checkbox"
                      aria-label={`Выбрать ${item.title}`}
                      checked={isSelected}
                      onChange={() => toggleSelectKey(item.key)}
                      onClick={(e) => e.stopPropagation()}
                    />
                  </span>

                  <span className="book-title-cell" role="cell">
                    <BookCover
                      title={item.title}
                      thumb={item.coverThumb ?? undefined}
                      format={item.format}
                      author={item.authors}
                      size="medium"
                    />
                    <span className="title-texts">
                      <button
                        className="title-button"
                        title={item.title}
                        onClick={() =>
                          item.kind === "book"
                            ? onSelectBook(item.id)
                            : onSelectEbook(item.id)
                        }
                      >
                        {item.title}
                      </button>
                      <small style={{ display: "flex", alignItems: "center", gap: 6 }}>
                        <span>
                          {item.series ||
                            (item.kind === "book" ? "Аудиокнига" : "Электронная книга")}
                        </span>
                        {item.durationMs ? (
                          <span>· {formatDuration(item.durationMs)}</span>
                        ) : null}
                      </small>
                    </span>
                  </span>

                  <span role="cell" title={item.authors}>
                    <div style={{ fontWeight: 600 }}>{item.authors}</div>
                    {item.narrators && (
                      <small style={{ color: "var(--text-muted)", display: "block" }}>
                        Чтец: {item.narrators}
                      </small>
                    )}
                  </span>

                  <span role="cell">
                    <span className={`badge badge-format ${getFormatClass(item.format)}`}>
                      {item.format.toUpperCase()}
                    </span>
                  </span>

                  <span role="cell" className="numeric">
                    {formatBytes(item.size)}
                  </span>

                  <span role="cell" className="book-storage">
                    <StatusIndicator online={item.online} label={item.volumeName} />
                    <small
                      style={{
                        color: item.online ? "#86efac" : "#fca5a5",
                        fontWeight: 600,
                      }}
                    >
                      {item.online ? "Онлайн" : "Отключён"}
                    </small>
                  </span>

                  <span className="row-actions-group" role="cell">
                    <button
                      type="button"
                      className="btn-listened-quick"
                      onClick={() => handleUnmarkSingle(item)}
                      title="Снять пометку на удаление и вернуть в библиотеку"
                    >
                      <RotateCcw size={13} />
                      <span>Вернуть</span>
                    </button>
                    <button
                      type="button"
                      className="btn-mark-del-quick is-active"
                      onClick={() => openDeleteConfirmation([item])}
                      title="Удалить этот файл навсегда"
                    >
                      <Trash2 size={14} />
                    </button>
                    <button
                      type="button"
                      className="plain-icon table-action-btn"
                      title="Открыть сведения"
                      onClick={() =>
                        item.kind === "book"
                          ? onSelectBook(item.id)
                          : onSelectEbook(item.id)
                      }
                    >
                      <Info size={15} />
                    </button>
                    {item.kind === "ebook" && item.online && item.path && (
                      <button
                        type="button"
                        className="plain-icon table-action-btn"
                        title="Показать в Проводнике"
                        onClick={() => onRevealFolder(item.path)}
                      >
                        <FolderOpen size={15} />
                      </button>
                    )}
                  </span>
                </div>
              );
            })}
          </div>
        </div>
      )}

      {/* Floating Bulk Action Bar */}
      {selectedKeys.size > 0 && (
        <div className="floating-bulk-bar">
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <CheckSquare size={16} color="var(--accent-hover)" />
            <strong style={{ fontSize: 13.5 }}>Выбрано: {selectedKeys.size}</strong>
          </div>

          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <button
              type="button"
              className="btn btn-secondary"
              disabled={busy}
              onClick={() => handleBulkUnmark()}
              style={{ fontSize: 12.5, gap: 6 }}
            >
              <RotateCcw size={14} color="#86efac" />
              <span>Вернуть в каталог ({selectedKeys.size})</span>
            </button>

            <button
              type="button"
              className="btn btn-danger"
              disabled={busy}
              onClick={() =>
                openDeleteConfirmation(
                  candidates.filter((c) => selectedKeys.has(c.key)),
                )
              }
              style={{ fontSize: 12.5, gap: 6 }}
            >
              <Trash2 size={14} />
              <span>Удалить навсегда ({selectedKeys.size})</span>
            </button>

            <button
              type="button"
              className="plain-icon"
              onClick={() => setSelectedKeys(new Set())}
              title="Снять выделение"
            >
              <X size={16} />
            </button>
          </div>
        </div>
      )}

      <DeletionConfirmModal
        isOpen={deleteModalOpen}
        items={itemsToDelete}
        busy={busy}
        onConfirm={handleConfirmDeletion}
        onCancel={() => setDeleteModalOpen(false)}
      />
    </div>
  );
}
