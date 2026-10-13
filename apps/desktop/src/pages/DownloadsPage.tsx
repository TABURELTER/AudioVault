import { useEffect, useState } from "react";
import {
  Download,
  Pause,
  Play,
  Trash2,
  ArrowDown,
  ArrowUp,
  RefreshCw,
} from "lucide-react";
import { api, formatBytes } from "../api/client";
import type { Download as DownloadItem } from "../api/types";
import { EmptyState, ProgressBar, LoadingRows } from "../components/Primitives";
import { Modal } from "../components/Modal";
export function DownloadsPage({
  onNavigateToRuTracker,
}: {
  onNavigateToRuTracker: () => void;
}) {
  const [downloads, setDownloads] = useState<DownloadItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [remove, setRemove] = useState<DownloadItem | null>(null);
  const [busy, setBusy] = useState<number | null>(null);
  useEffect(() => {
    let alive = true;
    let pending = false;
    const load = async () => {
      if (pending) return;
      pending = true;
      try {
        const data = await api.getDownloads();
        if (alive) {
          setDownloads(data);
          setError("");
        }
      } catch {
        if (alive)
          setError("Не удалось обновить загрузки. Повторим автоматически.");
      } finally {
        pending = false;
        if (alive) setLoading(false);
      }
    };
    load();
    const timer = setInterval(load, 2000);
    return () => {
      alive = false;
      clearInterval(timer);
    };
  }, []);
  const action = async (
    id: number,
    operation: "pause" | "resume" | "delete",
  ) => {
    setBusy(id);
    setError("");
    try {
      if (operation === "pause") await api.pauseDownload(id);
      else if (operation === "resume") await api.resumeDownload(id);
      else await api.deleteDownload(id);
      setDownloads(await api.getDownloads());
      setRemove(null);
    } catch {
      setError(
        "Не удалось изменить загрузку. Проверьте подключение и повторите.",
      );
    } finally {
      setBusy(null);
    }
  };
  const labels: Record<string, string> = {
    downloading: "Загрузка",
    paused: "Пауза",
    completed: "Завершено",
    seeding: "Раздача",
    checking: "Проверка",
    queued: "В очереди",
    error: "Ошибка",
    failed: "Ошибка",
  };
  return (
    <div>
      <div className="page-intro">
        <p>
          <strong>{downloads.length} загрузок</strong>
          <br />
          Прогресс обновляется автоматически. Завершённые книги добавляются в
          каталог.
        </p>
        <button className="btn btn-primary" onClick={onNavigateToRuTracker}>
          <Download size={14} />
          Найти раздачу
        </button>
      </div>
      {error && (
        <div className="notice error" role="alert">
          {error}
        </div>
      )}
      {loading ? (
        <LoadingRows />
      ) : !downloads.length ? (
        <EmptyState
          title="Загрузок пока нет"
          description="Найдите аудиокнигу на RuTracker и добавьте её в очередь. Прогресс и скорость появятся здесь."
          action={
            <button
              className="btn btn-secondary"
              onClick={onNavigateToRuTracker}
            >
              Открыть RuTracker
            </button>
          }
        />
      ) : (
        <div className="section-list">
          {downloads.map((item) => {
            const state = item.state.toLowerCase();
            const finished =
              item.progress >= 100 ||
              state === "completed" ||
              state === "seeding";
            return (
              <div className="activity-row" key={item.id}>
                <Download size={20} />
                <div className="activity-body">
                  <h3 title={item.name}>{item.name}</h3>
                  <div className="activity-metrics">
                    <span>
                      {formatBytes(item.downloaded_bytes)} /{" "}
                      {formatBytes(item.total_bytes)}
                    </span>
                    <span>{item.progress.toFixed(1)}%</span>
                    {state === "downloading" && (
                      <span>
                        <ArrowDown size={12} />
                        {item.down_speed || "—"}
                      </span>
                    )}
                    {item.up_speed && (
                      <span>
                        <ArrowUp size={12} />
                        {item.up_speed}
                      </span>
                    )}
                    {!finished && item.eta_str && (
                      <span>Осталось {item.eta_str}</span>
                    )}
                    <span>
                      {item.seeds} сидов · {item.peers} пиров
                    </span>
                  </div>
                  <ProgressBar value={item.progress} label={item.name} />
                </div>
                <div className={`activity-status ${finished ? "success" : ""}`}>
                  {labels[state] || item.state}
                </div>
                <div className="storage-actions">
                  {!finished && (
                    <button
                      className="plain-icon"
                      disabled={busy === item.id}
                      aria-label={
                        state === "paused"
                          ? "Продолжить загрузку"
                          : "Приостановить загрузку"
                      }
                      title={state === "paused" ? "Продолжить" : "Пауза"}
                      onClick={() =>
                        action(item.id, state === "paused" ? "resume" : "pause")
                      }
                    >
                      {busy === item.id ? (
                        <RefreshCw size={14} className="spin" />
                      ) : state === "paused" ? (
                        <Play size={14} />
                      ) : (
                        <Pause size={14} />
                      )}
                    </button>
                  )}
                  <button
                    className="plain-icon"
                    title="Удалить из списка"
                    aria-label={`Удалить ${item.name} из списка`}
                    onClick={() => setRemove(item)}
                  >
                    <Trash2 size={14} />
                  </button>
                </div>
              </div>
            );
          })}
        </div>
      )}
      {remove && (
        <Modal
          title="Удалить загрузку из списка?"
          onClose={() => setRemove(null)}
        >
          <p>{remove.name}</p>
          <p>
            Запись раздачи будет удалена. Скачанные файлы останутся на диске.
          </p>
          <div className="modal-actions">
            <button
              className="btn btn-secondary"
              onClick={() => setRemove(null)}
            >
              Отмена
            </button>
            <button
              className="btn btn-danger"
              disabled={busy === remove.id}
              onClick={() => action(remove.id, "delete")}
            >
              Удалить из списка
            </button>
          </div>
        </Modal>
      )}
    </div>
  );
}
