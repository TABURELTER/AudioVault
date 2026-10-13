import React from 'react';
import { Sparkles, Check } from 'lucide-react';
import type { DuplicatePair } from '../api/types';
import { formatBytes, formatDuration } from '../api/client';

interface DuplicatesPageProps {
  duplicates: DuplicatePair[];
  onFindDuplicates: () => void;
  onResolve: (id: number, action: string) => void;
  isSearching: boolean;
}

export const DuplicatesPage: React.FC<DuplicatesPageProps> = ({
  duplicates,
  onFindDuplicates,
  onResolve,
  isSearching,
}) => {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: '24px' }}>
      {/* Header Bar */}
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
        <div>
          <h2 style={{ fontSize: '20px', fontWeight: 700, color: '#fff' }}>
            Потенциальные дубликаты
          </h2>
          <p style={{ fontSize: '13px', color: 'var(--text-muted)' }}>
            Сравнение хэшей, акустических отпечатков Chromaprint и метаданных для очистки дискового пространства.
          </p>
        </div>

        <button onClick={onFindDuplicates} className="btn btn-primary" disabled={isSearching}>
          <Sparkles size={15} />
          <span>{isSearching ? 'Анализ коллекции...' : 'Найти дубликаты'}</span>
        </button>
      </div>

      {/* Empty State */}
      {duplicates.length === 0 ? (
        <div
          style={{
            display: 'flex',
            flexDirection: 'column',
            alignItems: 'center',
            justifyContent: 'center',
            padding: '80px 20px',
            textAlign: 'center',
            color: 'var(--text-muted)',
            background: 'var(--bg-surface)',
            borderRadius: 'var(--radius-lg)',
            border: '1px solid var(--border-subtle)',
          }}
        >
          <Sparkles size={48} color="var(--accent-primary)" style={{ marginBottom: '16px', opacity: 0.6 }} />
          <h3 style={{ fontSize: '18px', color: '#fff', marginBottom: '6px' }}>
            Дубликатов не найдено
          </h3>
          <p style={{ fontSize: '13px', maxWidth: '420px', marginBottom: '20px' }}>
            Все копии в вашей коллекции уникальны, либо анализ еще не запускался.
          </p>
          <button onClick={onFindDuplicates} className="btn btn-primary">
            <Sparkles size={14} />
            <span>Запустить глубокий поиск</span>
          </button>
        </div>
      ) : (
        <div style={{ display: 'flex', flexDirection: 'column', gap: '16px' }}>
          {duplicates.map((dup) => {
            const scorePct = Math.round(dup.score * 100);

            return (
              <div
                key={dup.id}
                className="glass-card"
                style={{
                  padding: '20px',
                  display: 'flex',
                  flexDirection: 'column',
                  gap: '16px',
                }}
              >
                {/* Title & Verdict Header */}
                <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                  <div>
                    <h3 style={{ fontSize: '16px', fontWeight: 700, color: '#fff' }}>
                      {dup.title}
                    </h3>
                    <p style={{ fontSize: '12px', color: 'var(--text-muted)' }}>
                      {dup.evidence || 'Обнаружено сходство метаданных и структуры'}
                    </p>
                  </div>

                  <div style={{ display: 'flex', alignItems: 'center', gap: '12px' }}>
                    <span
                      style={{
                        fontSize: '12px',
                        fontWeight: 700,
                        padding: '4px 10px',
                        borderRadius: '9999px',
                        background:
                          scorePct > 80
                            ? 'var(--danger-bg)'
                            : scorePct > 50
                            ? 'var(--warning-bg)'
                            : 'rgba(99, 102, 241, 0.15)',
                        color:
                          scorePct > 80
                            ? 'var(--danger)'
                            : scorePct > 50
                            ? 'var(--warning)'
                            : '#a5b4fc',
                        border: '1px solid currentColor',
                      }}
                    >
                      Сходство: {scorePct}% ({dup.verdict})
                    </span>
                  </div>
                </div>

                {/* Comparison Columns (A vs B) */}
                <div
                  style={{
                    display: 'grid',
                    gridTemplateColumns: '1fr 1fr',
                    gap: '16px',
                  }}
                >
                  {/* Copy A */}
                  <div
                    style={{
                      background: 'var(--bg-input)',
                      padding: '14px',
                      borderRadius: 'var(--radius-md)',
                      border: '1px solid var(--border-card)',
                      display: 'flex',
                      flexDirection: 'column',
                      gap: '8px',
                    }}
                  >
                    <div style={{ display: 'flex', justifyContent: 'space-between' }}>
                      <span style={{ fontSize: '11px', fontWeight: 700, color: 'var(--accent-primary)' }}>
                        ВАРИАНТ А
                      </span>
                      <span className={`badge ${dup.a.online ? 'badge-online' : 'badge-offline'}`}>
                        {dup.a.volume_name}
                      </span>
                    </div>

                    <div style={{ fontSize: '12.5px', color: '#fff', fontWeight: 600 }}>
                      Чтец: {dup.a.narrator || 'Не указан'}
                    </div>

                    <div style={{ fontSize: '11.5px', color: 'var(--text-muted)', wordBreak: 'break-all' }}>
                      {dup.a.rel_path}
                    </div>

                    <div style={{ display: 'flex', gap: '14px', fontSize: '11px', color: 'var(--text-secondary)' }}>
                      <span>Формат: <strong style={{ color: '#fff' }}>{dup.a.format}</strong></span>
                      <span>Размер: <strong style={{ color: '#fff' }}>{formatBytes(dup.a.size)}</strong></span>
                      <span>Длительность: <strong style={{ color: '#fff' }}>{formatDuration(dup.a.duration_ms)}</strong></span>
                    </div>

                    <button
                      onClick={() => onResolve(dup.id, 'keep_a')}
                      className="btn btn-secondary"
                      style={{ marginTop: '6px', fontSize: '12px', padding: '6px 12px' }}
                    >
                      <Check size={14} color="var(--success)" />
                      <span>Оставить этот вариант</span>
                    </button>
                  </div>

                  {/* Copy B */}
                  <div
                    style={{
                      background: 'var(--bg-input)',
                      padding: '14px',
                      borderRadius: 'var(--radius-md)',
                      border: '1px solid var(--border-card)',
                      display: 'flex',
                      flexDirection: 'column',
                      gap: '8px',
                    }}
                  >
                    <div style={{ display: 'flex', justifyContent: 'space-between' }}>
                      <span style={{ fontSize: '11px', fontWeight: 700, color: '#a855f7' }}>
                        ВАРИАНТ B
                      </span>
                      <span className={`badge ${dup.b.online ? 'badge-online' : 'badge-offline'}`}>
                        {dup.b.volume_name}
                      </span>
                    </div>

                    <div style={{ fontSize: '12.5px', color: '#fff', fontWeight: 600 }}>
                      Чтец: {dup.b.narrator || 'Не указан'}
                    </div>

                    <div style={{ fontSize: '11.5px', color: 'var(--text-muted)', wordBreak: 'break-all' }}>
                      {dup.b.rel_path}
                    </div>

                    <div style={{ display: 'flex', gap: '14px', fontSize: '11px', color: 'var(--text-secondary)' }}>
                      <span>Формат: <strong style={{ color: '#fff' }}>{dup.b.format}</strong></span>
                      <span>Размер: <strong style={{ color: '#fff' }}>{formatBytes(dup.b.size)}</strong></span>
                      <span>Длительность: <strong style={{ color: '#fff' }}>{formatDuration(dup.b.duration_ms)}</strong></span>
                    </div>

                    <button
                      onClick={() => onResolve(dup.id, 'keep_b')}
                      className="btn btn-secondary"
                      style={{ marginTop: '6px', fontSize: '12px', padding: '6px 12px' }}
                    >
                      <Check size={14} color="var(--success)" />
                      <span>Оставить этот вариант</span>
                    </button>
                  </div>
                </div>

                {/* Additional Resolution Buttons */}
                <div style={{ display: 'flex', justifyContent: 'flex-end', gap: '8px', borderTop: '1px solid var(--border-subtle)', paddingTop: '10px' }}>
                  <button
                    onClick={() => onResolve(dup.id, 'keep_both')}
                    className="btn btn-secondary"
                    style={{ fontSize: '11.5px', padding: '5px 10px' }}
                  >
                    Оставить обе копии
                  </button>
                  <button
                    onClick={() => onResolve(dup.id, 'mark_different')}
                    className="btn btn-secondary"
                    style={{ fontSize: '11.5px', padding: '5px 10px' }}
                  >
                    Разные записи / переводы
                  </button>
                  <button
                    onClick={() => onResolve(dup.id, 'ignore')}
                    className="btn btn-secondary"
                    style={{ fontSize: '11.5px', padding: '5px 10px' }}
                  >
                    Игнорировать
                  </button>
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
};
