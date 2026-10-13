import React, { useState, useEffect } from 'react';
import {
  Download,
  Pause,
  Play,
  Trash2,
  RefreshCw,
  ArrowDown,
  ArrowUp,
  Clock,
  Users,
} from 'lucide-react';
import { api, formatBytes } from '../api/client';
import type { Download as DownloadItem } from '../api/types';

interface DownloadsPageProps {
  onNavigateToRuTracker: () => void;
}

export const DownloadsPage: React.FC<DownloadsPageProps> = ({
  onNavigateToRuTracker,
}) => {
  const [downloads, setDownloads] = useState<DownloadItem[]>([]);
  const [loading, setLoading] = useState(true);

  const loadDownloads = async () => {
    try {
      const data = await api.getDownloads();
      setDownloads(data);
    } catch (e) {
      console.error('Failed to load downloads:', e);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadDownloads();
    // Live polling every 2 seconds for smooth torrent status updates
    const interval = setInterval(loadDownloads, 2000);
    return () => clearInterval(interval);
  }, []);

  const handlePause = async (id: number) => {
    try {
      await api.pauseDownload(id);
      loadDownloads();
    } catch (e) {
      console.error(e);
    }
  };

  const handleResume = async (id: number) => {
    try {
      await api.resumeDownload(id);
      loadDownloads();
    } catch (e) {
      console.error(e);
    }
  };

  const handleDelete = async (id: number) => {
    if (!window.confirm('Удалить эту раздачу из списка загрузок?')) return;
    try {
      await api.deleteDownload(id);
      loadDownloads();
    } catch (e) {
      console.error(e);
    }
  };

  const getStateBadge = (state: string) => {
    switch (state.toLowerCase()) {
      case 'downloading':
        return {
          label: 'Загрузка',
          color: 'var(--accent-primary)',
          bg: 'rgba(99, 102, 241, 0.15)',
          border: 'rgba(99, 102, 241, 0.3)',
        };
      case 'paused':
        return {
          label: 'Пауза',
          color: '#f59e0b',
          bg: 'rgba(245, 158, 11, 0.15)',
          border: 'rgba(245, 158, 11, 0.3)',
        };
      case 'completed':
      case 'seeding':
        return {
          label: 'Завершено',
          color: 'var(--success)',
          bg: 'rgba(16, 185, 129, 0.15)',
          border: 'rgba(16, 185, 129, 0.3)',
        };
      case 'checking':
        return {
          label: 'Проверка...',
          color: '#a855f7',
          bg: 'rgba(168, 85, 247, 0.15)',
          border: 'rgba(168, 85, 247, 0.3)',
        };
      default:
        return {
          label: state,
          color: 'var(--text-secondary)',
          bg: 'rgba(255, 255, 255, 0.08)',
          border: 'var(--border-subtle)',
        };
    }
  };

  return (
    <div style={{ padding: '32px', maxWidth: '1400px', margin: '0 auto' }}>
      {/* Top Header */}
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
          <h1
            style={{
              fontSize: '26px',
              fontWeight: 800,
              letterSpacing: '-0.02em',
              marginBottom: '6px',
            }}
          >
            Менеджер Загрузок BitTorrent
          </h1>
          <p style={{ color: 'var(--text-secondary)', fontSize: '14px' }}>
            Встроенный торрент-клиент с поддержкой DHT, magnet-ссылок и автоматической интеграцией в фонотеку.
          </p>
        </div>

        <div style={{ display: 'flex', alignItems: 'center', gap: '12px' }}>
          <button
            onClick={onNavigateToRuTracker}
            className="btn btn-primary"
            style={{ padding: '9px 18px' }}
          >
            <Download size={15} />
            <span>Найти раздачу</span>
          </button>
        </div>
      </div>

      {/* Loading state */}
      {loading && downloads.length === 0 && (
        <div style={{ textAlign: 'center', padding: '60px 20px' }}>
          <RefreshCw
            size={36}
            color="var(--accent-primary)"
            className="spin"
            style={{ margin: '0 auto 16px' }}
          />
          <p style={{ color: 'var(--text-secondary)', fontSize: '15px' }}>
            Подключение к торрент-сервису...
          </p>
        </div>
      )}

      {/* Empty State */}
      {!loading && downloads.length === 0 && (
        <div
          className="glass-card"
          style={{
            padding: '60px 30px',
            textAlign: 'center',
            maxWidth: '540px',
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
            <Download size={32} color="var(--accent-primary)" />
          </div>
          <h2 style={{ fontSize: '20px', fontWeight: 700, marginBottom: '8px' }}>
            Список загрузок пуст
          </h2>
          <p
            style={{
              color: 'var(--text-secondary)',
              fontSize: '14px',
              lineHeight: 1.6,
              marginBottom: '24px',
            }}
          >
            Вы можете найти и поставить аудиокниги на скачивание напрямую через вкладку RuTracker.
          </p>
          <button
            onClick={onNavigateToRuTracker}
            className="btn btn-primary"
            style={{ margin: '0 auto' }}
          >
            <span>Перейти к RuTracker</span>
          </button>
        </div>
      )}

      {/* Downloads List */}
      <div style={{ display: 'flex', flexDirection: 'column', gap: '14px' }}>
        {downloads.map((item) => {
          const badge = getStateBadge(item.state);
          const isFinished = item.progress >= 100 || item.state.toLowerCase() === 'completed';

          return (
            <div
              key={item.id}
              className="glass-card"
              style={{
                padding: '20px 24px',
                display: 'flex',
                flexDirection: 'column',
                gap: '16px',
              }}
            >
              {/* Header row: Name, State, Actions */}
              <div
                style={{
                  display: 'flex',
                  alignItems: 'flex-start',
                  justifyContent: 'space-between',
                  gap: '16px',
                }}
              >
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div
                    style={{
                      display: 'flex',
                      alignItems: 'center',
                      gap: '10px',
                      marginBottom: '6px',
                    }}
                  >
                    <span
                      style={{
                        fontSize: '11px',
                        fontWeight: 700,
                        padding: '3px 8px',
                        borderRadius: '6px',
                        background: badge.bg,
                        color: badge.color,
                        border: `1px solid ${badge.border}`,
                      }}
                    >
                      {badge.label}
                    </span>

                    <h3
                      style={{
                        fontSize: '15px',
                        fontWeight: 600,
                        color: 'var(--text-primary)',
                        overflow: 'hidden',
                        textOverflow: 'ellipsis',
                        whiteSpace: 'nowrap',
                      }}
                    >
                      {item.name}
                    </h3>
                  </div>

                  {/* Secondary info metrics */}
                  <div
                    style={{
                      display: 'flex',
                      alignItems: 'center',
                      gap: '16px',
                      fontSize: '12.5px',
                      color: 'var(--text-muted)',
                      flexWrap: 'wrap',
                    }}
                  >
                    <span style={{ fontFamily: 'monospace' }}>
                      {formatBytes(item.downloaded_bytes)} / {formatBytes(item.total_bytes)} ({item.progress.toFixed(1)}%)
                    </span>

                    {item.down_speed && item.state.toLowerCase() === 'downloading' && (
                      <span
                        style={{
                          display: 'flex',
                          alignItems: 'center',
                          gap: '4px',
                          color: '#60a5fa',
                          fontWeight: 600,
                        }}
                      >
                        <ArrowDown size={13} />
                        {item.down_speed}
                      </span>
                    )}

                    {item.up_speed && (
                      <span
                        style={{
                          display: 'flex',
                          alignItems: 'center',
                          gap: '4px',
                          color: 'var(--success)',
                        }}
                      >
                        <ArrowUp size={13} />
                        {item.up_speed}
                      </span>
                    )}

                    {item.eta_str && (
                      <span
                        style={{
                          display: 'flex',
                          alignItems: 'center',
                          gap: '4px',
                        }}
                      >
                        <Clock size={13} />
                        Осталось: {item.eta_str}
                      </span>
                    )}

                    <span
                      style={{
                        display: 'flex',
                        alignItems: 'center',
                        gap: '4px',
                      }}
                    >
                      <Users size={13} />
                      Сиды: {item.seeds} | Пиры: {item.peers}
                    </span>
                  </div>
                </div>

                {/* Right Action buttons */}
                <div style={{ display: 'flex', alignItems: 'center', gap: '8px', flexShrink: 0 }}>
                  {!isFinished && (
                    item.state.toLowerCase() === 'paused' ? (
                      <button
                        onClick={() => handleResume(item.id)}
                        className="btn btn-secondary"
                        style={{ padding: '8px 12px' }}
                        title="Возобновить скачивание"
                      >
                        <Play size={15} color="var(--success)" />
                      </button>
                    ) : (
                      <button
                        onClick={() => handlePause(item.id)}
                        className="btn btn-secondary"
                        style={{ padding: '8px 12px' }}
                        title="Поставить на паузу"
                      >
                        <Pause size={15} color="#f59e0b" />
                      </button>
                    )
                  )}

                  <button
                    onClick={() => handleDelete(item.id)}
                    className="btn"
                    style={{
                      padding: '8px 12px',
                      background: 'rgba(239, 68, 68, 0.1)',
                      color: 'var(--danger)',
                      border: '1px solid rgba(239, 68, 68, 0.25)',
                    }}
                    title="Удалить раздачу"
                  >
                    <Trash2 size={15} />
                  </button>
                </div>
              </div>

              {/* Progress Bar */}
              <div
                style={{
                  height: '6px',
                  borderRadius: '9999px',
                  background: 'rgba(255, 255, 255, 0.08)',
                  overflow: 'hidden',
                }}
              >
                <div
                  style={{
                    height: '100%',
                    width: `${Math.min(100, Math.max(0, item.progress))}%`,
                    background: isFinished
                      ? 'var(--success)'
                      : 'var(--accent-gradient)',
                    borderRadius: '9999px',
                    transition: 'width 0.4s ease',
                  }}
                />
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
};
