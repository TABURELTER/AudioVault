import React, { useState } from 'react';
import { BookCard } from '../components/BookCard';
import type { Book } from '../api/types';
import {
  Filter,
  ArrowUpDown,
  BookOpen,
  LayoutGrid,
  List,
  HardDrive,
  Mic,
} from 'lucide-react';
import { api, formatBytes, formatDuration } from '../api/client';

interface LibraryPageProps {
  books: Book[];
  onSelectBook: (bookId: number) => void;
  activeFilter: string;
  onFilterChange: (filter: string) => void;
  activeSort: string;
  onSortChange: (sort: string) => void;
}

const BookRowThumb: React.FC<{ title: string; thumb?: string }> = ({ title, thumb }) => {
  const [coverUrl, setCoverUrl] = React.useState<string | null>(null);

  React.useEffect(() => {
    let active = true;
    if (thumb) {
      api.getCover(thumb).then((url) => {
        if (active && url) setCoverUrl(url);
      });
    }
    return () => {
      active = false;
    };
  }, [thumb]);

  if (coverUrl) {
    return (
      <img
        src={coverUrl}
        alt={title}
        style={{
          width: '38px',
          height: '38px',
          borderRadius: '6px',
          objectFit: 'cover',
          background: 'rgba(255,255,255,0.05)',
          flexShrink: 0,
        }}
      />
    );
  }

  const hash = title.split('').reduce((acc, char) => acc + char.charCodeAt(0), 0);
  const hue = hash % 360;
  return (
    <div
      style={{
        width: '38px',
        height: '38px',
        borderRadius: '6px',
        background: `hsl(${hue}, 60%, 22%)`,
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        fontSize: '14px',
        fontWeight: 700,
        color: '#fff',
        flexShrink: 0,
      }}
    >
      {title.charAt(0)}
    </div>
  );
};

