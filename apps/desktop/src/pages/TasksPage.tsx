import React from 'react';
import { CheckCircle2, XCircle } from 'lucide-react';
import type { Task } from '../api/types';

interface TasksPageProps {
  tasks: Task[];
  onCancelTask: (id: number) => void;
}

export const TasksPage: React.FC<TasksPageProps> = ({ tasks, onCancelTask }) => {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: '24px' }}>
      <div>
        <h2 style={{ fontSize: '20px', fontWeight: 700, color: '#fff' }}>
          Очереди задач и операций
        </h2>
        <p style={{ fontSize: '13px', color: 'var(--text-muted)' }}>
          Фоновые сканеры дисков, расчет акустических отпечатков Chromaprint и конвертация FFmpeg.
        </p>
      </div>

      {tasks.length === 0 ? (
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
          <CheckCircle2 size={48} color="var(--success)" style={{ marginBottom: '16px', opacity: 0.7 }} />
          <h3 style={{ fontSize: '17px', color: '#fff', marginBottom: '4px' }}>
            Все задачи выполнены
          </h3>
          <p style={{ fontSize: '13px' }}>
            В очереди нет активных процессов.
          </p>
        </div>
      ) : (
        <div style={{ display: 'flex', flexDirection: 'column', gap: '12px' }}>
          {tasks.map((task) => {
            const isRunning = task.state === 'running';
            const isCompleted = task.state === 'completed';
            const isFailed = task.state === 'failed';

            return (
              <div
                key={task.id}
                className="glass-card"
                style={{
                  padding: '18px',
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'space-between',
                  gap: '20px',
                }}
              >
                <div style={{ flex: 1 }}>
                  <div style={{ display: 'flex', alignItems: 'center', gap: '10px', marginBottom: '6px' }}>
                    <span
                      style={{
                        fontSize: '11px',
                        fontWeight: 700,
                        padding: '2px 8px',
                        borderRadius: '9999px',
                        background: isRunning
                          ? 'var(--info-bg)'
                          : isCompleted
                          ? 'var(--success-bg)'
                          : isFailed
                          ? 'var(--danger-bg)'
                          : 'rgba(255, 255, 255, 0.08)',
                        color: isRunning
                          ? 'var(--info)'
                          : isCompleted
                          ? 'var(--success)'
                          : isFailed
                          ? 'var(--danger)'
                          : 'var(--text-muted)',
                      }}
                    >
                      {task.state.toUpperCase()}
                    </span>

                    <h4 style={{ fontSize: '14.5px', fontWeight: 600, color: '#fff' }}>
                      {task.label}
                    </h4>
                  </div>

                  <p style={{ fontSize: '12px', color: 'var(--text-muted)', marginBottom: isRunning ? '8px' : 0 }}>
                    {task.description}
                  </p>

                  {/* Progress Bar for Running tasks */}
                  {isRunning && (
                    <div
                      style={{
                        width: '100%',
                        height: '6px',
                        background: 'rgba(255, 255, 255, 0.1)',
                        borderRadius: '9999px',
                        overflow: 'hidden',
                        marginTop: '8px',
                      }}
                    >
                      <div
                        style={{
                          width: `${Math.round(task.progress * 100)}%`,
                          height: '100%',
                          background: 'var(--accent-gradient)',
                          borderRadius: '9999px',
                          transition: 'width 0.2s',
                        }}
                      />
                    </div>
                  )}
                </div>

                {/* Cancel Button */}
                {(isRunning || task.state === 'queued') && (
                  <button
                    onClick={() => onCancelTask(task.id)}
                    className="btn btn-secondary"
                    style={{ fontSize: '12px' }}
                  >
                    <XCircle size={14} />
                    <span>Отмена</span>
                  </button>
                )}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
};
