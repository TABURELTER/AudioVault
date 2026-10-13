import { ArrowRight, Package } from "lucide-react";
import type { OptimizerItem } from "../api/types";
import { formatBytes } from "../api/client";
import { EmptyState } from "../components/Primitives";
export function OptimizerPage({
  items,
  onTranscode,
}: {
  items: OptimizerItem[];
  onTranscode: (id: number, format?: string) => void;
}) {
  const savings = items.reduce((sum, i) => sum + i.savings_bytes, 0);
  return (
    <div>
      <div className="page-intro">
        <p>
          <strong>
            {items.length} кандидатов · до {formatBytes(savings)} экономии
          </strong>
          <br />
          Соберите главы в один файл или уменьшите размер записи. Оценка размера
          приблизительная.
        </p>
      </div>
      {!items.length ? (
        <EmptyState
          title="Нет кандидатов на оптимизацию"
          description="После сканирования здесь появятся записи, которые можно компактнее упаковать."
        />
      ) : (
        <div className="section-list">
          {items.map((i) => (
            <div className="activity-row" key={i.copy_id}>
              <Package size={20} />
              <div className="activity-body">
                <h3 title={i.book_title}>{i.book_title}</h3>
                <p>{i.authors || "Автор не указан"}</p>
                <div className="activity-metrics">
                  <span>
                    {i.current_codec} · {formatBytes(i.current_size)}
                  </span>
                  <ArrowRight size={13} />
                  <span>
                    {i.target_format} · ~{formatBytes(i.estimated_size)}
                  </span>
                  <span style={{ color: "var(--success)" }}>
                    −{i.savings_pct}%
                  </span>
                </div>
                {i.notes && <p>{i.notes}</p>}
                <p>
                  {i.reencode
                    ? "С перекодированием аудио"
                    : "Перепаковка без перекодирования"}
                </p>
              </div>
              <div className="storage-actions">
                <button
                  className="btn btn-secondary"
                  onClick={() => onTranscode(i.copy_id, "m4b")}
                >
                  В M4B
                </button>
                <button
                  className="btn btn-quiet"
                  onClick={() => onTranscode(i.copy_id, "mp3")}
                >
                  В MP3
                </button>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
