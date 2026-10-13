import React from 'react';
import {
  BookOpen,
  Search,
  HardDrive,
  Copy,
  AlertCircle,
  Globe,
  Download,
  Zap,
  ListTodo,
  Settings,
  Headphones,
} from 'lucide-react';
import type { Stats, Volume } from '../api/types';

interface SidebarProps {
  currentPage: string;
  onNavigate: (page: string) => void;
  stats: Stats;
  volumes: Volume[];
}

export const Sidebar: React.FC<SidebarProps> = ({
  currentPage,
  onNavigate,
  stats,
  volumes,
}) => {
  const onlineCount = volumes.filter((v) => v.online).length;

  const navItems = [
    { id: 'library', label: 'Фонотека', icon: BookOpen, count: stats.total_books },
    { id: 'search', label: 'Поиск', icon: Search },
    { id: 'storages', label: 'Хранилища', icon: HardDrive, count: volumes.length },
    {
      id: 'duplicates',
      label: 'Дубликаты',
      icon: Copy,
      badge: stats.duplicates_count > 0 ? stats.duplicates_count : undefined,
      badgeColor: 'var(--warning)',
    },
    {
      id: 'issues',
      label: 'Проблемы',
      icon: AlertCircle,
      badge: stats.issues_count > 0 ? stats.issues_count : undefined,
      badgeColor: 'var(--danger)',
    },
    { id: 'rutracker', label: 'RuTracker', icon: Globe },
    { id: 'downloads', label: 'Загрузки', icon: Download },
    { id: 'optimizer', label: 'Оптимизация', icon: Zap },
    { id: 'tasks', label: 'Задачи', icon: ListTodo },
    { id: 'settings', label: 'Настройки', icon: Settings },
  ];

  return (
    <aside
      style={{
        width: '260px',
        height: '100vh',
        background: 'var(--bg-sidebar)',
        borderRight: '1px solid var(--border-subtle)',
        display: 'flex',
        flexDirection: 'column',
        userSelect: 'none',
        zIndex: 20,
      }}
    >
      {/* Brand Header */}
      <div
        style={{
          padding: '24px 20px',
          display: 'flex',
          alignItems: 'center',
          gap: '12px',
          borderBottom: '1px solid var(--border-subtle)',
        }}
      >
        <div
          style={{
            width: '38px',
            height: '38px',
            borderRadius: '12px',
            background: 'var(--accent-gradient)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            boxShadow: 'var(--shadow-glow)',
          }}
        >
          <Headphones size={22} color="#ffffff" />
        </div>
        <div>
          <h1
            style={{
              fontSize: '17px',
              fontWeight: 800,
              letterSpacing: '-0.02em',
              background: 'linear-gradient(180deg, #fff 0%, #cbd5e1 100%)',
              WebkitBackgroundClip: 'text',
              WebkitTextFillColor: 'transparent',
            }}
          >
            AudioVault
          </h1>
          <p style={{ fontSize: '11px', color: 'var(--text-muted)' }}>
            Умная медиатека
          </p>
        </div>
      </div>

      {/* Navigation List */}
      <nav
        style={{
          flex: 1,
          padding: '16px 12px',
          display: 'flex',
          flexDirection: 'column',
          gap: '4px',
          overflowY: 'auto',
        }}
      >
        {navItems.map((item) => {
          const Icon = item.icon;
          const active = currentPage === item.id;

          return (
            <button
              key={item.id}
              onClick={() => onNavigate(item.id)}
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: '12px',
                padding: '10px 14px',
                borderRadius: 'var(--radius-md)',
                background: active ? 'rgba(99, 102, 241, 0.15)' : 'transparent',
                color: active ? '#ffffff' : 'var(--text-secondary)',
                border: active
                  ? '1px solid rgba(99, 102, 241, 0.35)'
                  : '1px solid transparent',
                cursor: 'pointer',
                textAlign: 'left',
                width: '100%',
                fontSize: '13.5px',
                fontWeight: active ? 600 : 500,
                transition: 'all 0.15s ease',
              }}
              onMouseEnter={(e) => {
                if (!active) {
                  e.currentTarget.style.background = 'rgba(255, 255, 255, 0.05)';
                  e.currentTarget.style.color = '#ffffff';
                }
              }}
              onMouseLeave={(e) => {
                if (!active) {
                  e.currentTarget.style.background = 'transparent';
                  e.currentTarget.style.color = 'var(--text-secondary)';
                }
              }}
            >
              <Icon
                size={18}
                color={active ? 'var(--accent-primary)' : 'currentColor'}
              />
              <span style={{ flex: 1 }}>{item.label}</span>

              {item.badge !== undefined && (
                <span
                  style={{
                    fontSize: '11px',
                    fontWeight: 700,
                    padding: '2px 7px',
                    borderRadius: '9999px',
                    background: item.badgeColor
                      ? `${item.badgeColor}25`
                      : 'rgba(255, 255, 255, 0.1)',
                    color: item.badgeColor || '#fff',
                    border: `1px solid ${item.badgeColor || 'rgba(255, 255, 255, 0.2)'}`,
                  }}
                >
                  {item.badge}
                </span>
              )}

              {item.count !== undefined && item.badge === undefined && (
                <span style={{ fontSize: '11px', color: 'var(--text-muted)' }}>
                  {item.count}
                </span>
              )}
            </button>
          );
        })}
      </nav>

      {/* Storage Footer Status */}
      <div
        style={{
          padding: '16px',
          borderTop: '1px solid var(--border-subtle)',
          background: 'rgba(0, 0, 0, 0.2)',
        }}
      >
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            marginBottom: '6px',
          }}
        >
          <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
            <span
              className={`dot ${onlineCount > 0 ? 'dot-online' : 'dot-offline'}`}
            />
            <span style={{ fontSize: '12px', fontWeight: 600 }}>
              {onlineCount > 0
                ? `${onlineCount} тома в сети`
                : 'Все диски отключены'}
            </span>
          </div>
        </div>
        <div style={{ display: 'flex', gap: '4px', flexWrap: 'wrap' }}>
          {volumes.map((v) => (
            <span
              key={v.id}
              style={{
                fontSize: '10px',
                padding: '2px 6px',
                borderRadius: '4px',
                background: v.online
                  ? 'rgba(16, 185, 129, 0.12)'
                  : 'rgba(239, 68, 68, 0.12)',
                color: v.online ? 'var(--success)' : 'var(--danger)',
                border: `1px solid ${v.online ? 'rgba(16, 185, 129, 0.25)' : 'rgba(239, 68, 68, 0.25)'}`,
              }}
            >
              {v.mount_path.replace('\\', '') || v.label}
            </span>
          ))}
        </div>
      </div>
    </aside>
  );
};
