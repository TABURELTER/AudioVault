export interface Book {
  id: number;
  title: string;
  authors: string;
  narrators: string;
  series?: string;
  series_index?: number;
  duration_ms: number;
  size: number;
  format: string;
  copies_count: number;
  online: boolean;
  volume_name: string;
  cover_thumb?: string;
}

export interface CopyDetail {
  id: number;
  volume_id?: number;
  volume_name: string;
  volume_online: boolean;
  rel_path: string;
  folder_name?: string;
  full_path?: string;
  format: string;
  codec?: string;
  bitrate_kbps?: number;
  total_size: number;
  duration_ms: number;
  audio_file_count: number;
  is_single_file: boolean;
  quality_score: number;
}

export interface MediaFileDetail {
  id: number;
  file_name: string;
  rel_path: string;
  full_path?: string;
  track_no?: number;
  duration_ms?: number;
  size: number;
  bitrate_kbps?: number;
  title?: string;
}

export interface BookDetail {
  id: number;
  title: string;
  authors: string;
  narrators: string;
  series?: string;
  series_index?: number;
  duration_ms: number;
  description?: string;
  cover_thumb?: string;
  copies: CopyDetail[];
  media: MediaFileDetail[];
}

export interface Volume {
  id: number;
  label: string;
  mount_path: string;
  online: boolean;
  storage_class: 'ssd' | 'hdd' | 'removable' | 'network' | 'unknown';
  fs_type: string;
  capacity_bytes: number;
  free_bytes: number;
  book_count: number;
  roots_count: number;
}

export interface DuplicatePair {
  id: number;
  score: number;
  verdict: string;
  evidence: string;
  title: string;
  authors: string;
  a: {
    copy_id: number;
    volume_name: string;
    narrator: string;
    format: string;
    size: number;
    duration_ms: number;
    online: boolean;
    rel_path: string;
  };
  b: {
    copy_id: number;
    volume_name: string;
    narrator: string;
    format: string;
    size: number;
    duration_ms: number;
    online: boolean;
    rel_path: string;
  };
}

export interface Issue {
  id: number;
  book_id: number;
  copy_id?: number;
  kind: string;
  kind_label: string;
  book_title: string;
  book_authors: string;
  message: string;
  field?: string;
  current_val?: string;
  proposed_val?: string;
}

export interface OptimizerItem {
  copy_id: number;
  book_title: string;
  authors: string;
  current_codec: string;
  current_bitrate_kbps?: number;
  current_size: number;
  target_format: string;
  target_bitrate_kbps?: number;
  estimated_size: number;
  savings_bytes: number;
  savings_pct: number;
  notes: string;
  reencode: boolean;
}

export interface Task {
  id: number;
  kind: string;
  label: string;
  description: string;
  progress: number;
  state: 'queued' | 'running' | 'completed' | 'failed' | 'cancelled' | 'paused';
  started_at?: string;
}

export interface TorrentResult {
  id: string;
  title: string;
  author: string;
  reader: string;
  size_bytes: number;
  seeds: number;
  leeches: number;
  bitrate: string;
  category: string;
}

export interface Download {
  id: number;
  name: string;
  state: string;
  progress: number;
  total_bytes: number;
  downloaded_bytes: number;
  down_speed: string;
  up_speed: string;
  eta_str: string;
  seeds: number;
  peers: number;
}

export interface Settings {
  rutracker_username: string;
  rutracker_cookie: string;
  rutracker_proxy: string;
  rutracker_base_url: string;
  ffmpeg_path: string;
  ffmpeg_version: string;
  ffmpeg_ok: boolean;
  theme: string;
  db_path: string;
  logs_path: string;
}

export interface Stats {
  total_books: number;
  total_copies: number;
  total_duration_ms: number;
  total_size_bytes: number;
  online_volumes: number;
  offline_volumes: number;
  duplicates_count: number;
  issues_count: number;
  total_ebooks: number;
}

export interface Ebook {
  id: number;
  volume_id: number;
  volume_name: string;
  volume_online: boolean;
  rel_path: string;
  file_name: string;
  format: string;
  size: number;
  title: string;
  authors: string;
  series?: string;
  series_index?: number;
  year?: number;
  cover_thumb?: string;
  full_path?: string;
}

export interface EbookDetail {
  id: number;
  volume_id: number;
  root_id?: number;
  volume_name: string;
  volume_online: boolean;
  mount_path?: string;
  rel_path: string;
  file_name: string;
  format: string;
  size: number;
  mtime: number;
  title: string;
  authors: string;
  series?: string;
  series_index?: number;
  genres?: string;
  year?: number;
  language?: string;
  description?: string;
  cover_thumb?: string;
  state: string;
  full_path?: string;
  created_at: number;
  updated_at: number;
}

export interface EbookStats {
  total_count: number;
  total_size_bytes: number;
  formats: [string, number][];
  authors_count: number;
}

export interface EbookSearchResult {
  items: Ebook[];
  total_count: number;
}
