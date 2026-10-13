import React, { useState } from 'react';
import {
  Search,
  Download,
  ArrowUp,
  ArrowDown,
  Headphones,
  CheckCircle,
  Loader2,
  Globe,
  Sparkles,
} from 'lucide-react';
import { api, formatBytes } from '../api/client';
import type { TorrentResult } from '../api/types';

interface RuTrackerPageProps {
  onNavigateToDownloads: () => void;
}

export const RuTrackerPage: React.FC<RuTrackerPageProps> = ({
  onNavigateToDownloads,
}) => {
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<TorrentResult[]>([]);
  const [loading, setLoading] = useState(false);
  const [downloadingIds, setDownloadingIds] = useState<Set<string>>(new Set());
  const [downloadedIds, setDownloadedIds] = useState<Set<string>>(new Set());

  const quickTags = [
    'Борис Акунин',
    'Стругацкие',
    'Стивен Кинг',
    'Сергей Лукьяненко',
    'Ю Несбё',
    'Фантастика',
    'Детектив',
  ];

  const handleSearch = async (searchQuery: string = query) => {
    if (!searchQuery.trim()) return;
    setLoading(true);
    try {
      const data = await api.searchTorrents(searchQuery);
      setResults(data);
    } catch (e) {
      console.error('RuTracker search failed:', e);
    } finally {
      setLoading(false);
    }
  };

  const handleDownload = async (item: TorrentResult) => {
    setDownloadingIds((prev) => new Set(prev).add(item.id));
    try {
      const success = await api.downloadTorrent(item.id);
      if (success) {
        setDownloadedIds((prev) => new Set(prev).add(item.id));
      }
    } catch (e) {
      console.error('Failed to trigger download:', e);
    } finally {
      setDownloadingIds((prev) => {
        const next = new Set(prev);
        next.delete(item.id);
        return next;
      });
    }
  };

  return (
    <div style={{ padding: '32px', maxWidth: '1400px', margin: '0 auto' }}>
      {/* Top Banner */}
      <div
        style={{
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'flex-start',
          marginBottom: '28px',
          gap: '20px',
          flexWrap: 'wrap',
        }}
      >
        <div>
          <div style={{ display: 'flex', alignItems: 'center', gap: '10px', marginBottom: '6px' }}>
            <div
              style={{
                width: '32px',
                height: '32px',
                borderRadius: '8px',
                background: 'rgba(59, 130, 246, 0.15)',
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
              }}
            >
              <Globe size={18} color="#60a5fa" />
            </div>
            <h1
              style={{
                fontSize: '26px',
                fontWeight: 800,
                letterSpacing: '-0.02em',
              }}
            >
              Поиск на RuTracker
            </h1>
          </div>
          <p style={{ color: 'var(--text-secondary)', fontSize: '14px' }}>
            Прямой поиск аудиокниг, проверка раздающих, просмотр битрейта и загрузка в один клик.
          </p>
        </div>

        <button
          onClick={onNavigateToDownloads}
          className="btn btn-secondary"
          style={{ padding: '9px 18px' }}
        >
          <Download size={16} />
          <span>Перейти к загрузкам</span>
        </button>
      </div>

      {/* Search Input Bar */}
      <div
        className="glass-card"
        style={{
          padding: '20px 24px',
          marginBottom: '24px',
          display: 'flex',
          flexDirection: 'column',
          gap: '14px',
        }}
      >
        <div style={{ display: 'flex', gap: '12px' }}>
          <div style={{ position: 'relative', flex: 1 }}>
            <Search
              size={18}
              color="var(--text-muted)"
              style={{ position: 'absolute', left: '16px', top: '13px' }}
            />
            <input
              type="text"
              placeholder="Введите название книги, автора или чтеца..."
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={(e) => e.key === 'Enter' && handleSearch()}
              className="input-text"
              style={{
                paddingLeft: '44px',
                height: '46px',
                fontSize: '15px',
                background: 'rgba(0, 0, 0, 0.3)',
              }}
            />
          </div>

          <button
            onClick={() => handleSearch()}
            disabled={loading || !query.trim()}
            className="btn btn-primary"
            style={{ padding: '0 28px', height: '46px', fontSize: '15px' }}
          >
            {loading ? (
              <Loader2 size={18} className="spin" />
            ) : (
              <Search size={18} />
            )}
            <span>Искать</span>
          </button>
        </div>

        {/* Quick Suggestion Chips */}
        <div style={{ display: 'flex', alignItems: 'center', gap: '8px', flexWrap: 'wrap' }}>
          <span style={{ fontSize: '12px', color: 'var(--text-muted)' }}>
            Быстрый поиск:
          </span>
          {quickTags.map((tag) => (
            <button
              key={tag}
              onClick={() => {
                setQuery(tag);
                handleSearch(tag);
              }}
              style={{
                padding: '4px 10px',
                borderRadius: '6px',
                fontSize: '12px',
                background: 'rgba(255, 255, 255, 0.04)',
                color: 'var(--text-secondary)',
                border: '1px solid var(--border-subtle)',
                cursor: 'pointer',
                transition: 'all 0.15s ease',
              }}
              onMouseEnter={(e) => {
                e.currentTarget.style.background = 'rgba(255, 255, 255, 0.1)';
                e.currentTarget.style.color = '#fff';
              }}
              onMouseLeave={(e) => {
                e.currentTarget.style.background = 'rgba(255, 255, 255, 0.04)';
                e.currentTarget.style.color = 'var(--text-secondary)';
              }}
            >
              {tag}
            </button>
          ))}
        </div>
      </div>

      {/* Loading state */}
      {loading && (
        <div style={{ textAlign: 'center', padding: '60px 20px' }}>
          <Loader2
            size={36}
            color="var(--accent-primary)"
            className="spin"
            style={{ margin: '0 auto 16px' }}
          />
          <p style={{ color: 'var(--text-secondary)', fontSize: '15px' }}>
            Поиск раздач на RuTracker...
          </p>
        </div>
      )}

      {/* Empty State before search */}
      {!loading && results.length === 0 && (
        <div
          className="glass-card"
          style={{
            padding: '60px 30px',
            textAlign: 'center',
            maxWidth: '560px',
            margin: '40px auto',
          }}
        >
          <div
            style={{
              width: '64px',
              height: '64px',
              borderRadius: '20px',
              background: 'rgba(99, 102, 241, 0.12)',
              border: '1px solid rgba(99, 102, 241, 0.25)',
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
              margin: '0 auto 20px',
            }}
          >
            <Sparkles size={30} color="var(--accent-primary)" />
          </div>
          <h2 style={{ fontSize: '19px', fontWeight: 700, marginBottom: '8px' }}>
            Поиск раздач аудиокниг
          </h2>
          <p
            style={{
              color: 'var(--text-secondary)',
              fontSize: '14px',
              lineHeight: 1.6,
            }}
          >
            Введите поисковый запрос выше, чтобы найти раздачи в каталоге RuTracker.
            Встроенный клиент автоматически отслеживает сидеров и битрейт.
          </p>
        </div>
      )}

      {/* Results List */}
      {!loading && results.length > 0 && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: '10px' }}>
          <div
            style={{
              display: 'flex',
              justifyContent: 'space-between',
              alignItems: 'center',
              padding: '0 8px',
              marginBottom: '6px',
            }}
          >
            <span style={{ fontSize: '13px', color: 'var(--text-muted)' }}>
              Найдено раздач: <strong>{results.length}</strong>
            </span>
          </div>

          {results.map((item) => {
            const isDownloading = downloadingIds.has(item.id);
            const isDownloaded = downloadedIds.has(item.id);

            return (
              <div
                key={item.id}
                className="glass-card"
                style={{
                  padding: '16px 20px',
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'space-between',
                  gap: '20px',
                  transition: 'border-color 0.15s ease',
                }}
              >
                {/* Left Info */}
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div
                    style={{
                      display: 'flex',
                      alignItems: 'center',
                      gap: '8px',
                      marginBottom: '6px',
                      flexWrap: 'wrap',
                    }}
                  >
                    {item.category && (
                      <span
                        style={{
                          fontSize: '11px',
                          padding: '2px 8px',
                          borderRadius: '4px',
                          background: 'rgba(255, 255, 255, 0.08)',
                          color: 'var(--text-secondary)',
                        }}
                      >
                        {item.category}
                      </span>
                    )}

                    {item.bitrate && (
                      <span
                        style={{
                          fontSize: '11px',
                          padding: '2px 8px',
                          borderRadius: '4px',
                          background: 'rgba(99, 102, 241, 0.15)',
                          color: '#a5b4fc',
                          border: '1px solid rgba(99, 102, 241, 0.3)',
                          fontWeight: 600,
                        }}
                      >
                        {item.bitrate}
                      </span>
                    )}

                    <span
                      style={{
                        fontSize: '12px',
                        color: 'var(--text-muted)',
                        fontFamily: 'monospace',
                      }}
                    >
                      {formatBytes(item.size_bytes)}
                    </span>
                  </div>

                  <h3
                    style={{
                      fontSize: '15px',
                      fontWeight: 600,
                      color: 'var(--text-primary)',
                      marginBottom: '6px',
                      lineHeight: 1.4,
                    }}
                  >
                    {item.title}
                  </h3>

                  <div
                    style={{
                      display: 'flex',
                      alignItems: 'center',
                      gap: '16px',
                      fontSize: '13px',
                      color: 'var(--text-secondary)',
                      flexWrap: 'wrap',
                    }}
                  >
                    {item.author && (
                      <span>
                        Автор: <strong style={{ color: '#fff' }}>{item.author}</strong>
                      </span>
                    )}
                    {item.reader && (
                      <span style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
                        <Headphones size={13} color="var(--text-muted)" />
                        Чтец: <strong style={{ color: '#fff' }}>{item.reader}</strong>
                      </span>
                    )}
                  </div>
                </div>

                {/* Center: Seeds & Leeches */}
                <div
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    gap: '14px',
                    padding: '8px 16px',
                    borderRadius: '8px',
                    background: 'rgba(0, 0, 0, 0.25)',
                    border: '1px solid var(--border-subtle)',
                    flexShrink: 0,
                  }}
                >
                  <div
                    style={{
                      display: 'flex',
                      alignItems: 'center',
                      gap: '4px',
                      color: 'var(--success)',
                      fontSize: '13px',
                      fontWeight: 700,
                    }}
                    title="Сиды (раздающие)"
                  >
                    <ArrowUp size={14} />
                    <span>{item.seeds}</span>
                  </div>

                  <div
                    style={{
                      display: 'flex',
                      alignItems: 'center',
                      gap: '4px',
                      color: item.leeches > 0 ? '#f59e0b' : 'var(--text-muted)',
                      fontSize: '13px',
                      fontWeight: 600,
                    }}
                    title="Личи (качающие)"
                  >
                    <ArrowDown size={14} />
                    <span>{item.leeches}</span>
                  </div>
                </div>

                {/* Right: Download Action */}
                <div style={{ flexShrink: 0 }}>
                  {isDownloaded ? (
                    <button
                      className="btn"
                      disabled
                      style={{
                        padding: '9px 16px',
                        fontSize: '13px',
                        background: 'rgba(16, 185, 129, 0.15)',
                        color: 'var(--success)',
                        border: '1px solid rgba(16, 185, 129, 0.3)',
                      }}
                    >
                      <CheckCircle size={15} />
                      <span>В загрузках</span>
                    </button>
                  ) : (
                    <button
                      onClick={() => handleDownload(item)}
                      disabled={isDownloading}
                      className="btn btn-primary"
                      style={{ padding: '9px 18px', fontSize: '13px' }}
                    >
                      {isDownloading ? (
                        <Loader2 size={15} className="spin" />
                      ) : (
                        <Download size={15} />
                      )}
                      <span>Скачать</span>
                    </button>
                  )}
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
};
