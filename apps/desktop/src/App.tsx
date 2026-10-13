import React, { useState, useEffect, useCallback, useRef } from "react";
import { Sidebar } from "./components/Sidebar";
import { Header } from "./components/Header";
import { BookInspectorDrawer } from "./components/BookInspectorDrawer";
import { EbookInspectorDrawer } from "./components/EbookInspectorDrawer";
import { ThemeOnboardingModal } from "./components/ThemeOnboardingModal";
import { DeletionHistoryModal } from "./components/DeletionHistoryModal";
import { UpdateModal } from "./components/UpdateModal";

// Pages
import { LibraryPage } from "./pages/LibraryPage";
import { EbooksPage } from "./pages/EbooksPage";
import { SearchPage } from "./pages/SearchPage";
import { StoragesPage } from "./pages/StoragesPage";
import { DuplicatesPage } from "./pages/DuplicatesPage";
import { DeletionCandidatesPage } from "./pages/DeletionCandidatesPage";
import { IssuesPage } from "./pages/IssuesPage";
import { RuTrackerPage } from "./pages/RuTrackerPage";
import { DownloadsPage } from "./pages/DownloadsPage";
import { OptimizerPage } from "./pages/OptimizerPage";
import { TasksPage } from "./pages/TasksPage";
import { SettingsPage } from "./pages/SettingsPage";

// API & Types
import { api } from "./api/client";
import type {
  BookDetail,
  EbookDetail,
  Volume,
  DuplicatePair,
  OptimizerItem,
  Task,
  Stats,
  UpdateCheckResult,
} from "./api/types";
import { useLibrary } from "./hooks/useLibrary";
import { Modal } from "./components/Modal";

const initialStats: Stats = {
  total_books: 0,
  total_copies: 0,
  total_duration_ms: 0,
  total_size_bytes: 0,
  online_volumes: 0,
  offline_volumes: 0,
  duplicates_count: 0,
  issues_count: 0,
  total_ebooks: 0,
};

