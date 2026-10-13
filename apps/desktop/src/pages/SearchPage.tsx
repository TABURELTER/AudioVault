import React, { useState } from 'react';
import { Search as SearchIcon, BookOpen } from 'lucide-react';
import { BookCard } from '../components/BookCard';
import type { Book } from '../api/types';

interface SearchPageProps {
  onSelectBook: (id: number) => void;
  onSearch: (q: string) => Promise<Book[]>;
}

export const SearchPage: React.FC<SearchPageProps> = ({ onSelectBook, onSearch }) => {
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<Book[]>([]);
  const [hasSearched, setHasSearched] = useState(false);
  const [isSearching, setIsSearching] = useState(false);

  const handleSearch = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!query.trim()) return;
    setIsSearching(true);
    const res = await onSearch(query.trim());
    setResults(res);
    setHasSearched(true);
    setIsSearching(false);
  };

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: '24px' }}>
      {/* Search Bar Input Card */}
      <form
        onSubmit={handleSearch}
        className="glass-card"
        style={{
          padding: '20px',
          display: 'flex',
          gap: '12px',
          alignItems: 'center',
        }}
      >
        <div style={{ position: 'relative', flex: 1 }}>
          <SearchIcon
            size={18}
            color="var(--text-muted)"
            style={{ position: 'absolute', left: '16px', top: '14px' }}
          />
          <input
            type="text"
            placeholder="Введите название книги, автора или чтеца (поддерживается поиск латиницей и опечатки)..."
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            className="input-text"
            style={{
              padding: '12px 16px 12px 46px',
              fontSize: '15px',
              background: 'var(--bg-input)',
            }}
            autoFocus
          />
        </div>

        <button type="submit" className="btn btn-primary" style={{ padding: '12px 24px', fontSize: '14px' }}>
          <span>{isSearching ? 'Поиск...' : 'Искать'}</span>
        </button>
      </form>

      {/* Results Header */}
      {hasSearched && (
        <div style={{ fontSize: '13px', color: 'var(--text-muted)' }}>
          Найдено результатов: <strong style={{ color: '#fff' }}>{results.length}</strong>
        </div>
      )}

      {/* Results Grid */}
      {results.length > 0 ? (
        <div
          style={{
            display: 'grid',
            gridTemplateColumns: 'repeat(auto-fill, minmax(220px, 1fr))',
            gap: '20px',
          }}
        >
          {results.map((book) => (
            <BookCard
              key={book.id}
              book={book}
              onClick={() => onSelectBook(book.id)}
            />
          ))}
        </div>
      ) : hasSearched ? (
        <div
          style={{
            display: 'flex',
            flexDirection: 'column',
            alignItems: 'center',
            justifyContent: 'center',
            padding: '60px 20px',
            textAlign: 'center',
            color: 'var(--text-muted)',
          }}
        >
          <BookOpen size={48} strokeWidth={1.5} style={{ marginBottom: '16px', opacity: 0.5 }} />
          <h3 style={{ fontSize: '17px', color: '#fff', marginBottom: '4px' }}>
            Ничего не найдено по запросу «{query}»
          </h3>
          <p style={{ fontSize: '13px' }}>
            Попробуйте изменить поисковую фразу или часть фамилии автора.
          </p>
        </div>
      ) : null}
    </div>
  );
};
