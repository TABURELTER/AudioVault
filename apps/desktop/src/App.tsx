import React, { useState, useEffect, useCallback } from 'react';
import { Sidebar } from './components/Sidebar';
import { Header } from './components/Header';
import { BookInspectorDrawer } from './components/BookInspectorDrawer';

// Pages
import { LibraryPage } from './pages/LibraryPage';
import { SearchPage } from './pages/SearchPage';
import { StoragesPage } from './pages/StoragesPage';
import { DuplicatesPage } from './pages/DuplicatesPage';
import { IssuesPage } from './pages/IssuesPage';
import { RuTrackerPage } from './pages/RuTrackerPage';
import { DownloadsPage } from './pages/DownloadsPage';
import { OptimizerPage } from './pages/OptimizerPage';
import { TasksPage } from './pages/TasksPage';
import { SettingsPage } from './pages/SettingsPage';

// API & Types
import { api } from './api/client';
import type {
  Book,
  BookDetail,
  Volume,
  DuplicatePair,
  OptimizerItem,
  Task,
  Stats,
} from './api/types';
import { FolderPlus, X } from 'lucide-react';

const initialStats: Stats = {
  total_books: 0,
  total_copies: 0,
  total_duration_ms: 0,
  total_size_bytes: 0,
  online_volumes: 0,
  offline_volumes: 0,
  duplicates_count: 0,
  issues_count: 0,
};