export const App: React.FC = () => {
  const [currentPage, setCurrentPage] = useState<string>("library");
  const [stats, setStats] = useState<Stats>(initialStats);
  const [volumes, setVolumes] = useState<Volume[]>([]);
  const [tasks, setTasks] = useState<Task[]>([]);
  const [duplicates, setDuplicates] = useState<DuplicatePair[]>([]);
  const [optimizerItems, setOptimizerItems] = useState<OptimizerItem[]>([]);

  // Filtering & Sorting
  const [activeFilter, setActiveFilter] = useState<string>("all");
  const [activeSort, setActiveSort] = useState<string>("author");
  const [searchQuery, setSearchQuery] = useState<string>("");
  const [volumeFilter, setVolumeFilter] = useState("");
  const {
    books,
    loading: libraryLoading,
    hasMore,
    error: libraryError,
    reload: reloadBooks,
    loadMore,
  } = useLibrary(activeFilter, activeSort, searchQuery, volumeFilter);
  const [notice, setNotice] = useState("");
  const selectionRequest = useRef(0);
  const activeInspector = useRef<{ book: number | null; ebook: number | null }>({ book: null, ebook: null });
  const [catalogRevision, setCatalogRevision] = useState(0);
  const detailTrigger = useRef<HTMLElement | null>(null);

  // Book details inspector
  const [selectedBookId, setSelectedBookId] = useState<number | null>(null);
  const [selectedBook, setSelectedBook] = useState<BookDetail | null>(null);

  // E-book details inspector
  const [selectedEbookId, setSelectedEbookId] = useState<number | null>(null);
  const [selectedEbook, setSelectedEbook] = useState<EbookDetail | null>(null);

  // Status flags
  const [isScanning, setIsScanning] = useState(false);
  const [isSearchingDuplicates, setIsSearchingDuplicates] = useState(false);

  // Folder modal
  const [folderModalOpen, setFolderModalOpen] = useState(false);
  const [manualFolderPath, setManualFolderPath] = useState("");
  const [addFolderError, setAddFolderError] = useState<string | null>(null);
  const [addingFolder, setAddingFolder] = useState(false);

  // Theme state & Onboarding modal
  const [theme, setTheme] = useState<"dark" | "light">(() => {
    try {
      const saved = localStorage.getItem("audiovault-theme");
      if (saved === "light" || saved === "dark") return saved;
    } catch {}
    return "dark";
  });

  const [showThemeOnboarding, setShowThemeOnboarding] = useState<boolean>(() => {
    try {
      return !localStorage.getItem("audiovault-theme-chosen");
    } catch {
      return false;
    }
  });

  const [historyModalOpen, setHistoryModalOpen] = useState(false);
  const [candidatesCount, setCandidatesCount] = useState(0);
  const [updateInfo, setUpdateInfo] = useState<UpdateCheckResult | null>(null);

  useEffect(() => {
    document.documentElement.setAttribute("data-theme", theme);
  }, [theme]);

  const handleSelectTheme = (newTheme: "dark" | "light") => {
    setTheme(newTheme);
    try {
      localStorage.setItem("audiovault-theme", newTheme);
    } catch {}
    document.documentElement.setAttribute("data-theme", newTheme);
  };

  const handleConfirmOnboarding = () => {
    try {
      localStorage.setItem("audiovault-theme-chosen", "true");
      localStorage.setItem("audiovault-theme", theme);
    } catch {}
    setShowThemeOnboarding(false);
  };

  const toggleTheme = () => {
    handleSelectTheme(theme === "dark" ? "light" : "dark");
  };

  // Reload core data
  const reloadStats = useCallback(async () => {
    try {
      const [data, delSummary] = await Promise.all([
        api.getStats(),
        api.getDeletionCounts(),
      ]);
      setStats(data);
      if (delSummary) {
        setCandidatesCount(
          (delSummary.candidates_books || 0) +
            (delSummary.candidates_ebooks || 0) +
            (delSummary.pending_tasks || 0),
        );
      }
    } catch (e) {
      console.error("reloadStats error:", e);
    }
  }, []);

  const reloadVolumes = useCallback(async () => {
    try {
      const data = await api.getVolumes();
      setVolumes(data);
    } catch (e) {
      console.error("reloadVolumes error:", e);
    }
  }, []);

  const reloadTasks = useCallback(async () => {
    try {
      const data = await api.getTasks();
      setTasks(data);
    } catch (e) {
      console.error("reloadTasks error:", e);
    }
  }, []);

  const reloadDuplicates = useCallback(async () => {
    try {
      const data = await api.getDuplicates();
      setDuplicates(data);
    } catch (e) {
      console.error("reloadDuplicates error:", e);
    }
  }, []);

  const reloadOptimizer = useCallback(async () => {
    try {
      const data = await api.getOptimizations();
      setOptimizerItems(data);
    } catch (e) {
      console.error("reloadOptimizer error:", e);
    }
  }, []);

  // Load catalog summaries without blocking the library or refetching on search.
  useEffect(() => {
    // Summary requests only update state after their asynchronous IPC responses.
    // eslint-disable-next-line react/set-state-in-effect
    reloadStats();
    reloadVolumes();
    reloadTasks();
  }, [reloadStats, reloadVolumes, reloadTasks]);

  // Check for GitHub updates on startup
  useEffect(() => {
    let cancelled = false;
    api
      .getSettings()
      .then(async (s) => {
        if (cancelled || s.check_updates_on_startup === false) return;
        const info = await api.checkGithubUpdate();
        if (!cancelled && info && info.update_available && !info.skipped) {
          setUpdateInfo(info);
        }
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    // These requests update state only after IPC, and run when the page is opened.
    // eslint-disable-next-line react/set-state-in-effect
    if (currentPage === "duplicates") reloadDuplicates();
    if (currentPage === "optimizer") reloadOptimizer();
  }, [currentPage, reloadDuplicates, reloadOptimizer]);

  // Subscribe to backend events
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let disposed = false;
    const refreshInspector = () => {
      const { book, ebook } = activeInspector.current;
      if (book === null && ebook === null) return;
      const request = ++selectionRequest.current;
      if (book !== null) api.getBookDetails(book).then(detail => {
        if (request === selectionRequest.current && detail) setSelectedBook(detail);
      });
      else if (ebook !== null) api.getEbookDetail(ebook).then(detail => {
        if (request === selectionRequest.current && detail) setSelectedEbook(detail);
      });
    };
    api
      .onEvent((event) => {
        if (disposed) return;
        if (event === "library_changed") {
          setCatalogRevision(value => value + 1);
          refreshInspector();
          reloadBooks();
          reloadStats();
        } else if (event === "volumes_changed") {
          setCatalogRevision(value => value + 1);
          refreshInspector();
          reloadBooks();
          reloadVolumes();
          reloadStats();
        } else if (event === "duplicates_changed") {
          reloadDuplicates();
          reloadStats();
        } else if (event === "jobs_changed") {
          reloadTasks();
          reloadStats();
        } else if (event === "issues_changed") {
          reloadStats();
        }
      })
      .then((un) => {
        if (disposed) un();
        else unlisten = un;
      })
      .catch(() =>
        setNotice(
          "Не удалось подключиться к обновлениям каталога. Перезапустите приложение.",
        ),
      );

    return () => {
      disposed = true;
      if (unlisten) unlisten();
    };
  }, [reloadBooks, reloadVolumes, reloadDuplicates, reloadTasks, reloadStats]);

  // Book selection
  const handleSelectBook = async (bookId: number) => {
    activeInspector.current = { book: bookId, ebook: null };
    const request = ++selectionRequest.current;
    detailTrigger.current = document.activeElement as HTMLElement;
    setSelectedEbookId(null);
    setSelectedEbook(null);
    setSelectedBookId(bookId);
    setSelectedBook(null);
    try {
      const detail = await api.getBookDetails(bookId);
      if (request === selectionRequest.current) {
        setSelectedBook(detail);
        if (!detail) {
          activeInspector.current = { book: null, ebook: null };
          setSelectedBookId(null);
          setNotice("Не удалось открыть сведения о книге. Попробуйте ещё раз.");
        }
      }
    } catch (e) {
      console.error("Failed to load book details:", e);
    }
  };

  const handleCloseDrawer = () => {
    activeInspector.current = { book: null, ebook: null };
    requestAnimationFrame(() => detailTrigger.current?.focus());
    selectionRequest.current++;
    setSelectedBookId(null);
    setSelectedBook(null);
  };

  // E-book selection
  const handleSelectEbook = async (ebookId: number) => {
    activeInspector.current = { book: null, ebook: ebookId };
    const request = ++selectionRequest.current;
    detailTrigger.current = document.activeElement as HTMLElement;
    setSelectedBookId(null);
    setSelectedBook(null);
    setSelectedEbookId(ebookId);
    setSelectedEbook(null);
    try {
      const detail = await api.getEbookDetail(ebookId);
      if (request === selectionRequest.current) {
        setSelectedEbook(detail);
        if (!detail) {
          activeInspector.current = { book: null, ebook: null };
          setSelectedEbookId(null);
          setNotice("Не удалось открыть сведения о книге.");
        }
      }
    } catch (e) {
      console.error("Failed to load ebook details:", e);
    }
  };

  const handleCloseEbookDrawer = () => {
    activeInspector.current = { book: null, ebook: null };
    requestAnimationFrame(() => detailTrigger.current?.focus());
    selectionRequest.current++;
    setSelectedEbookId(null);
    setSelectedEbook(null);
  };

  const handleOpenFile = async (path: string) => {
    try {
      await api.openFile(path);
    } catch (e) {
      console.error("handleOpenFile error:", e);
      setNotice(
        "Не удалось выполнить действие. Проверьте подключение накопителя и повторите попытку.",
      );
    }
  };

  // Scan triggers
  const handleScanAll = async () => {
    setIsScanning(true);
    try {
      await api.scanAll();
      await reloadTasks();
      await reloadStats();
    } catch (e) {
      console.error("handleScanAll error:", e);
      setNotice(
        "Не удалось выполнить действие. Проверьте подключение накопителя и повторите попытку.",
      );
    } finally {
      setIsScanning(false);
    }
  };

  const handleScanVolume = async (volumeId: number) => {
    try {
      await api.scanVolume(volumeId);
      await reloadTasks();
    } catch (e) {
      console.error("handleScanVolume error:", e);
      setNotice(
        "Не удалось выполнить действие. Проверьте подключение накопителя и повторите попытку.",
      );
    }
  };

  const handleFindDuplicates = async () => {
    setIsSearchingDuplicates(true);
    try {
      await api.findAllDuplicates();
      await reloadDuplicates();
      await reloadTasks();
      await reloadStats();
    } catch (e) {
      console.error("handleFindDuplicates error:", e);
      setNotice(
        "Не удалось выполнить действие. Проверьте подключение накопителя и повторите попытку.",
      );
    } finally {
      setIsSearchingDuplicates(false);
    }
  };

  const handleResolveDuplicate = async (dupId: number, status: string) => {
    try {
      await api.resolveDuplicate(dupId, status);
      await reloadDuplicates();
      await reloadStats();
    } catch (e) {
      console.error("handleResolveDuplicate error:", e);
      setNotice(
        "Не удалось выполнить действие. Проверьте подключение накопителя и повторите попытку.",
      );
    }
  };

  const handleTranscode = async (copyId: number, format?: string) => {
    try {
      await api.transcodeCopy(copyId, format);
      await reloadTasks();
      setCurrentPage("tasks");
    } catch (e) {
      console.error("handleTranscode error:", e);
      setNotice(
        "Не удалось выполнить действие. Проверьте подключение накопителя и повторите попытку.",
      );
    }
  };

  const handleCancelTask = async (taskId: number) => {
    try {
      await api.cancelTask(taskId);
      await reloadTasks();
    } catch (e) {
      console.error("handleCancelTask error:", e);
      setNotice(
        "Не удалось выполнить действие. Проверьте подключение накопителя и повторите попытку.",
      );
    }
  };

  const handleRevealFolder = async (path: string) => {
    try {
      await api.revealFolder(path);
    } catch (e) {
      console.error("handleRevealFolder error:", e);
      setNotice(
        "Не удалось выполнить действие. Проверьте подключение накопителя и повторите попытку.",
      );
    }
  };

  const handlePlayTrack = async (_fileName: string, path: string) => {
    try {
      await api.openFile(path);
    } catch (e) {
      console.error("handlePlayTrack error:", e);
      setNotice(
        "Не удалось выполнить действие. Проверьте подключение накопителя и повторите попытку.",
      );
    }
  };

  // Add Folder flow: first try native dialog, if cancelled or error fallback to modal
  const handleAddFolderClick = async () => {
    try {
      const picked = await api.pickFolder();
      if (picked) {
        await api.addStorageFolder(picked);
        await reloadVolumes();
        await reloadBooks();
        await reloadStats();
        return;
      }
      return;
    } catch {
      // Fallback to manual entry modal
    }
    setAddFolderError(null);
    setManualFolderPath("");
    setFolderModalOpen(true);
  };

  const handleManualAddFolder = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!manualFolderPath.trim() || addingFolder) return;
    setAddingFolder(true);
    try {
      setAddFolderError(null);
      await api.addStorageFolder(manualFolderPath.trim());
      setFolderModalOpen(false);
      setManualFolderPath("");
      await reloadVolumes();
      await reloadBooks();
      await reloadStats();
    } catch (err) {
      console.error(err);
      setAddFolderError(
        "Не удалось добавить папку. Проверьте путь и доступ к накопителю.",
      );
    } finally {
      setAddingFolder(false);
    }
  };

  useEffect(() => {
    const key = (event: KeyboardEvent) => {
      if (
        (event.ctrlKey || event.metaKey) &&
        (event.key.toLowerCase() === "k" || event.key.toLowerCase() === "f")
      ) {
        event.preventDefault();
        const input =
          event.key.toLowerCase() === "k"
            ? document.querySelector<HTMLInputElement>('[data-search="global"]')
            : document.querySelector<HTMLInputElement>(
                '[data-search="local"]',
              ) ||
              document.querySelector<HTMLInputElement>(
                '[data-search="global"]',
              );
        input?.focus();
        input?.select();
      }
      if (event.key === "Escape") {
        if (document.querySelector(".context-menu")) return;
        selectionRequest.current++;
        activeInspector.current = { book: null, ebook: null };
        requestAnimationFrame(() => detailTrigger.current?.focus());
        setFolderModalOpen(false);
        setSelectedBookId(null);
        setSelectedBook(null);
        setSelectedEbookId(null);
        setSelectedEbook(null);
      }
    };
    document.addEventListener("keydown", key);
    return () => document.removeEventListener("keydown", key);
  }, []);

  // Page titles
  const getPageInfo = (): { title: string; subtitle: string } => {
    switch (currentPage) {
      case "library":
        return {
          title: "Фонотека",
          subtitle: `${stats.total_books.toLocaleString("ru-RU")} аудиокниг · ${stats.online_volumes} накопителей подключено`,
        };
      case "ebooks":
        return {
          title: "Электронные книги",
          subtitle:
            "Каталог электронных и текстовых изданий (FB2, EPUB, PDF, DJVU, MOBI)",
        };
      case "read":
        return {
          title: "Прочитанные книги",
          subtitle: "Электронные книги, прочитанные вами",
        };
      case "search":
        return {
          title: "Поиск книг",
          subtitle: "По всей коллекции, включая отключённые диски",
        };
      case "storages":
        return {
          title: "Хранилища",
          subtitle: "Управление подключенными и архивными дисками",
        };
      case "duplicates":
        return {
          title: "Дубликаты",
          subtitle: "Проверка похожих аудиокниг, электронных книг и кросс-форматных пар",
        };
      case "deletion_candidates":
        return {
          title: "Помеченные на удаление",
          subtitle: "Управление книгами, подготовленными к удалению, и очередью внешних дисков",
        };
      case "issues":
        return {
          title: "Проблемы метаданных",
          subtitle: "Ошибки распознавания тегов и структуры папок",
        };
      case "rutracker":
        return {
          title: "RuTracker",
          subtitle: "Поиск раздач и релизов аудиокниг",
        };
      case "downloads":
        return {
          title: "Загрузки",
          subtitle: "Встроенный торрент-клиент AudioVault",
        };
      case "optimizer":
        return {
          title: "Оптимизация",
          subtitle: "Анализ потенциальной экономии места и сборка M4B",
        };
      case "tasks":
        return {
          title: "Задачи",
          subtitle: "Активные фоновые операции и сканирование",
        };
      case "settings":
        return {
          title: "Настройки",
          subtitle: "Конфигурация учетных записей, путей, обновлений и FFmpeg",
        };
      default:
        return { title: "AudioVault", subtitle: "Менеджер аудиокниг" };
    }
  };

  const { title, subtitle } = getPageInfo();

  return (
    <div className="app-container">
      {/* Navigation Sidebar */}
      <Sidebar
        currentPage={currentPage}
        onNavigate={(page) => {
          if (page === "listened") {
            setActiveFilter("listened");
            setCurrentPage("library");
          } else if (page === "library") {
            setActiveFilter("all");
            setCurrentPage("library");
          } else if (page === "history") {
            setHistoryModalOpen(true);
          } else {
            setCurrentPage(page);
          }
        }}
        stats={stats}
        volumes={volumes}
        candidatesCount={candidatesCount}
        onOpenHistory={() => setHistoryModalOpen(true)}
      />

      {/* Main Content Area */}
      <main className="main-content">
        <Header
          title={title}
          subtitle={subtitle}
          searchQuery={searchQuery}
          onSearchChange={(q) => {
            setSearchQuery(q);
            if (currentPage !== "library" && q.trim().length > 0) {
              setCurrentPage("library");
            }
          }}
          onScanAll={handleScanAll}
          activeTasks={tasks}
          isScanning={isScanning}
          page={currentPage}
          searching={libraryLoading && !!searchQuery}
          currentTheme={theme}
          onToggleTheme={toggleTheme}
        />

        {(notice || libraryError) && (
          <div
            className="notice error"
            role="alert"
            style={{ margin: "0 24px 10px" }}
          >
            {notice || libraryError}
            <button
              className="btn"
              onClick={() => {
                setNotice("");
                if (libraryError) reloadBooks();
              }}
            >
              Закрыть
            </button>
          </div>
        )}
        <div className="page-container">
          {currentPage === "library" && (
            <LibraryPage
              books={books}
              onSelectBook={handleSelectBook}
              activeFilter={activeFilter}
              onFilterChange={setActiveFilter}
              activeSort={activeSort}
              onSortChange={setActiveSort}
              loading={libraryLoading}
              searchQuery={searchQuery}
              onAddFolder={handleAddFolderClick}
              volumes={volumes}
              selectedBookId={selectedBookId}
              onFindDuplicates={() => setCurrentPage("duplicates")}
              volumeFilter={volumeFilter}
              onVolumeFilterChange={setVolumeFilter}
              hasMore={hasMore}
              onLoadMore={loadMore}
              onRefresh={() => {
                reloadBooks();
                reloadStats();
              }}
            />
          )}

          {(currentPage === "ebooks" || currentPage === "read") && (
            <EbooksPage
              catalogRevision={catalogRevision}
              initialViewMode={currentPage === "read" ? "read" : "catalog"}
              onSelectEbook={handleSelectEbook}
              onRevealFolder={handleRevealFolder}
              onOpenFile={handleOpenFile}
            />
          )}

          {currentPage === "search" && (
            <SearchPage
              onSelectBook={handleSelectBook}
              onSearch={api.searchBooks}
            />
          )}

          {currentPage === "storages" && (
            <StoragesPage
              volumes={volumes}
              tasks={tasks}
              onRefresh={async () => {
                await reloadVolumes();
                await reloadStats();
              }}
              onAddFolder={handleAddFolderClick}
              onScanVolume={handleScanVolume}
              onRevealFolder={handleRevealFolder}
              onRenameVolume={async (id, newName) => {
                await api.renameVolume(id, newName);
                await reloadVolumes();
              }}
              onOpenVolumeBooks={(id) => {
                setVolumeFilter(String(id));
                setActiveFilter("all");
                setSearchQuery("");
                setCurrentPage("library");
              }}
            />
          )}

          {currentPage === "duplicates" && (
            <DuplicatesPage
              duplicates={duplicates}
              onFindDuplicates={handleFindDuplicates}
              onResolve={handleResolveDuplicate}
              isSearching={isSearchingDuplicates}
            />
          )}

          {currentPage === "deletion_candidates" && (
            <DeletionCandidatesPage
              catalogRevision={catalogRevision}
              onSelectBook={handleSelectBook}
              onSelectEbook={handleSelectEbook}
              onRevealFolder={handleRevealFolder}
              onOpenHistory={() => setHistoryModalOpen(true)}
              onChanged={() => {
                reloadBooks();
                reloadStats();
                setCatalogRevision((r) => r + 1);
              }}
            />
          )}

          {currentPage === "issues" && (
            <IssuesPage
              onOpenBook={(id) => {
                handleSelectBook(id);
              }}
            />
          )}

          {currentPage === "rutracker" && (
            <RuTrackerPage
              onNavigateToDownloads={() => setCurrentPage("downloads")}
              onNavigateToSettings={() => setCurrentPage("settings")}
            />
          )}

          {currentPage === "downloads" && (
            <DownloadsPage
              onNavigateToRuTracker={() => setCurrentPage("rutracker")}
            />
          )}

          {currentPage === "optimizer" && (
            <OptimizerPage
              items={optimizerItems}
              onTranscode={handleTranscode}
            />
          )}

          {currentPage === "tasks" && (
            <TasksPage tasks={tasks} onCancelTask={handleCancelTask} />
          )}

          {currentPage === "settings" && (
            <SettingsPage
              currentTheme={theme}
              onThemeChange={handleSelectTheme}
              onUpdateFound={(info) => setUpdateInfo(info)}
              onCatalogReset={async () => {
                await reloadVolumes();
                await reloadStats();
                setCatalogRevision((r) => r + 1);
              }}
            />
          )}
        </div>
      </main>

      {/* Book Inspector Details Drawer */}
      <BookInspectorDrawer
        key={selectedBookId}
        book={selectedBook}
        isOpen={selectedBookId !== null}
        onClose={handleCloseDrawer}
        onOptimizeCopy={handleTranscode}
        onRevealFolder={handleRevealFolder}
        onPlayTrack={handlePlayTrack}
      />

      {/* E-book Inspector Details Drawer */}
      <EbookInspectorDrawer
        key={selectedEbookId}
        ebook={selectedEbook}
        isOpen={selectedEbookId !== null}
        onClose={handleCloseEbookDrawer}
        onRevealFolder={handleRevealFolder}
        onOpenFile={handleOpenFile}
      />

      {folderModalOpen && (
        <Modal
          title="Добавить папку коллекции"
          onClose={() => setFolderModalOpen(false)}
        >
          <p>
            Укажите полный путь к папке с аудиокнигами на компьютере или внешнем
            накопителе.
          </p>
          <form onSubmit={handleManualAddFolder}>
            <input
              aria-label="Путь к папке"
              className="input-text"
              value={manualFolderPath}
              onChange={(e) => setManualFolderPath(e.target.value)}
              placeholder={"G:\\Аудиокниги"}
              autoFocus
            />
            {addFolderError && (
              <div className="notice error" role="alert">
                {addFolderError}
              </div>
            )}
            <div className="modal-actions">
              <button
                type="button"
                className="btn btn-secondary"
                onClick={() => setFolderModalOpen(false)}
              >
                Отмена
              </button>
              <button
                className="btn btn-primary"
                disabled={!manualFolderPath.trim() || addingFolder}
              >
                {addingFolder ? "Добавление…" : "Добавить"}
              </button>
            </div>
          </form>
        </Modal>
      )}

      {/* First-time Launch Theme Selection Onboarding */}
      <ThemeOnboardingModal
        isOpen={showThemeOnboarding}
        currentTheme={theme}
        onSelectTheme={handleSelectTheme}
        onConfirm={handleConfirmOnboarding}
      />

      {/* Hidden/Archived Deletion History & Pending Offline Queue */}
      <DeletionHistoryModal
        isOpen={historyModalOpen}
        onClose={() => setHistoryModalOpen(false)}
      />

      {/* GitHub Auto-Update Modal */}
      <UpdateModal
        updateInfo={updateInfo}
        onClose={() => setUpdateInfo(null)}
      />
    </div>
  );
};
export default App;
