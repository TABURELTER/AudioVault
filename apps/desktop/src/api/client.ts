import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type {
  Book,
  BookDetail,
  Volume,
  DuplicatePair,
  Issue,
  OptimizerItem,
  Task,
  TorrentResult,
  Download,
  Settings,
  Stats,
} from './types';

// In-memory cache for loaded cover base64 strings
const coverCache = new Map<string, string>();

// Tauri IPC Client with fallbacks
export const api = {
  async getLibrary(filter = 'all', sort = 'author', search = '', limit = 5000, offset = 0): Promise<Book[]> {
    try {
      return await invoke<Book[]>('get_library_books', { filter, sort, search, limit, offset });
    } catch (e) {
      console.error('getLibrary error:', e);
      return [];
    }
  },

  async searchBooks(query: string): Promise<Book[]> {
    try {
      return await invoke<Book[]>('search_books', { query });
    } catch (e) {
      console.error('searchBooks error:', e);
      return [];
    }
  },

  async getBookDetails(bookId: number): Promise<BookDetail | null> {
    try {
      return await invoke<BookDetail | null>('get_book_details', { bookId });
    } catch (e) {
      console.error('getBookDetails error:', e);
      return null;
    }
  },

  async getVolumes(): Promise<Volume[]> {
    try {
      return await invoke<Volume[]>('get_volumes');
    } catch (e) {
      console.error('getVolumes error:', e);
      return [];
    }
  },

  async addStorageFolder(path: string): Promise<boolean> {
    return await invoke<boolean>('add_storage_folder', { path });
  },

  async pickFolder(): Promise<string | null> {
    try {
      return await invoke<string | null>('pick_folder');
    } catch (e) {
      console.error('pickFolder error:', e);
      return null;
    }
  },

  async scanVolume(volumeId: number): Promise<number> {
    return await invoke<number>('scan_volume', { volumeId });
  },

  async scanAll(): Promise<number[]> {
    return await invoke<number[]>('scan_all');
  },

  async getDuplicates(): Promise<DuplicatePair[]> {
    try {
      return await invoke<DuplicatePair[]>('get_duplicates');
    } catch (e) {
      console.error('getDuplicates error:', e);
      return [];
    }
  },

  async findAllDuplicates(): Promise<number> {
    return await invoke<number>('find_all_duplicates');
  },

  async resolveDuplicate(dupId: number, status: string): Promise<boolean> {
    return await invoke<boolean>('resolve_duplicate', { dupId, status });
  },

  async getIssues(): Promise<Issue[]> {
    try {
      return await invoke<Issue[]>('get_issues');
    } catch (e) {
      console.error('getIssues error:', e);
      return [];
    }
  },

  async ignoreIssue(issueId: number): Promise<boolean> {
    return await invoke<boolean>('ignore_issue', { issueId });
  },

  async getOptimizations(): Promise<OptimizerItem[]> {
    try {
      return await invoke<OptimizerItem[]>('get_optimizations');
    } catch (e) {
      console.error('getOptimizations error:', e);
      return [];
    }
  },

  async startTranscode(optId: number): Promise<number> {
    return await invoke<number>('start_transcode', { optId });
  },

  async getTasks(): Promise<Task[]> {
    try {
      return await invoke<Task[]>('get_tasks');
    } catch (e) {
      console.error('getTasks error:', e);
      return [];
    }
  },

  async cancelTask(taskId: number): Promise<boolean> {
    return await invoke<boolean>('cancel_task', { taskId });
  },

  async searchTorrents(query: string): Promise<TorrentResult[]> {
    try {
      return await invoke<TorrentResult[]>('search_torrents', { query });
    } catch (e) {
      console.error('searchTorrents error:', e);
      return [];
    }
  },

  async downloadTorrent(topicId: string): Promise<boolean> {
    return await invoke<boolean>('download_torrent', { topicId });
  },

  async getDownloads(): Promise<Download[]> {
    try {
      return await invoke<Download[]>('get_downloads');
    } catch (e) {
      console.error('getDownloads error:', e);
      return [];
    }
  },

  async pauseDownload(id: number): Promise<boolean> {
    return await invoke<boolean>('pause_download', { id });
  },

  async resumeDownload(id: number): Promise<boolean> {
    return await invoke<boolean>('resume_download', { id });
  },

  async deleteDownload(id: number): Promise<boolean> {
    return await invoke<boolean>('delete_download', { id });
  },

  async getSettings(): Promise<Settings> {
    try {
      return await invoke<Settings>('get_settings');
    } catch (e) {
      return {
        rutracker_username: '',
        ffmpeg_path: '',
        ffmpeg_version: 'Не найден',
        ffmpeg_ok: false,
        theme: 'dark',
        db_path: '',
        logs_path: '',
      };
    }
  },

  async saveSettings(settings: Partial<Settings>): Promise<boolean> {
    return await invoke<boolean>('save_settings', { settings });
  },

  async saveCredentials(username: string, password: string): Promise<boolean> {
    return await invoke<boolean>('save_credentials', { username, password });
  },

  async getStats(): Promise<Stats> {
    try {
      return await invoke<Stats>('get_stats');
    } catch (e) {
      return {
        total_books: 0,
        total_copies: 0,
        total_duration_ms: 0,
        total_size_bytes: 0,
        online_volumes: 0,
        offline_volumes: 0,
        duplicates_count: 0,
        issues_count: 0,
      };
    }
  },

  async revealFolder(path: string): Promise<void> {
    await invoke('reveal_folder', { path });
  },

  async openFile(path: string): Promise<boolean> {
    try {
      await invoke('open_file', { path });
      return true;
    } catch (e) {
      console.error('openFile error:', e);
      return false;
    }
  },

  async getCover(path?: string): Promise<string | null> {
    if (!path) return null;
    if (coverCache.has(path)) {
      return coverCache.get(path)!;
    }
    try {
      const data = await invoke<string | null>('get_cover_data', { path });
      if (data) {
        coverCache.set(path, data);
      }
      return data;
    } catch {
      return null;
    }
  },

  onEvent(handler: (event: string) => void) {
    try {
      return listen<string>('app_event', (e) => {
        handler(e.payload);
      });
    } catch {
      return Promise.resolve(() => {});
    }
  }
};

export function formatBytes(bytes: number): string {
  if (!bytes || bytes <= 0) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(1024));
  return `${(bytes / Math.pow(1024, i)).toFixed(i > 1 ? 1 : 0)} ${units[i]}`;
}

export function formatDuration(ms: number): string {
  if (!ms || ms <= 0) return '0м';
  const totalSeconds = Math.floor(ms / 1000);
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  if (hours > 0) {
    return `${hours}ч ${minutes}м`;
  }
  return `${minutes}м`;
}
