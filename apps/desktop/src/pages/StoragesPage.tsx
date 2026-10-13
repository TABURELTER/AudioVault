import React from 'react';
import {
  HardDrive,
  FolderPlus,
  RefreshCw,
  FolderOpen,
  BookOpen,
} from 'lucide-react';
import type { Volume } from '../api/types';
import { formatBytes } from '../api/client';

interface StoragesPageProps {
  volumes: Volume[];
  onRefresh: () => void;
  onAddFolder: () => void;
  onScanVolume: (id: number) => void;
  onRevealFolder?: (path: string) => void;
  onOpenVolumeBooks?: (volumeLabel: string) => void;
}

export const StoragesPage: React.FC<StoragesPageProps> = ({
  volumes,
  onRefresh,
  onAddFolder,
  onScanVolume,
  onRevealFolder,
  onOpenVolumeBooks,
}) => {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: '24px' }}>
      {/* Page Header */}
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
        <div>
          <h2 style={{ fontSize: '20px', fontWeight: 700, color: '#fff' }}>
            Хранилища и носители
          </h2>
          <p style={{ fontSize: '13px', color: 'var(--text-muted)' }}>
            AudioVault помнит отключенные внешние диски и флешки. Книги остаются доступны для поиска.
          </p>
        </div>

        <div style={{ display: 'flex', gap: '10px' }}>
          <button onClick={onRefresh} className="btn btn-secondary">
            <RefreshCw size={14} />
            <span>Обновить</span>
          </button>
          <button onClick={onAddFolder} className="btn btn-primary">
            <FolderPlus size={14} />
            <span>Добавить папку</span>
          </button>
        </div>
      </div>

      {/* Volumes Cards Grid */}
      <div style={{ display: 'flex', flexDirection: 'column', gap: '16px' }}>
        {volumes.map((v) => {
          const usedBytes = Math.max(0, v.capacity_bytes - v.free_bytes);
          const usedPct = v.capacity_bytes > 0 ? (usedBytes / v.capacity_bytes) * 100 : 0;

          return (
            <div
              key={v.id}
              className="glass-card"
              style={{
                padding: '24px',
                display: 'flex',
                flexDirection: 'column',
                gap: '16px',
              }}
            >
              {/* Top Row: Icon, Label, Status, Scan Button */}
              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                <div style={{ display: 'flex', alignItems: 'center', gap: '14px' }}>
                  <div
                    style={{
                      width: '44px',
                      height: '44px',
                      borderRadius: '12px',
                      background: v.online ? 'rgba(99, 102, 241, 0.15)' : 'rgba(255, 255, 255, 0.05)',
                      display: 'flex',
                      alignItems: 'center',
                      justifyContent: 'center',
                      color: v.online ? 'var(--accent-primary)' : 'var(--text-muted)',
                      border: '1px solid var(--border-subtle)',
                    }}
                  >
                    <HardDrive size={24} />
                  </div>

                  <div>
                    <div style={{ display: 'flex', alignItems: 'center', gap: '10px' }}>
                      <h3 style={{ fontSize: '16px', fontWeight: 700, color: '#fff' }}>
                        {v.label}
                      </h3>
                      <span className={`badge ${v.online ? 'badge-online' : 'badge-offline'}`}>
                        {v.online ? 'В сети' : 'Отключен'}
                      </span>
                      <span className="badge badge-format">
                        {v.fs_type}
                      </span>
                    </div>
                    <p style={{ fontSize: '12px', color: 'var(--text-muted)', marginTop: '2px' }}>
                      Точка монтирования: <strong style={{ color: 'var(--text-secondary)' }}>{v.mount_path}</strong>
                    </p>
                  </div>
                </div>

                <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                  {v.online && onRevealFolder && (
                    <button
                      onClick={() => onRevealFolder(v.mount_path)}
                      className="btn btn-secondary"
                      title="Открыть том в Проводнике Windows"
                      style={{ padding: '6px 12px', fontSize: '12px' }}
                    >
                      <FolderOpen size={14} />
                      <span>В Проводнике</span>
                    </button>
                  )}
                  {onOpenVolumeBooks && (
                    <button
                      onClick={() => onOpenVolumeBooks(v.label)}
                      className="btn btn-secondary"
                      title="Показать аудиокниги этого тома"
                      style={{ padding: '6px 12px', fontSize: '12px' }}
                    >
                      <BookOpen size={14} />
                      <span>Книги ({v.book_count})</span>
                    </button>
                  )}
                  <button
                    onClick={() => onScanVolume(v.id)}
                    className="btn btn-secondary"
                    disabled={!v.online}
                    style={{ opacity: v.online ? 1 : 0.5, padding: '6px 12px', fontSize: '12px' }}
                  >
                    <RefreshCw size={14} />
                    <span>Сканировать</span>
                  </button>
                </div>
              </div>

              {/* Progress Bar for Storage Space */}
              <div>
                <div
                  style={{
                    display: 'flex',
                    justifyContent: 'space-between',
                    fontSize: '12px',
                    color: 'var(--text-muted)',
                    marginBottom: '8px',
                  }}
                >
                  <span>
                    Использовано: <strong style={{ color: '#fff' }}>{formatBytes(usedBytes)}</strong> из {formatBytes(v.capacity_bytes)}
                  </span>
                  <span>
                    Свободно: <strong style={{ color: 'var(--success)' }}>{formatBytes(v.free_bytes)}</strong> ({100 - Math.round(usedPct)}%)
                  </span>
                </div>

                <div
                  style={{
                    width: '100%',
                    height: '8px',
                    background: 'rgba(255, 255, 255, 0.08)',
                    borderRadius: '9999px',
                    overflow: 'hidden',
                  }}
                >
                  <div
                    style={{
                      width: `${usedPct}%`,
                      height: '100%',
                      background: usedPct > 90 ? 'var(--danger)' : usedPct > 75 ? 'var(--warning)' : 'var(--accent-gradient)',
                      borderRadius: '9999px',
                      transition: 'width 0.3s ease',
                    }}
                  />
                </div>
              </div>

              {/* Stats Footer */}
              <div
                style={{
                  display: 'flex',
                  gap: '24px',
                  fontSize: '12px',
                  color: 'var(--text-secondary)',
                  borderTop: '1px solid var(--border-subtle)',
                  paddingTop: '12px',
                }}
              >
                <div>
                  Книг в каталоге: <strong style={{ color: '#fff' }}>{v.book_count}</strong>
                </div>
                <div>
                  Папок библиотеки: <strong style={{ color: '#fff' }}>{v.roots_count}</strong>
                </div>
                <div>
                  Тип носителя: <strong style={{ color: '#fff' }}>{v.storage_class.toUpperCase()}</strong>
                </div>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
};
