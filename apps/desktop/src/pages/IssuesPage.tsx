import React, { useState, useEffect } from 'react';
import {
  AlertTriangle,
  CheckCircle2,
  Eye,
  RefreshCw,
  ArrowRight,
} from 'lucide-react';
import { api } from '../api/client';
import type { Issue } from '../api/types';

interface IssuesPageProps {
  onOpenBook: (bookId: number) => void;
}

export const IssuesPage: React.FC<IssuesPageProps> = ({ onOpenBook }) => {
  const [issues, setIssues] = useState<Issue[]>([]);
  const [loading, setLoading] = useState(true);
  const [filterKind, setFilterKind] = useState<string>('all');
  const [ignoredIds, setIgnoredIds] = useState<Set<number>>(new Set());

  const loadIssues = async () => {
    setLoading(true);
    try {
      const data = await api.getIssues();
      setIssues(data);
    } catch (e) {
      console.error(e);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadIssues();
  }, []);

  const handleIgnore = async (issueId: number) => {
    try {
      await api.ignoreIssue(issueId);
      setIgnoredIds((prev) => new Set(prev).add(issueId));
    } catch (e) {
      console.error('Failed to ignore issue:', e);
    }
  };

  const visibleIssues = issues
    .filter((issue) => !ignoredIds.has(issue.id))
    .filter((issue) => {
      if (filterKind === 'all') return true;
      return issue.kind === filterKind;
    });

  // Extract unique kinds for filter
  const kinds = Array.from(new Set(issues.map((i) => i.kind)));

  return (
    <div style={{ padding: '32px', maxWidth: '1400px', margin: '0 auto' }}>
      {/* Top Header & Metrics */}
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
            Контроль Качества Метаданных
          </h1>
          <p style={{ color: 'var(--text-secondary)', fontSize: '14px' }}>
            Автоматическое обнаружение пропущенных тегов, повреждённых названий и несоответствий в треках.
          </p>
        </div>

        <div style={{ display: 'flex', alignItems: 'center', gap: '12px' }}>
          <button
            onClick={loadIssues}
            disabled={loading}
            className="btn btn-secondary"
          >
            <RefreshCw size={15} className={loading ? 'spin' : ''} />
            <span>Обновить</span>
          </button>
        </div>
      </div>

      {/* Filter Tabs */}
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: '8px',
          marginBottom: '24px',
          overflowX: 'auto',
          paddingBottom: '4px',
        }}
      >
        <button
          onClick={() => setFilterKind('all')}
          style={{
            padding: '8px 16px',
            borderRadius: '9999px',
            fontSize: '13px',
            fontWeight: 600,
            cursor: 'pointer',
            border: 'none',
            background:
              filterKind === 'all'
                ? 'var(--accent-primary)'
                : 'rgba(255, 255, 255, 0.05)',
            color: filterKind === 'all' ? '#fff' : 'var(--text-secondary)',
            transition: 'all 0.15s ease',
          }}
        >
          Все замечания ({visibleIssues.length})
        </button>

        {kinds.map((kind) => {
          const count = issues.filter(
            (i) => i.kind === kind && !ignoredIds.has(i.id)
          ).length;
          const label =
            issues.find((i) => i.kind === kind)?.kind_label || kind;

          return (
            <button
              key={kind}
              onClick={() => setFilterKind(kind)}
              style={{
                padding: '8px 16px',
                borderRadius: '9999px',
                fontSize: '13px',
                fontWeight: 600,
                cursor: 'pointer',
                border: 'none',
                background:
                  filterKind === kind
                    ? 'rgba(99, 102, 241, 0.25)'
                    : 'rgba(255, 255, 255, 0.05)',
                color: filterKind === kind ? '#a5b4fc' : 'var(--text-secondary)',
                outline:
                  filterKind === kind
                    ? '1px solid rgba(99, 102, 241, 0.4)'
                    : 'none',
                transition: 'all 0.15s ease',
              }}
            >
              {label} ({count})
            </button>
          );
        })}
      </div>

      {/* Loading state */}
      {loading && issues.length === 0 && (
        <div style={{ textAlign: 'center', padding: '80px 20px' }}>
          <RefreshCw
            size={36}
            color="var(--accent-primary)"
            className="spin"
            style={{ margin: '0 auto 16px' }}
          />
          <p style={{ color: 'var(--text-secondary)', fontSize: '15px' }}>
            Анализ каталога на наличие проблем с метаданными...
          </p>
        </div>
      )}

      {/* Empty State */}
      {!loading && visibleIssues.length === 0 && (
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
              background: 'rgba(16, 185, 129, 0.12)',
              border: '1px solid rgba(16, 185, 129, 0.25)',
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
              margin: '0 auto 20px',
            }}
          >
            <CheckCircle2 size={32} color="var(--success)" />
          </div>
          <h2 style={{ fontSize: '20px', fontWeight: 700, marginBottom: '8px' }}>
            Замечаний не обнаружено!
          </h2>
          <p
            style={{
              color: 'var(--text-secondary)',
              fontSize: '14px',
              lineHeight: 1.6,
            }}
          >
            Все аудиокниги в фонотеке имеют корректные метаданные, имена авторов и правильную разметку глав.
          </p>
        </div>
      )}

      {/* Issues List */}
      <div style={{ display: 'flex', flexDirection: 'column', gap: '12px' }}>
        {visibleIssues.map((issue) => (
          <div
            key={issue.id}
            className="glass-card"
            style={{
              padding: '20px 24px',
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'space-between',
              gap: '24px',
              transition: 'transform 0.15s ease, border-color 0.15s ease',
            }}
            onMouseEnter={(e) => {
              e.currentTarget.style.borderColor = 'rgba(99, 102, 241, 0.4)';
              e.currentTarget.style.transform = 'translateY(-1px)';
            }}
            onMouseLeave={(e) => {
              e.currentTarget.style.borderColor = 'var(--border-subtle)';
              e.currentTarget.style.transform = 'translateY(0)';
            }}
          >
            {/* Left: Icon & Description */}
            <div style={{ display: 'flex', alignItems: 'flex-start', gap: '16px', flex: 1 }}>
              <div
                style={{
                  width: '42px',
                  height: '42px',
                  borderRadius: '12px',
                  background: 'rgba(239, 68, 68, 0.12)',
                  border: '1px solid rgba(239, 68, 68, 0.25)',
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'center',
                  flexShrink: 0,
                  marginTop: '2px',
                }}
              >
                <AlertTriangle size={20} color="var(--danger)" />
              </div>

              <div style={{ flex: 1 }}>
                <div
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    gap: '10px',
                    marginBottom: '6px',
                    flexWrap: 'wrap',
                  }}
                >
                  <span
                    style={{
                      fontSize: '11px',
                      fontWeight: 700,
                      padding: '3px 8px',
                      borderRadius: '6px',
                      background: 'rgba(239, 68, 68, 0.15)',
                      color: 'var(--danger)',
                      border: '1px solid rgba(239, 68, 68, 0.3)',
                    }}
                  >
                    {issue.kind_label || issue.kind}
                  </span>

                  <span
                    style={{
                      fontSize: '15px',
                      fontWeight: 700,
                      color: 'var(--text-primary)',
                    }}
                  >
                    {issue.book_title || 'Без названия'}
                  </span>

                  <span style={{ fontSize: '13px', color: 'var(--text-muted)' }}>
                    • {issue.book_authors || 'Автор не указан'}
                  </span>
                </div>

                <p
                  style={{
                    fontSize: '13.5px',
                    color: 'var(--text-secondary)',
                    marginBottom: issue.proposed_val ? '10px' : '0',
                    lineHeight: 1.5,
                  }}
                >
                  {issue.message}
                </p>

                {/* Diff Comparison if values present */}
                {issue.proposed_val && (
                  <div
                    style={{
                      display: 'flex',
                      alignItems: 'center',
                      gap: '10px',
                      padding: '8px 12px',
                      borderRadius: '8px',
                      background: 'rgba(0, 0, 0, 0.25)',
                      fontSize: '12.5px',
                      fontFamily: 'monospace',
                    }}
                  >
                    <span style={{ color: 'var(--danger)', textDecoration: 'line-through' }}>
                      {issue.current_val || '(пусто)'}
                    </span>
                    <ArrowRight size={14} color="var(--text-muted)" />
                    <span style={{ color: 'var(--success)', fontWeight: 600 }}>
                      {issue.proposed_val}
                    </span>
                  </div>
                )}
              </div>
            </div>

            {/* Right: Actions */}
            <div style={{ display: 'flex', alignItems: 'center', gap: '10px', flexShrink: 0 }}>
              <button
                onClick={() => onOpenBook(issue.book_id)}
                className="btn btn-secondary"
                style={{ padding: '8px 14px', fontSize: '13px' }}
                title="Перейти к инспектору книги"
              >
                <Eye size={15} />
                <span>Открыть</span>
              </button>

              <button
                onClick={() => handleIgnore(issue.id)}
                className="btn"
                style={{
                  padding: '8px 14px',
                  fontSize: '13px',
                  background: 'rgba(255, 255, 255, 0.05)',
                  color: 'var(--text-secondary)',
                  border: '1px solid var(--border-subtle)',
                }}
                title="Скрыть это замечание"
              >
                Игнорировать
              </button>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
};
