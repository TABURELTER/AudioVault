import {
  CheckCircle2,
  XCircle,
  HardDrive,
  Activity,
  Clock,
} from "lucide-react";
import type { Task } from "../api/types";
import { taskLabels } from "../api/presentation";
import { EmptyState, ProgressBar } from "../components/Primitives";
export function TasksPage({
  tasks,
  onCancelTask,
}: {
  tasks: Task[];
  onCancelTask: (id: number) => void;
}) {
  const labels = {
    running: "Выполняется",
    queued: "В очереди",
    completed: "Завершена",
    failed: "Ошибка",
    cancelled: "Отменена",
    paused: "Приостановлена",
  };
  const ordered = [...tasks].sort(
    (a, b) => Number(b.state === "running") - Number(a.state === "running"),
  );
  return (
    <div>
      <div className="page-intro">
        <p>
          <strong>
            {tasks.filter((t) => t.state === "running").length} активных задач
          </strong>
          <br />
          Сканирование и конвертация выполняются в фоне. Можно продолжать работу
          с коллекцией.
        </p>
      </div>
      {!tasks.length ? (
        <EmptyState
          title="Всё спокойно"
          description="Здесь появятся фоновые задачи: сканирование дисков, поиск дубликатов и конвертация."
        />
      ) : (
        <div className="section-list">
          {ordered.map((t) => (
            <div className="activity-row" key={t.id}>
              {t.state === "completed" ? (
                <CheckCircle2 size={19} />
              ) : t.kind.includes("scan") ? (
                <HardDrive size={19} />
              ) : t.state === "queued" ? (
                <Clock size={19} />
              ) : (
                <Activity size={19} />
              )}
              <div className="activity-body">
                <h3>
                  {/^[a-z_ ]+$/i.test(t.label)
                    ? taskLabels[t.kind] || t.label
                    : t.label}
                </h3>
                <p>
                  {t.description.replace(
                    /Состояние: [^,]+, попыток: (\d+)/,
                    "Попытка №$1",
                  )}
                </p>
                {t.state === "running" && (
                  <ProgressBar value={t.progress * 100} label={t.label} />
                )}
              </div>
              <div
                className={`activity-status ${t.state === "completed" ? "success" : t.state === "failed" ? "failed" : ""}`}
              >
                {labels[t.state]}
                {t.state === "running" && (
                  <div>{Math.round(t.progress * 100)}%</div>
                )}
              </div>
              {(t.state === "running" || t.state === "queued") && (
                <button
                  className="btn btn-quiet"
                  onClick={() => onCancelTask(t.id)}
                >
                  <XCircle size={14} />
                  Отменить
                </button>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
