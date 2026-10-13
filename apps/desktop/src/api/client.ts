import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
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
  EbookDetail,
  EbookStats,
  EbookSearchResult,
  DeletionReport,
  DeletionHistoryEntry,
  PendingDeletion,
  DeletionCounts,
} from "./types";

// In-memory cache for loaded cover base64 strings
const coverCache = new Map<string, string>();
const coverRequests = new Map<string, Promise<string | null>>();

// Tauri IPC Client with fallbacks
export const api = {
  async getLibrary(
    filter = "all",
    sort = "author",
    search = "",
    limit = 5000,
    offset = 0,
  ): Promise<Book[]> {
    try {
      return await invoke<Book[]>("get_library_books", {
        filter,
        sort,
        search,
        limit,
        offset,
      });
    } catch (e) {
      console.error("getLibrary error:", e);
      throw e;
    }
  },

  async searchBooks(query: string): Promise<Book[]> {
    try {
      return await invoke<Book[]>("search_books", { query });
    } catch (e) {
      console.error("searchBooks error:", e);
      throw e;
    }
  },

  async getBookDetails(bookId: number): Promise<BookDetail | null> {
    try {
      return await invoke<BookDetail | null>("get_book_details", { bookId });
    } catch (e) {
      console.error("getBookDetails error:", e);
      return null;
    }
  },

  async getVolumes(): Promise<Volume[]> {
    try {
      return await invoke<Volume[]>("get_volumes");
    } catch (e) {
      console.error("getVolumes error:", e);
      throw e;
    }
  },

  async addStorageFolder(path: string): Promise<boolean> {
    return await invoke<boolean>("add_storage_folder", { path });
  },

  async renameVolume(volumeId: number, newName: string): Promise<boolean> {
    return await invoke<boolean>("rename_volume", { volumeId, newName });
  },

  async resetCatalog(): Promise<boolean> {
    return await invoke<boolean>("reset_catalog");
  },

  async pickFolder(): Promise<string | null> {
    try {
      return await invoke<string | null>("pick_folder");
    } catch (e) {
      console.error("pickFolder error:", e);
      throw e;
    }
  },

  async scanVolume(volumeId: number): Promise<number> {
    return await invoke<number>("scan_volume", { volumeId });
  },

  async scanAll(): Promise<number[]> {
    return await invoke<number[]>("scan_all");
  },

  async getDuplicates(): Promise<DuplicatePair[]> {
    try {
      return await invoke<DuplicatePair[]>("get_duplicates");
    } catch (e) {
      console.error("getDuplicates error:", e);
      throw e;
    }
  },

  async findAllDuplicates(): Promise<number> {
    return await invoke<number>("find_all_duplicates");
  },

  async resolveDuplicate(dupId: number, status: string): Promise<boolean> {
    return await invoke<boolean>("resolve_duplicate", { dupId, status });
  },

  async getIssues(kind = "all", offset = 0, limit = 100): Promise<Issue[]> {
    try {
      return await invoke<Issue[]>("get_issues", {
        kind: kind === "all" ? null : kind,
        offset,
        limit,
      });
    } catch (e) {
      console.error("getIssues error:", e);
      throw e;
    }
  },

  async ignoreIssue(issueId: number): Promise<boolean> {
    return await invoke<boolean>("ignore_issue", { issueId });
  },

  async getOptimizations(): Promise<OptimizerItem[]> {
    try {
      return await invoke<OptimizerItem[]>("get_optimizations");
    } catch (e) {
      console.error("getOptimizations error:", e);
      throw e;
    }
  },

  async startTranscode(optId: number): Promise<number> {
    return await invoke<number>("start_transcode", { optId });
  },

  async transcodeCopy(copyId: number, format?: string): Promise<number> {
    return await invoke<number>("transcode_copy", { copyId, format });
  },

  async getTasks(): Promise<Task[]> {
    try {
      return await invoke<Task[]>("get_tasks");
    } catch (e) {
      console.error("getTasks error:", e);
      throw e;
    }
  },

  async cancelTask(taskId: number): Promise<boolean> {
    return await invoke<boolean>("cancel_task", { taskId });
  },

  async searchTorrents(query: string): Promise<TorrentResult[]> {
    try {
      return await invoke<TorrentResult[]>("search_torrents", { query });
    } catch (e) {
      console.error("searchTorrents error:", e);
      throw e;
    }
  },

  async downloadTorrent(topicId: string): Promise<boolean> {
    return await invoke<boolean>("download_torrent", { topicId });
  },

  async getDownloads(): Promise<Download[]> {
    try {
      return await invoke<Download[]>("get_downloads");
    } catch (e) {
      console.error("getDownloads error:", e);
      throw e;
    }
  },

  async pauseDownload(id: number): Promise<boolean> {
    return await invoke<boolean>("pause_download", { id });
  },

  async resumeDownload(id: number): Promise<boolean> {
    return await invoke<boolean>("resume_download", { id });
  },

  async deleteDownload(id: number): Promise<boolean> {
    return await invoke<boolean>("delete_download", { id });
  },

  async getSettings(): Promise<Settings> {
    return await invoke<Settings>("get_settings");
  },

  async saveSettings(settings: Partial<Settings>): Promise<boolean> {
    return await invoke<boolean>("save_settings", { settings });
  },

  async saveCredentials(username: string, password: string): Promise<boolean> {
    return await invoke<boolean>("save_credentials", { username, password });
  },

  async testRutrackerConnection(): Promise<string> {
    return await invoke<string>("test_rutracker_connection");
  },

  async getStats(): Promise<Stats> {
    try {
      return await invoke<Stats>("get_stats");
    } catch {
      return {
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
    }
  },

  async revealFolder(path: string): Promise<void> {
    await invoke("reveal_folder", { path });
  },

  async openFile(path: string): Promise<boolean> {
    try {
      await invoke("open_file", { path });
      return true;
    } catch (e) {
      console.error("openFile error:", e);
      throw e;
    }
  },

  async getCover(path?: string): Promise<string | null> {
    if (!path) return null;
    if (coverCache.has(path)) return coverCache.get(path)!;
    if (coverRequests.has(path)) return coverRequests.get(path)!;
    const request = invoke<string | null>("get_cover_data", { path })
      .then((data) => {
        if (data) {
          if (coverCache.size >= 128)
            coverCache.delete(coverCache.keys().next().value!);
          coverCache.set(path, data);
        }
        return data;
      })
      .catch(() => null)
      .finally(() => coverRequests.delete(path));
    coverRequests.set(path, request);
    return request;
  },

  async searchEbooks(
    query?: string,
    formatFilter?: string,
    sortBy?: string,
    readFilter?: boolean,
    candidateFilter?: boolean,
    limit?: number,
    offset?: number,
  ): Promise<EbookSearchResult> {
    try {
      return await invoke<EbookSearchResult>("search_ebooks", {
        query: query?.trim() ? query.trim() : null,
        formatFilter:
          formatFilter && formatFilter !== "all" ? formatFilter : null,
        sortBy: sortBy || "author",
        readFilter: readFilter ?? null,
        candidateFilter: candidateFilter ?? null,
        limit: limit ?? 60,
        offset: offset ?? 0,
      });
    } catch (e) {
      console.error("searchEbooks error:", e);
      throw e;
    }
  },

  async getEbookDetail(ebookId: number): Promise<EbookDetail | null> {
    try {
      return await invoke<EbookDetail | null>("get_ebook_detail", { ebookId });
    } catch (e) {
      console.error("getEbookDetail error:", e);
      return null;
    }
  },

  async getEbookStats(): Promise<EbookStats | null> {
    try {
      return await invoke<EbookStats | null>("get_ebook_stats");
    } catch (e) {
      console.error("getEbookStats error:", e);
      return null;
    }
  },

  async setBookListened(bookId: number, listened: boolean): Promise<boolean> {
    try {
      return await invoke<boolean>("set_book_listened", { bookId, listened });
    } catch (e) {
      console.error("setBookListened error:", e);
      throw e;
    }
  },

  async setEbookRead(ebookId: number, read: boolean): Promise<boolean> {
    try {
      return await invoke<boolean>("set_ebook_read", { ebookId, read });
    } catch (e) {
      console.error("setEbookRead error:", e);
      throw e;
    }
  },

  async setBookMarkedForDeletion(bookId: number, marked: boolean): Promise<boolean> {
    try {
      return await invoke<boolean>("set_book_marked_for_deletion", { bookId, marked });
    } catch (e) {
      console.error("setBookMarkedForDeletion error:", e);
      throw e;
    }
  },

  async setEbookMarkedForDeletion(ebookId: number, marked: boolean): Promise<boolean> {
    try {
      return await invoke<boolean>("set_ebook_marked_for_deletion", { ebookId, marked });
    } catch (e) {
      console.error("setEbookMarkedForDeletion error:", e);
      throw e;
    }
  },

  async setMultipleBooksMarkedForDeletion(bookIds: number[], marked: boolean): Promise<boolean> {
    try {
      return await invoke<boolean>("set_multiple_books_marked_for_deletion", { bookIds, marked });
    } catch (e) {
      console.error("setMultipleBooksMarkedForDeletion error:", e);
      throw e;
    }
  },

  async setMultipleEbooksMarkedForDeletion(ebookIds: number[], marked: boolean): Promise<boolean> {
    try {
      return await invoke<boolean>("set_multiple_ebooks_marked_for_deletion", { ebookIds, marked });
    } catch (e) {
      console.error("setMultipleEbooksMarkedForDeletion error:", e);
      throw e;
    }
  },

  async executeDeletion(bookIds: number[], ebookIds: number[]): Promise<DeletionReport> {
    try {
      return await invoke<DeletionReport>("execute_deletion", { bookIds, ebookIds });
    } catch (e) {
      console.error("executeDeletion error:", e);
      throw e;
    }
  },

  async executePendingDeletionsForVolume(volumeId: number): Promise<number> {
    try {
      return await invoke<number>("execute_pending_deletions_for_volume", { volumeId });
    } catch (e) {
      console.error("executePendingDeletionsForVolume error:", e);
      throw e;
    }
  },

  async getDeletionHistory(limit?: number, offset?: number): Promise<DeletionHistoryEntry[]> {
    try {
      return await invoke<DeletionHistoryEntry[]>("get_deletion_history", { limit, offset });
    } catch (e) {
      console.error("getDeletionHistory error:", e);
      return [];
    }
  },

  async clearDeletionHistory(): Promise<boolean> {
    try {
      return await invoke<boolean>("clear_deletion_history");
    } catch (e) {
      console.error("clearDeletionHistory error:", e);
      return false;
    }
  },

  async getPendingDeletions(): Promise<PendingDeletion[]> {
    try {
      return await invoke<PendingDeletion[]>("get_pending_deletions");
    } catch (e) {
      console.error("getPendingDeletions error:", e);
      return [];
    }
  },

  async getDeletionCounts(): Promise<DeletionCounts | null> {
    try {
      return await invoke<DeletionCounts>("get_deletion_counts");
    } catch (e) {
      console.error("getDeletionCounts error:", e);
      return null;
    }
  },

  async ignoreIssues(issueIds: number[]): Promise<boolean> {
    try {
      return await invoke<boolean>("ignore_issues", { issueIds });
    } catch (e) {
      console.error("ignoreIssues error:", e);
      throw e;
    }
  },

  async ignoreIssuesByKind(kind?: string): Promise<number> {
    try {
      return await invoke<number>("ignore_issues_by_kind", { kind: kind && kind !== "all" ? kind : null });
    } catch (e) {
      console.error("ignoreIssuesByKind error:", e);
      throw e;
    }
  },

  onEvent(handler: (event: string) => void) {
    try {
      return listen<string>("app_event", (e) => {
        handler(e.payload);
      });
    } catch {
      return Promise.resolve(() => {});
    }
  },
};

export function formatBytes(bytes: number): string {
  if (!bytes || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(1024));
  return `${(bytes / Math.pow(1024, i)).toFixed(i > 1 ? 1 : 0)} ${units[i]}`;
}

export function formatDuration(ms: number): string {
  if (!ms || ms <= 0) return "—";
  const totalSeconds = Math.floor(ms / 1000);
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  if (hours > 0) {
    return `${hours}ч ${minutes}м`;
  }
  return `${minutes}м`;
}