export const App: React.FC = () => {
  const [currentPage, setCurrentPage] = useState<string>('library');
  const [stats, setStats] = useState<Stats>(initialStats);
  const [volumes, setVolumes] = useState<Volume[]>([]);
  const [books, setBooks] = useState<Book[]>([]);
  const [tasks, setTasks] = useState<Task[]>([]);
  const [duplicates, setDuplicates] = useState<DuplicatePair[]>([]);
  const [optimizerItems, setOptimizerItems] = useState<OptimizerItem[]>([]);

  // Filtering & Sorting
  const [activeFilter, setActiveFilter] = useState<string>('all');
  const [activeSort, setActiveSort] = useState<string>('author');
  const [searchQuery, setSearchQuery] = useState<string>('');

  // Book details inspector
  const [selectedBookId, setSelectedBookId] = useState<number | null>(null);
  const [selectedBook, setSelectedBook] = useState<BookDetail | null>(null);

  // Status flags
  const [isScanning, setIsScanning] = useState(false);
  const [isSearchingDuplicates, setIsSearchingDuplicates] = useState(false);

  // Folder modal
  const [folderModalOpen, setFolderModalOpen] = useState(false);
  const [manualFolderPath, setManualFolderPath] = useState('');
  const [addFolderError, setAddFolderError] = useState<string | null>(null);

  // Reload core data
  const reloadStats = useCallback(async () => {
    try {
      const data = await api.getStats();
      setStats(data);
    } catch (e) {
      console.error('reloadStats error:', e);
    }
  }, []);

  const reloadVolumes = useCallback(async () => {
    try {
      const data = await api.getVolumes();
      setVolumes(data);
    } catch (e) {
      console.error('reloadVolumes error:', e);
    }
  }, []);

  const reloadBooks = useCallback(async () => {
    try {
      const data = await api.getLibrary(activeFilter, activeSort, searchQuery);
      setBooks(data);
    } catch (e) {
      console.error('reloadBooks error:', e);
    }
  }, [activeFilter, activeSort, searchQuery]);

  const reloadTasks = useCallback(async () => {
    try {
      const data = await api.getTasks();
      setTasks(data);
    } catch (e) {
      console.error('reloadTasks error:', e);
    }
  }, []);

  const reloadDuplicates = useCallback(async () => {
    try {
      const data = await api.getDuplicates();
      setDuplicates(data);
    } catch (e) {
      console.error('reloadDuplicates error:', e);
    }
  }, []);

  const reloadOptimizer = useCallback(async () => {
    try {
      const data = await api.getOptimizations();
      setOptimizerItems(data);
    } catch (e) {
      console.error('reloadOptimizer error:', e);
    }
  }, []);

  // Initial load
  useEffect(() => {
    reloadStats();
    reloadVolumes();
    reloadBooks();
    reloadTasks();
    reloadDuplicates();
    reloadOptimizer();
  }, [reloadStats, reloadVolumes, reloadBooks, reloadTasks, reloadDuplicates, reloadOptimizer]);

  // Subscribe to backend events
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    api.onEvent((event) => {
      if (event === 'library_changed') {
        reloadBooks();
        reloadStats();
      } else if (event === 'volumes_changed') {
        reloadVolumes();
        reloadStats();
      } else if (event === 'duplicates_changed') {
        reloadDuplicates();
        reloadStats();
      } else if (event === 'jobs_changed') {
        reloadTasks();
        reloadStats();
      } else if (event === 'issues_changed') {
        reloadStats();
      }
    }).then((un) => {
      unlisten = un;
    });

    return () => {
      if (unlisten) unlisten();
    };
  }, [reloadBooks, reloadVolumes, reloadDuplicates, reloadTasks, reloadStats]);

  // Book selection
  const handleSelectBook = async (bookId: number) => {
    setSelectedBookId(bookId);
    try {
      const detail = await api.getBookDetails(bookId);
      setSelectedBook(detail);
    } catch (e) {
      console.error('Failed to load book details:', e);
    }
  };

  const handleCloseDrawer = () => {
    setSelectedBookId(null);
    setSelectedBook(null);
  };

  // Scan triggers
  const handleScanAll = async () => {
    setIsScanning(true);
    try {
      await api.scanAll();
      await reloadTasks();
      await reloadStats();
    } catch (e) {
      console.error('handleScanAll error:', e);
    } finally {
      setIsScanning(false);
    }
  };

  const handleScanVolume = async (volumeId: number) => {
    try {
      await api.scanVolume(volumeId);
      await reloadTasks();
    } catch (e) {
      console.error('handleScanVolume error:', e);
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
      console.error('handleFindDuplicates error:', e);
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
      console.error('handleResolveDuplicate error:', e);
    }
  };

  const handleTranscode = async (copyId: number) => {
    try {
      await api.startTranscode(copyId);
      await reloadTasks();
      setCurrentPage('tasks');
    } catch (e) {
      console.error('handleTranscode error:', e);
    }
  };

  const handleCancelTask = async (taskId: number) => {
    try {
      await api.cancelTask(taskId);
      await reloadTasks();
    } catch (e) {
      console.error('handleCancelTask error:', e);
    }
  };

  const handleRevealFolder = async (path: string) => {
    try {
      await api.revealFolder(path);
    } catch (e) {
      console.error('handleRevealFolder error:', e);
    }
  };

  const handlePlayTrack = async (_fileName: string, path: string) => {
    try {
      await api.openFile(path);
    } catch (e) {
      console.error('handlePlayTrack error:', e);
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
    } catch {
      // Fallback to manual entry modal
    }
    setAddFolderError(null);
    setManualFolderPath('');
    setFolderModalOpen(true);
  };

  const handleManualAddFolder = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!manualFolderPath.trim()) return;
    try {
      setAddFolderError(null);
      await api.addStorageFolder(manualFolderPath.trim());
      setFolderModalOpen(false);
      setManualFolderPath('');
      await reloadVolumes();
      await reloadBooks();
      await reloadStats();
    } catch (err) {
      setAddFolderError(String(err));
    }
  };

  // Page titles
  const getPageInfo = (): { title: string; subtitle: string } => {
    switch (currentPage) {
      case 'library':
        return {
          title: 'Фонотека',
          subtitle: 'Каталог аудиокниг со всех подключенных и оффлайн дисков',
        };
      case 'search':
        return {
          title: 'Поиск книг',
          subtitle: 'Мгновенный поиск по SQLite FTS5 с поддержкой транслитерации',
        };
      case 'storages':
        return {
          title: 'Хранилища и носители',
          subtitle: 'Управление подключенными и архивными дисками',
        };
      case 'duplicates':
        return {
          title: 'Потенциальные дубликаты',
          subtitle: 'Сравнение хэшей, аудио-отпечатков Chromaprint и метаданных',
        };
      case 'issues':
        return {
          title: 'Проблемы метаданных',
          subtitle: 'Ошибки распознавания тегов и структуры папок',
        };
      case 'rutracker':
        return {
          title: 'RuTracker',
          subtitle: 'Поиск раздач и релизов аудиокниг',
        };
      case 'downloads':
        return {
          title: 'Менеджер загрузок',
          subtitle: 'Встроенный торрент-клиент AudioVault',
        };
      case 'optimizer':
        return {
          title: 'Оптимизация и конвертация',
          subtitle: 'Анализ потенциальной экономии места и сборка M4B',
        };
      case 'tasks':
        return {
          title: 'Очередь задач',
          subtitle: 'Активные фоновые операции и сканирование',
        };
      case 'settings':
        return {
          title: 'Настройки',
          subtitle: 'Конфигурация учетных записей, путей и FFmpeg',
        };
      default:
        return { title: 'AudioVault', subtitle: 'Менеджер аудиокниг' };
    }
  };

  const { title, subtitle } = getPageInfo();

  return (
    <div className="app-container">
      {/* Navigation Sidebar */}
      <Sidebar
        currentPage={currentPage}
        onNavigate={setCurrentPage}
        stats={stats}
        volumes={volumes}
      />

      {/* Main Content Area */}
      <main className="main-content">
        <Header
          title={title}
          subtitle={subtitle}
          searchQuery={searchQuery}
          onSearchChange={(q) => {
            setSearchQuery(q);
            if (currentPage !== 'library' && q.trim().length > 0) {
              setCurrentPage('library');
            }
          }}
          onScanAll={handleScanAll}
          onFindDuplicates={handleFindDuplicates}
          activeTasks={tasks}
          isScanning={isScanning}
        />

        <div className="page-container">
          {currentPage === 'library' && (
            <LibraryPage
              books={books}
              onSelectBook={handleSelectBook}
              activeFilter={activeFilter}
              onFilterChange={setActiveFilter}
              activeSort={activeSort}
              onSortChange={setActiveSort}
            />
          )}

          {currentPage === 'search' && (
            <SearchPage
              onSelectBook={handleSelectBook}
              onSearch={api.searchBooks}
            />
          )}

          {currentPage === 'storages' && (
            <StoragesPage
              volumes={volumes}
              onRefresh={async () => {
                await reloadVolumes();
                await reloadStats();
              }}
              onAddFolder={handleAddFolderClick}
              onScanVolume={handleScanVolume}
              onRevealFolder={handleRevealFolder}
              onOpenVolumeBooks={(label) => {
                setSearchQuery(label);
                setCurrentPage('library');
              }}
            />
          )}

          {currentPage === 'duplicates' && (
            <DuplicatesPage
              duplicates={duplicates}
              onFindDuplicates={handleFindDuplicates}
              onResolve={handleResolveDuplicate}
              isSearching={isSearchingDuplicates}
            />
          )}

          {currentPage === 'issues' && (
            <IssuesPage
              onOpenBook={(id) => {
                handleSelectBook(id);
              }}
            />
          )}

          {currentPage === 'rutracker' && (
            <RuTrackerPage
              onNavigateToDownloads={() => setCurrentPage('downloads')}
            />
          )}

          {currentPage === 'downloads' && (
            <DownloadsPage
              onNavigateToRuTracker={() => setCurrentPage('rutracker')}
            />
          )}

          {currentPage === 'optimizer' && (
            <OptimizerPage
              items={optimizerItems}
              onTranscode={handleTranscode}
            />
          )}

          {currentPage === 'tasks' && (
            <TasksPage
              tasks={tasks}
              onCancelTask={handleCancelTask}
            />
          )}

          {currentPage === 'settings' && <SettingsPage />}
        </div>
      </main>

      {/* Book Inspector Details Drawer */}
      <BookInspectorDrawer
        book={selectedBook}
        isOpen={selectedBookId !== null}
        onClose={handleCloseDrawer}
        onOptimizeCopy={handleTranscode}
        onRevealFolder={handleRevealFolder}
        onPlayTrack={handlePlayTrack}
      />

      {/* Add Folder Modal (Dialog) */}
      {folderModalOpen && (
        <div
          style={{
            position: 'fixed',
            inset: 0,
            background: 'rgba(0, 0, 0, 0.7)',
            backdropFilter: 'blur(6px)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            zIndex: 50,
          }}
        >
          <div
            className="glass-card"
            style={{
              width: '480px',
              padding: '24px',
              display: 'flex',
              flexDirection: 'column',
              gap: '16px',
            }}
          >
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: '10px' }}>
                <FolderPlus size={20} color="var(--accent-primary)" />
                <h3 style={{ fontSize: '17px', fontWeight: 700, color: '#fff' }}>
                  Добавить папку библиотеки
                </h3>
              </div>
              <button
                onClick={() => setFolderModalOpen(false)}
                className="btn btn-icon"
                style={{ width: '30px', height: '30px' }}
              >
                <X size={16} />
              </button>
            </div>

            <p style={{ fontSize: '13px', color: 'var(--text-muted)' }}>
              Укажите абсолютный путь к папке с аудиокнигами (например, <code>G:\аудио</code> или <code>F:\МОИ КНИГИ</code>).
            </p>

            <form onSubmit={handleManualAddFolder} style={{ display: 'flex', flexDirection: 'column', gap: '12px' }}>
              <input
                type="text"
                placeholder="D:\Audiobooks или G:\аудио..."
                value={manualFolderPath}
                onChange={(e) => setManualFolderPath(e.target.value)}
                className="input-text"
                autoFocus
              />

              {addFolderError && (
                <div style={{ fontSize: '12px', color: 'var(--danger)', padding: '6px 10px', background: 'var(--danger-bg)', borderRadius: '6px' }}>
                  {addFolderError}
                </div>
              )}

              <div style={{ display: 'flex', justifyContent: 'flex-end', gap: '10px', marginTop: '8px' }}>
                <button
                  type="button"
                  onClick={() => setFolderModalOpen(false)}
                  className="btn btn-secondary"
                >
                  Отмена
                </button>
                <button type="submit" className="btn btn-primary">
                  Добавить
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
    </div>
  );
};

export default App;
