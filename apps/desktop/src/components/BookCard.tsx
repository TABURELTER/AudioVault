import React from 'react';
import { Clock, Disc, Mic } from 'lucide-react';
import type { Book } from '../api/types';
import { api, formatBytes, formatDuration } from '../api/client';

interface BookCardProps {
  book: Book;
  onClick: () => void;
}

export const BookCard: React.FC<BookCardProps> = ({ book, onClick }) => {
  const [coverUrl, setCoverUrl] = React.useState<string | null>(null);

  React.useEffect(() => {
    let active = true;
    if (book.cover_thumb) {
      api.getCover(book.cover_thumb).then((url) => {
        if (active && url) {
          setCoverUrl(url);
        }
      });
    }
    return () => {
      active = false;
    };
  }, [book.cover_thumb]);

  // Generate a deterministic gradient for covers without image
  const hash = book.title.split('').reduce((acc, char) => acc + char.charCodeAt(0), 0);
  const hue1 = hash % 360;
  const hue2 = (hue1 + 45) % 360;
  const gradient = `linear-gradient(145deg, hsl(${hue1}, 65%, 22%) 0%, hsl(${hue2}, 70%, 12%) 100%)`;

  return (
    <div
      onClick={onClick}
      className="glass-card"
      style={{
        display: 'flex',
        flexDirection: 'column',
        borderRadius: 'var(--radius-lg)',
        overflow: 'hidden',
        cursor: 'pointer',
        height: '320px',
        position: 'relative',
        transition: 'all 0.22s cubic-bezier(0.4, 0, 0.2, 1)',
      }}
    >
      {/* Cover / Header Section */}
      <div
        style={{
          height: '160px',
          background: gradient,
          position: 'relative',
          padding: '16px',
          display: 'flex',
          flexDirection: 'column',
          justifyContent: 'space-between',
        }}
      >
        {/* Real Cover Artwork Image */}
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
        {/* Overlay gradient for contrast with badges */}
        {coverUrl && (
          <div
            style={{
              position: 'absolute',
              inset: 0,
              background: 'linear-gradient(to bottom, rgba(0,0,0,0.65) 0%, rgba(0,0,0,0.2) 40%, rgba(0,0,0,0.8) 100%)',
            }}
          />
        )}
        {/* Top Badges */}
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
          <span className="badge badge-format">
            {book.format.toUpperCase()}
          </span>

          <span className={`badge ${book.online ? 'badge-online' : 'badge-offline'}`}>
            <span className={`dot ${book.online ? 'dot-online' : 'dot-offline'}`} />
            {book.volume_name || (book.online ? 'В сети' : 'Оффлайн')}
          </span>
        </div>

        {/* Large Decorative Initial */}
        <div
          style={{
            position: 'absolute',
            bottom: '10px',
            right: '16px',
            fontSize: '64px',
            fontWeight: 900,
            opacity: 0.12,
            lineHeight: 1,
            pointerEvents: 'none',
            color: '#fff',
          }}
        >
          {book.title.charAt(0)}
        </div>

        {/* Series Badge if available */}
        {book.series && (
          <div
            style={{
              alignSelf: 'flex-start',
              fontSize: '11px',
              fontWeight: 600,
              padding: '2px 8px',
              borderRadius: '6px',
              background: 'rgba(0, 0, 0, 0.4)',
              color: '#e2e8f0',
              backdropFilter: 'blur(8px)',
              maxWidth: '90%',
              overflow: 'hidden',
              textOverflow: 'ellipsis',
              whiteSpace: 'nowrap',
            }}
          >
            {book.series} {book.series_index ? `#${book.series_index}` : ''}
          </div>
        )}
      </div>

      {/* Book Metadata Body */}
      <div
        style={{
          padding: '16px',
          flex: 1,
          display: 'flex',
          flexDirection: 'column',
          justifyContent: 'space-between',
        }}
      >
        <div>
          <h3
            title={book.title}
            style={{
              fontSize: '15px',
              fontWeight: 700,
              color: 'var(--text-primary)',
              lineHeight: 1.3,
              marginBottom: '4px',
              display: '-webkit-box',
              WebkitLineClamp: 2,
              WebkitBoxOrient: 'vertical',
              overflow: 'hidden',
            }}
          >
            {book.title}
          </h3>

          <p
            style={{
              fontSize: '13px',
              color: 'var(--text-secondary)',
              fontWeight: 500,
              overflow: 'hidden',
              textOverflow: 'ellipsis',
              whiteSpace: 'nowrap',
              marginBottom: '8px',
            }}
          >
            {book.authors || 'Неизвестный автор'}
          </p>
        </div>

        {/* Narrator and Tech Info Footer */}
        <div style={{ display: 'flex', flexDirection: 'column', gap: '6px' }}>
          {book.narrators && (
            <div
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: '6px',
                fontSize: '11.5px',
                color: 'var(--text-muted)',
              }}
            >
              <Mic size={13} color="var(--accent-primary)" />
              <span
                style={{
                  overflow: 'hidden',
                  textOverflow: 'ellipsis',
                  whiteSpace: 'nowrap',
                }}
              >
                {book.narrators}
              </span>
            </div>
          )}

          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'space-between',
              fontSize: '11px',
              color: 'var(--text-muted)',
              borderTop: '1px solid var(--border-subtle)',
              paddingTop: '8px',
              marginTop: '4px',
            }}
          >
            <div style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
              <Clock size={12} />
              <span>{formatDuration(book.duration_ms)}</span>
            </div>

            <div style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
              <Disc size={12} />
              <span>{formatBytes(book.size)}</span>
            </div>

            {book.copies_count > 1 && (
              <span
                style={{
                  fontSize: '10px',
                  fontWeight: 700,
                  padding: '1px 5px',
                  borderRadius: '4px',
                  background: 'rgba(99, 102, 241, 0.2)',
                  color: '#a5b4fc',
                }}
              >
                {book.copies_count} копии
              </span>
            )}
          </div>
        </div>
      </div>
    </div>
  );
};