export const LibraryPage: React.FC<LibraryPageProps> = ({
  books,
  onSelectBook,
  activeFilter,
  onFilterChange,
  activeSort,
  onSortChange,
}) => {
  const [viewMode, setViewMode] = useState<'grid' | 'table'>('table');

  const filterOptions = [
    { id: 'all', label: 'Все аудиокниги' },
    { id: 'online', label: 'В сети' },
    { id: 'offline', label: 'Оффлайн тома' },
    { id: 'multiple', label: 'Несколько версий' },
    { id: 'duplicates', label: 'Дубликаты' },
  ];

  const sortOptions = [
    { id: 'author', label: 'По автору' },
    { id: 'title', label: 'По названию' },
    { id: 'duration', label: 'По длительности' },
    { id: 'size', label: 'По размеру' },
  ];

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: '20px' }}>
      {/* Control Bar: Filters, Sort & View Mode Toggle */}
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          flexWrap: 'wrap',
          gap: '12px',
          background: 'var(--bg-surface)',
          padding: '12px 16px',
          borderRadius: 'var(--radius-lg)',
          border: '1px solid var(--border-subtle)',
        }}
      >
        {/* Filter Pills */}
        <div style={{ display: 'flex', alignItems: 'center', gap: '8px', flexWrap: 'wrap' }}>
          <Filter size={15} color="var(--text-muted)" />
          {filterOptions.map((f) => (
            <button
              key={f.id}
              onClick={() => onFilterChange(f.id)}
              style={{
                padding: '6px 14px',
                borderRadius: '9999px',
                fontSize: '12px',
                fontWeight: activeFilter === f.id ? 700 : 500,
                background:
                  activeFilter === f.id
                    ? 'var(--accent-primary)'
                    : 'rgba(255, 255, 255, 0.05)',
                color: activeFilter === f.id ? '#ffffff' : 'var(--text-secondary)',
                border:
                  activeFilter === f.id
                    ? '1px solid var(--accent-primary)'
                    : '1px solid var(--border-subtle)',
                cursor: 'pointer',
                transition: 'all 0.15s ease',
              }}
            >
              {f.label}
            </button>
          ))}
        </div>

        {/* View Mode, Sort and Count */}
        <div style={{ display: 'flex', alignItems: 'center', gap: '16px' }}>
          <span style={{ fontSize: '12px', color: 'var(--text-muted)' }}>
            Найдено: <strong style={{ color: '#fff' }}>{books.length}</strong>
          </span>

          {/* Sort Dropdown */}
          <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
            <ArrowUpDown size={15} color="var(--text-muted)" />
            <select
              value={activeSort}
              onChange={(e) => onSortChange(e.target.value)}
              className="input-text"
              style={{
                padding: '6px 12px',
                fontSize: '12px',
                width: '150px',
                cursor: 'pointer',
              }}
            >
              {sortOptions.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.label}
                </option>
              ))}
            </select>
          </div>

          {/* View Mode Toggle: Table / Grid */}
          <div
            style={{
              display: 'flex',
              background: 'rgba(255, 255, 255, 0.05)',
              borderRadius: '8px',
              padding: '3px',
              border: '1px solid var(--border-subtle)',
            }}
          >
            <button
              onClick={() => setViewMode('table')}
              title="Табличный вид (высокая плотность)"
              style={{
                padding: '5px 9px',
                borderRadius: '6px',
                border: 'none',
                background: viewMode === 'table' ? 'var(--accent-primary)' : 'transparent',
                color: viewMode === 'table' ? '#fff' : 'var(--text-muted)',
                cursor: 'pointer',
                display: 'flex',
                alignItems: 'center',
                transition: 'all 0.15s ease',
              }}
            >
              <List size={15} />
            </button>
            <button
              onClick={() => setViewMode('grid')}
              title="Вид сеткой (карточки)"
              style={{
                padding: '5px 9px',
                borderRadius: '6px',
                border: 'none',
                background: viewMode === 'grid' ? 'var(--accent-primary)' : 'transparent',
                color: viewMode === 'grid' ? '#fff' : 'var(--text-muted)',
                cursor: 'pointer',
                display: 'flex',
                alignItems: 'center',
                transition: 'all 0.15s ease',
              }}
            >
              <LayoutGrid size={15} />
            </button>
          </div>
        </div>
      </div>

      {/* Books Content */}
      {books.length === 0 ? (
        <div
          style={{
            display: 'flex',
            flexDirection: 'column',
            alignItems: 'center',
            justifyContent: 'center',
            padding: '80px 20px',
            textAlign: 'center',
            color: 'var(--text-muted)',
          }}
        >
          <BookOpen size={48} strokeWidth={1.5} style={{ marginBottom: '16px', opacity: 0.5 }} />
          <h3 style={{ fontSize: '18px', color: 'var(--text-primary)', marginBottom: '6px' }}>
            Аудиокниги не найдены
          </h3>
          <p style={{ fontSize: '13px', maxWidth: '400px' }}>
            Проверьте фильтры или добавьте папку во вкладке «Хранилища», чтобы проиндексировать аудиокниги.
          </p>
        </div>
      ) : viewMode === 'table' ? (
        /* High-Density Desktop Table View */
        <div
          className="glass-card"
          style={{
            overflowX: 'auto',
            borderRadius: 'var(--radius-lg)',
            border: '1px solid var(--border-subtle)',
          }}
        >
          <table
            style={{
              width: '100%',
              borderCollapse: 'collapse',
              textAlign: 'left',
              fontSize: '12.5px',
            }}
          >
            <thead>
              <tr
                style={{
                  background: 'rgba(255, 255, 255, 0.03)',
                  borderBottom: '1px solid var(--border-subtle)',
                  color: 'var(--text-muted)',
                  fontSize: '11.5px',
                  textTransform: 'uppercase',
                  letterSpacing: '0.04em',
                }}
              >
                <th style={{ padding: '12px 14px', width: '50px' }}>Обложка</th>
                <th style={{ padding: '12px 14px' }}>Название и цикл</th>
                <th style={{ padding: '12px 14px' }}>Автор</th>
                <th style={{ padding: '12px 14px' }}>Чтец</th>
                <th style={{ padding: '12px 14px', width: '90px' }}>Время</th>
                <th style={{ padding: '12px 14px', width: '80px' }}>Формат</th>
                <th style={{ padding: '12px 14px', width: '90px' }}>Размер</th>
                <th style={{ padding: '12px 14px' }}>Хранилище</th>
                <th style={{ padding: '12px 14px', width: '90px' }}>Статус</th>
              </tr>
            </thead>
            <tbody>
              {books.map((book) => (
                <tr
                  key={book.id}
                  onClick={() => onSelectBook(book.id)}
                  style={{
                    borderBottom: '1px solid rgba(255, 255, 255, 0.04)',
                    cursor: 'pointer',
                    transition: 'background 0.12s ease',
                  }}
                  onMouseEnter={(e) => (e.currentTarget.style.background = 'rgba(255, 255, 255, 0.05)')}
                  onMouseLeave={(e) => (e.currentTarget.style.background = 'transparent')}
                >
                  {/* Cover Thumb */}
                  <td style={{ padding: '8px 14px' }}>
                    <BookRowThumb title={book.title} thumb={book.cover_thumb} />
                  </td>

                  {/* Title & Series */}
                  <td style={{ padding: '8px 14px' }}>
                    <div style={{ fontWeight: 600, color: '#fff', fontSize: '13px' }}>
                      {book.title}
                    </div>
                    {book.series && (
                      <div
                        style={{
                          fontSize: '11px',
                          color: 'var(--accent-primary)',
                          marginTop: '2px',
                        }}
                      >
                        {book.series} {book.series_index ? `#${book.series_index}` : ''}
                      </div>
                    )}
                  </td>

                  {/* Author */}
                  <td style={{ padding: '8px 14px', color: 'var(--text-secondary)' }}>
                    {book.authors || '—'}
                  </td>

                  {/* Narrator */}
                  <td style={{ padding: '8px 14px', color: 'var(--text-muted)' }}>
                    {book.narrators ? (
                      <div style={{ display: 'flex', alignItems: 'center', gap: '5px' }}>
                        <Mic size={12} style={{ opacity: 0.6 }} />
                        <span>{book.narrators}</span>
                      </div>
                    ) : (
                      '—'
                    )}
                  </td>

                  {/* Duration */}
                  <td style={{ padding: '8px 14px', color: 'var(--text-secondary)', whiteSpace: 'nowrap' }}>
                    {formatDuration(book.duration_ms)}
                  </td>

                  {/* Format */}
                  <td style={{ padding: '8px 14px' }}>
                    <span className="badge badge-format">
                      {book.format.toUpperCase()}
                    </span>
                  </td>

                  {/* Size */}
                  <td style={{ padding: '8px 14px', color: 'var(--text-secondary)', whiteSpace: 'nowrap' }}>
                    {formatBytes(book.size)}
                  </td>

                  {/* Storage */}
                  <td style={{ padding: '8px 14px', color: 'var(--text-secondary)' }}>
                    <div style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
                      <HardDrive size={13} style={{ opacity: 0.6 }} />
                      <span>{book.volume_name || 'Архив'}</span>
                    </div>
                  </td>

                  {/* Online/Offline Status */}
                  <td style={{ padding: '8px 14px' }}>
                    <span className={`badge ${book.online ? 'badge-online' : 'badge-offline'}`}>
                      <span className={`dot ${book.online ? 'dot-online' : 'dot-offline'}`} />
                      {book.online ? 'В сети' : 'Оффлайн'}
                    </span>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : (
        /* Visual Cards Grid View */
        <div
          style={{
            display: 'grid',
            gridTemplateColumns: 'repeat(auto-fill, minmax(220px, 1fr))',
            gap: '20px',
          }}
        >
          {books.map((book) => (
            <BookCard
              key={book.id}
              book={book}
              onClick={() => onSelectBook(book.id)}
            />
          ))}
        </div>
      )}
    </div>
  );
};
