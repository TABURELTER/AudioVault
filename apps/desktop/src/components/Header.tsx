import React from 'react';
import { Search, RefreshCw, Sparkles, Loader2 } from 'lucide-react';
import type { Task } from '../api/types';

interface HeaderProps {
  title: string;
  subtitle: string;
  searchQuery: string;
  onSearchChange: (q: string) => void;
  onScanAll: () => void;
  onFindDuplicates: () => void;
  activeTasks: Task[];
  isScanning: boolean;
}

export const Header: React.FC<HeaderProps> = ({
  title,
  subtitle,
  searchQuery,
  onSearchChange,
  onScanAll,
  onFindDuplicates,
  activeTasks,
  isScanning,
}) => {
  const currentRunning = activeTasks.find((t) => t.state === 'running');

  return (
    <header
      style={{
        height: '70px',
        padding: '0 32px',
        background: 'var(--bg-header)',
        backdropFilter: 'blur(20px)',
        WebkitBackdropFilter: 'blur(20px)',
        borderBottom: '1px solid var(--border-subtle)',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'space-between',
        position: 'sticky',
        top: 0,
        zIndex: 10,
      }}
    >
      {/* Title & Subtitle */}
      <div>
        <h2 style={{ fontSize: '18px', fontWeight: 700, color: 'var(--text-primary)' }}>
          {title}
        </h2>
        <p style={{ fontSize: '12px', color: 'var(--text-muted)' }}>
          {subtitle}
        </p>
      </div>

      {/* Center Search Input */}
      <div style={{ position: 'relative', width: '360px' }}>
        <Search
          size={16}
          color="var(--text-muted)"
          style={{ position: 'absolute', left: '14px', top: '11px' }}
        />
        <input
          type="text"
          placeholder="Поиск книг, авторов, чтецов... (FTS5)"
          value={searchQuery}
          onChange={(e) => onSearchChange(e.target.value)}
          className="input-text"
          style={{
            paddingLeft: '38px',
            background: 'rgba(0, 0, 0, 0.25)',
            borderColor: 'var(--border-subtle)',
          }}
        />
      </div>

      {/* Right Actions & Status */}
      <div style={{ display: 'flex', alignItems: 'center', gap: '12px' }}>
        {/* Active Task Indicator */}
        {(currentRunning || isScanning) && (
          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: '8px',
              padding: '6px 12px',
              borderRadius: '9999px',
              background: 'rgba(99, 102, 241, 0.15)',
              border: '1px solid rgba(99, 102, 241, 0.3)',
              fontSize: '12px',
              color: '#a5b4fc',
            }}
          >
            <Loader2 size={14} className="spin" style={{ animation: 'spin 1s linear infinite' }} />
            <span>
              {currentRunning ? currentRunning.label : 'Сканирование...'}
            </span>
          </div>
        )}

        {/* Global Scan Button */}
        <button
          onClick={onScanAll}
          className="btn btn-secondary"
          title="Пересканировать подключенные тома"
        >
          <RefreshCw size={14} className={isScanning ? 'spin' : ''} />
          <span>Сканировать</span>
        </button>

        {/* Global Duplicates Button */}
        <button
          onClick={onFindDuplicates}
          className="btn btn-primary"
          title="Запустить поиск дубликатов"
        >
          <Sparkles size={14} />
          <span>Дубликаты</span>
        </button>
      </div>

      <style>{`
        @keyframes spin {
          from { transform: rotate(0deg); }
          to { transform: rotate(360deg); }
        }
        .spin {
          animation: spin 1s linear infinite;
        }
      `}</style>
    </header>
  );
};
