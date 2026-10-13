import React, { useState, useEffect } from 'react';
import {
  X,
  Play,
  FolderOpen,
  Zap,
  Clock,
  Disc,
  Mic,
} from 'lucide-react';
import type { BookDetail } from '../api/types';
import { api, formatBytes, formatDuration } from '../api/client';

interface DrawerProps {
  book: BookDetail | null;
  isOpen: boolean;
  onClose: () => void;
  onOptimizeCopy: (copyId: number) => void;
  onRevealFolder: (path: string) => void;
  onPlayTrack?: (fileName: string, path: string) => void;
}

export const BookInspectorDrawer: React.FC<DrawerProps> = ({
  book,
  isOpen,
  onClose,
  onOptimizeCopy,
  onRevealFolder,
  onPlayTrack,
}) => {
  const [activeTab, setActiveTab] = useState<'copies' | 'tracks' | 'desc'>('copies');
  const [coverUrl, setCoverUrl] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    if (book?.cover_thumb) {
      api.getCover(book.cover_thumb).then((url) => {
        if (active && url) {
          setCoverUrl(url);
        }
      });
    } else {
      setCoverUrl(null);
    }
    return () => {
      active = false;
    };
  }, [book?.cover_thumb]);

  if (!isOpen || !book) return null;

  const hash = book.title.split('').reduce((acc, char) => acc + char.charCodeAt(0), 0);
  const hue1 = hash % 360;
  const hue2 = (hue1 + 50) % 360;
  const gradient = `linear-gradient(135deg, hsl(${hue1}, 60%, 25%) 0%, hsl(${hue2}, 75%, 12%) 100%)`;

  return (
    <>
      {/* Backdrop */}
      <div
        onClick={onClose}
        style={{
          position: 'fixed',
          inset: 0,
          background: 'rgba(0, 0, 0, 0.65)',
          backdropFilter: 'blur(4px)',
          zIndex: 40,
          transition: 'opacity 0.25s ease',
        }}
      />

      {/* Drawer Panel */}
      <div
        style={{
          position: 'fixed',
          top: 0,
          right: 0,
          width: '520px',
          height: '100vh',
          background: 'var(--bg-surface)',
          borderLeft: '1px solid var(--border-card)',
          boxShadow: 'var(--shadow-lg)',
          zIndex: 50,
          display: 'flex',
          flexDirection: 'column',
          overflow: 'hidden',
          animation: 'slideIn 0.25s cubic-bezier(0.16, 1, 0.3, 1)',
        }}
      >
        {/* Header Cover Banner */}
        <div
          style={{
            height: '190px',
            background: gradient,
            padding: '24px',
            display: 'flex',
            flexDirection: 'column',
            justifyContent: 'space-between',
            position: 'relative',
            overflow: 'hidden',
          }}
        >
          {coverUrl && (
            <img
              src={coverUrl}
              alt={book.title}
              style={{
                position: 'absolute',
                inset: 0,
                width: '100%',
                height: '100%',
                objectFit: 'cover',
              }}
            />
          )}
          {coverUrl && (
            <div
              style={{
                position: 'absolute',
                inset: 0,
                background: 'linear-gradient(to bottom, rgba(0,0,0,0.7) 0%, rgba(0,0,0,0.3) 40%, rgba(0,0,0,0.85) 100%)',
              }}
            />
          )}

          {/* Close Button */}
          <button
            onClick={onClose}
            className="btn btn-icon"
            style={{
              position: 'absolute',
              top: '16px',
              right: '16px',
              background: 'rgba(0, 0, 0, 0.4)',
              color: '#fff',
              border: 'none',
              zIndex: 2,
            }}
          >
            <X size={18} />
          </button>

          {/* Book Series Badge */}
          {book.series ? (
            <span
              style={{
                alignSelf: 'flex-start',
                fontSize: '11px',
                fontWeight: 600,
                padding: '3px 10px',
                borderRadius: '9999px',
                background: 'rgba(0, 0, 0, 0.4)',
                color: '#e2e8f0',
                backdropFilter: 'blur(8px)',
              }}
            >
              {book.series} {book.series_index ? `#${book.series_index}` : ''}
            </span>
          ) : <div />}

          <div>
            <h2
              style={{
                fontSize: '20px',
                fontWeight: 800,
                color: '#ffffff',
                lineHeight: 1.25,
                marginBottom: '4px',
                textShadow: '0 2px 10px rgba(0,0,0,0.5)',
              }}
            >
              {book.title}
            </h2>
            <p
              style={{
                fontSize: '14px',
                color: '#cbd5e1',
                fontWeight: 600,
              }}
            >
              {book.authors || 'Неизвестный автор'}
            </p>
          </div>
        </div>

        {/* Quick Info Bar */}
        <div
          style={{
            padding: '12px 24px',
            background: 'var(--bg-input)',
            borderBottom: '1px solid var(--border-subtle)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            fontSize: '12px',
            color: 'var(--text-secondary)',
          }}
        >
          {book.narrators && (
            <div style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
              <Mic size={14} color="var(--accent-primary)" />
              <span>{book.narrators}</span>
            </div>
          )}
          <div style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
            <Clock size={14} />
            <span>{formatDuration(book.duration_ms)}</span>
          </div>
          <div style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
            <Disc size={14} />
            <span>{book.copies.length} копии</span>
          </div>
        </div>

        {/* Sub-Tabs */}
        <div
          style={{
            display: 'flex',
            padding: '0 24px',
            borderBottom: '1px solid var(--border-subtle)',
            background: 'var(--bg-surface)',
          }}
        >
          <button
            onClick={() => setActiveTab('copies')}
            style={{
              padding: '12px 16px',
              fontSize: '13px',
              fontWeight: activeTab === 'copies' ? 700 : 500,
              color: activeTab === 'copies' ? 'var(--accent-primary)' : 'var(--text-muted)',
              borderBottom: activeTab === 'copies' ? '2px solid var(--accent-primary)' : '2px solid transparent',
              background: 'transparent',
              borderTop: 'none',
              borderLeft: 'none',
              borderRight: 'none',
              cursor: 'pointer',
            }}
          >
            Копии ({book.copies.length})
          </button>
          <button
            onClick={() => setActiveTab('tracks')}
            style={{
              padding: '12px 16px',
              fontSize: '13px',
              fontWeight: activeTab === 'tracks' ? 700 : 500,
              color: activeTab === 'tracks' ? 'var(--accent-primary)' : 'var(--text-muted)',
              borderBottom: activeTab === 'tracks' ? '2px solid var(--accent-primary)' : '2px solid transparent',
              background: 'transparent',
              borderTop: 'none',
              borderLeft: 'none',
              borderRight: 'none',
              cursor: 'pointer',
            }}
          >
            Файлы ({book.media.length})
          </button>
          {book.description && (
            <button
              onClick={() => setActiveTab('desc')}
              style={{
                padding: '12px 16px',
                fontSize: '13px',
                fontWeight: activeTab === 'desc' ? 700 : 500,
                color: activeTab === 'desc' ? 'var(--accent-primary)' : 'var(--text-muted)',
                borderBottom: activeTab === 'desc' ? '2px solid var(--accent-primary)' : '2px solid transparent',
                background: 'transparent',
                borderTop: 'none',
                borderLeft: 'none',
                borderRight: 'none',
                cursor: 'pointer',
              }}
            >
              Аннотация FB2
            </button>
          )}
        </div>

        {/* Tab Content */}
        <div style={{ flex: 1, overflowY: 'auto', padding: '20px 24px' }}>
          {activeTab === 'copies' && (
            <div style={{ display: 'flex', flexDirection: 'column', gap: '16px' }}>
              {book.copies.map((c) => (
                <div
                  key={c.id}
                  style={{
                    background: 'var(--bg-input)',
                    borderRadius: 'var(--radius-md)',
                    border: '1px solid var(--border-card)',
                    padding: '16px',
                    display: 'flex',
                    flexDirection: 'column',
                    gap: '12px',
                  }}
                >
                  {/* Copy Top Line */}
                  <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                    <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                      <span className="badge badge-format">
                        {c.codec || c.format} {c.bitrate_kbps ? `${c.bitrate_kbps} kbps` : ''}
                      </span>
                      <span className={`badge ${c.volume_online ? 'badge-online' : 'badge-offline'}`}>
                        {c.volume_name} ({c.volume_online ? 'в сети' : 'отключен'})
                      </span>
                    </div>

                    <span style={{ fontSize: '12px', fontWeight: 700, color: 'var(--accent-primary)' }}>
                      {c.quality_score}/100
                    </span>
                  </div>

                  {/* Relative / Full Path */}
                  <p
                    title={c.full_path || c.rel_path}
                    style={{
                      fontSize: '12px',
                      color: 'var(--text-muted)',
                      wordBreak: 'break-all',
                      background: 'rgba(0,0,0,0.2)',
                      padding: '8px 10px',
                      borderRadius: '6px',
                    }}
                  >
                    {c.full_path || c.rel_path}
                  </p>

                  {/* Copy Specs */}
                  <div
                    style={{
                      display: 'grid',
                      gridTemplateColumns: 'repeat(3, 1fr)',
                      gap: '8px',
                      fontSize: '11.5px',
                      color: 'var(--text-secondary)',
                    }}
                  >
                    <div>
                      Размер: <strong style={{ color: '#fff' }}>{formatBytes(c.total_size)}</strong>
                    </div>
                    <div>
                      Файлов: <strong style={{ color: '#fff' }}>{c.audio_file_count}</strong>
                    </div>
                    <div>
                      Главы: <strong style={{ color: '#fff' }}>{c.is_single_file ? 'В M4B' : 'Файлы'}</strong>
                    </div>
                  </div>

                  {/* Actions */}
                  <div style={{ display: 'flex', gap: '8px', marginTop: '4px' }}>
                    <button
                      onClick={() => onRevealFolder(c.full_path || c.rel_path)}
                      className="btn btn-secondary"
                      style={{ flex: 1, padding: '6px 12px', fontSize: '12px' }}
                      title="Открыть папку с аудиокнигой в Проводнике Windows"
                    >
                      <FolderOpen size={14} />
                      <span>Папка</span>
                    </button>

                    <button
                      onClick={() => onOptimizeCopy(c.id)}
                      className="btn btn-primary"
                      style={{ flex: 1, padding: '6px 12px', fontSize: '12px' }}
                    >
                      <Zap size={14} />
                      <span>В M4B</span>
                    </button>
                  </div>
                </div>
              ))}
            </div>
          )}

          {activeTab === 'tracks' && (
            <div style={{ display: 'flex', flexDirection: 'column', gap: '6px' }}>
              {book.media.map((m) => (
                <div
                  key={m.id}
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'space-between',
                    padding: '10px 12px',
                    borderRadius: '8px',
                    background: 'var(--bg-input)',
                    border: '1px solid var(--border-subtle)',
                    fontSize: '12px',
                  }}
                >
                  <div style={{ display: 'flex', alignItems: 'center', gap: '10px', flex: 1, overflow: 'hidden' }}>
                    <button
                      onClick={() => onPlayTrack && onPlayTrack(m.file_name, m.full_path || m.rel_path)}
                      className="btn btn-icon"
                      title="Воспроизвести аудиофайл в проигрывателе по умолчанию"
                      style={{ width: '28px', height: '28px' }}
                    >
                      <Play size={12} />
                    </button>
                    <div style={{ overflow: 'hidden' }}>
                      <p
                        style={{
                          fontWeight: 600,
                          color: 'var(--text-primary)',
                          overflow: 'hidden',
                          textOverflow: 'ellipsis',
                          whiteSpace: 'nowrap',
                        }}
                      >
                        {m.title || m.file_name}
                      </p>
                      <span style={{ fontSize: '10.5px', color: 'var(--text-muted)' }}>
                        {m.file_name}
                      </span>
                    </div>
                  </div>

                  <div style={{ display: 'flex', alignItems: 'center', gap: '10px', color: 'var(--text-muted)' }}>
                    <span>{formatBytes(m.size)}</span>
                    {m.duration_ms && <span>{formatDuration(m.duration_ms)}</span>}
                    <button
                      onClick={() => onRevealFolder(m.full_path || m.rel_path)}
                      className="btn btn-icon"
                      title="Показать файл в Проводнике"
                      style={{ width: '26px', height: '26px' }}
                    >
                      <FolderOpen size={13} />
                    </button>
                  </div>
                </div>
              ))}
            </div>
          )}

          {activeTab === 'desc' && book.description && (
            <div
              style={{
                fontSize: '13.5px',
                lineHeight: 1.6,
                color: 'var(--text-secondary)',
                whiteSpace: 'pre-wrap',
                background: 'var(--bg-input)',
                padding: '16px',
                borderRadius: 'var(--radius-md)',
                border: '1px solid var(--border-subtle)',
              }}
            >
              {book.description}
            </div>
          )}
        </div>
      </div>

      <style>{`
        @keyframes slideIn {
          from { transform: translateX(100%); }
          to { transform: translateX(0); }
        }
      `}</style>
    </>
  );
};
