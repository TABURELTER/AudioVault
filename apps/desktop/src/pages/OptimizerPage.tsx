import React from 'react';
import { Zap, ArrowRight, CheckCircle } from 'lucide-react';
import type { OptimizerItem } from '../api/types';
import { formatBytes } from '../api/client';

interface OptimizerPageProps {
  items: OptimizerItem[];
  onTranscode: (copyId: number) => void;
}

export const OptimizerPage: React.FC<OptimizerPageProps> = ({ items, onTranscode }) => {
  const totalSavings = items.reduce((acc, item) => acc + item.savings_bytes, 0);

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: '24px' }}>
      {/* Page Header */}
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
        <div>
          <h2 style={{ fontSize: '20px', fontWeight: 700, color: '#fff' }}>
            Оптимизация и конвертация
          </h2>
          <p style={{ fontSize: '13px', color: 'var(--text-muted)' }}>
            Сборка сотен мелких файлов в аккуратный M4B с главами, сжатие без потери качества голоса.
          </p>
        </div>

        {totalSavings > 0 && (
          <div
            style={{
              padding: '8px 16px',
              borderRadius: 'var(--radius-md)',
              background: 'rgba(16, 185, 129, 0.15)',
              border: '1px solid rgba(16, 185, 129, 0.3)',
              display: 'flex',
              alignItems: 'center',
              gap: '10px',
            }}
          >
            <Zap size={18} color="var(--success)" />
            <div>
              <span style={{ fontSize: '11px', color: 'var(--text-muted)' }}>Потенциальная экономия:</span>
              <p style={{ fontSize: '14px', fontWeight: 700, color: 'var(--success)' }}>
                {formatBytes(totalSavings)}
              </p>
            </div>
          </div>
        )}
      </div>

      {/* Empty State */}
      {items.length === 0 ? (
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
          <CheckCircle size={48} color="var(--success)" style={{ marginBottom: '16px', opacity: 0.7 }} />
          <h3 style={{ fontSize: '18px', color: '#fff', marginBottom: '6px' }}>
            Все аудиокниги оптимально упакованы
          </h3>
          <p style={{ fontSize: '13px', maxWidth: '400px' }}>
            В коллекции нет раздутых файлов или кандидатов на упаковку в M4B.
          </p>
        </div>
      ) : (
        <div style={{ display: 'flex', flexDirection: 'column', gap: '14px' }}>
          {items.map((item) => (
            <div
              key={item.copy_id}
              className="glass-card"
              style={{
                padding: '20px',
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'space-between',
                gap: '20px',
              }}
            >
              {/* Left Info */}
              <div style={{ flex: 1 }}>
                <h3 style={{ fontSize: '15px', fontWeight: 700, color: '#fff', marginBottom: '4px' }}>
                  {item.book_title}
                </h3>
                <p style={{ fontSize: '13px', color: 'var(--text-secondary)', marginBottom: '10px' }}>
                  {item.authors || 'Неизвестный автор'}
                </p>

                {/* Conversion Comparison Badge */}
                <div style={{ display: 'flex', alignItems: 'center', gap: '12px', fontSize: '12px' }}>
                  <div
                    style={{
                      padding: '4px 10px',
                      borderRadius: '6px',
                      background: 'rgba(255, 255, 255, 0.05)',
                      color: 'var(--text-secondary)',
                      border: '1px solid var(--border-subtle)',
                    }}
                  >
                    Текущий: <strong style={{ color: '#fff' }}>{item.current_codec}</strong> ({formatBytes(item.current_size)})
                  </div>

                  <ArrowRight size={14} color="var(--text-muted)" />

                  <div
                    style={{
                      padding: '4px 10px',
                      borderRadius: '6px',
                      background: 'rgba(99, 102, 241, 0.15)',
                      color: '#a5b4fc',
                      border: '1px solid rgba(99, 102, 241, 0.3)',
                    }}
                  >
                    В цель: <strong style={{ color: '#fff' }}>{item.target_format}</strong> ({formatBytes(item.estimated_size)})
                  </div>

                  <span
                    style={{
                      fontWeight: 700,
                      color: 'var(--success)',
                      background: 'var(--success-bg)',
                      padding: '4px 8px',
                      borderRadius: '6px',
                    }}
                  >
                    -{item.savings_pct}% ({formatBytes(item.savings_bytes)})
                  </span>
                </div>
              </div>

              {/* Right Action */}
              <button
                onClick={() => onTranscode(item.copy_id)}
                className="btn btn-primary"
                style={{ padding: '10px 18px' }}
              >
                <Zap size={15} />
                <span>Собрать M4B</span>
              </button>
            </div>
          ))}
        </div>
      )}
    </div>
  );
};
