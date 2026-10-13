import { Fragment, useState } from "react";
import { Copy, ScanSearch, ShieldCheck } from "lucide-react";
import type { DuplicatePair } from "../api/types";
import { formatBytes, formatDuration } from "../api/client";
import { duplicateEvidence, duplicateVerdict } from "../api/presentation";
import { EmptyState, StatusIndicator } from "../components/Primitives";
interface Props {
  duplicates: DuplicatePair[];
  onFindDuplicates: () => void;
  onResolve: (id: number, action: string) => Promise<void> | void;
  isSearching: boolean;
}
export function DuplicatesPage({
  duplicates,
  onFindDuplicates,
  onResolve,
  isSearching,
}: Props) {
  const [selected, setSelected] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);
  const pair = duplicates.find((d) => d.id === selected) || duplicates[0];
  const resolve = async (action: string) => {
    if (!pair || busy) return;
    setBusy(true);
    try {
      await onResolve(pair.id, action);
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="collection-page">
      <div className="page-intro">
        <p>
          <strong>
            {
              duplicates.filter(
                (d) => d.verdict !== "Same book, different narrator",
              ).length
            }{" "}
            пар для проверки
            {duplicates.some(
              (d) => d.verdict === "Same book, different narrator",
            ) &&
              ` · ${duplicates.filter((d) => d.verdict === "Same book, different narrator").length} с другим чтецом`}
          </strong>
          <br />
          Сравните записи и выберите, какие копии сохранить.
        </p>
        <button
          className="btn btn-primary"
          disabled={isSearching}
          onClick={onFindDuplicates}
        >
          <ScanSearch size={14} />
          {isSearching ? "Анализ…" : "Найти дубликаты"}
        </button>
      </div>
      {!pair ? (
        <EmptyState
          title="Нет пар для проверки"
          description="Запустите анализ коллекции, чтобы найти похожие записи. Совпадение названия само по себе не доказывает дубликат."
        />
      ) : (
        <div className="duplicate-workspace">
          <div className="duplicate-list" aria-label="Пары дубликатов">
            {duplicates.map((d) => (
              <button
                key={d.id}
                className={`duplicate-option ${d.id === pair.id ? "active" : ""}`}
                aria-pressed={d.id === pair.id}
                onClick={() => setSelected(d.id)}
              >
                <strong title={d.title}>{d.title}</strong>
                <small>{d.authors || "Автор не указан"}</small>
                <small>
                  <Copy size={10} /> {Math.round(d.score * 100)}% сходства ·{" "}
                  {d.a.format} / {d.b.format}
                </small>
              </button>
            ))}
          </div>
          <div className="duplicate-review">
            <h2>{pair.title}</h2>
            <p>{pair.authors || "Автор не указан"}</p>
            <div className="evidence">
              <strong>
                {Math.round(pair.score * 100)}% сходства ·{" "}
                {duplicateVerdict(pair.verdict)}
              </strong>
              <p style={{ marginTop: 6 }}>{duplicateEvidence(pair)}</p>
              <details className="evidence-details">
                <summary>Подробные основания</summary>
                <p>{pair.evidence || "Нет дополнительных сведений"}</p>
              </details>
            </div>
            <div className="comparison">
              <div />
              <div className="comparison-heading">Вариант A</div>
              <div className="comparison-heading">Вариант B</div>
              {[
                [
                  "Накопитель",
                  <StatusIndicator
                    key="a"
                    online={pair.a.online}
                    label={pair.a.volume_name}
                  />,
                  <StatusIndicator
                    key="b"
                    online={pair.b.online}
                    label={pair.b.volume_name}
                  />,
                ],
                [
                  "Чтец",
                  pair.a.narrator || "Не указан",
                  pair.b.narrator || "Не указан",
                ],
                ["Формат", pair.a.format, pair.b.format],
                ["Размер", formatBytes(pair.a.size), formatBytes(pair.b.size)],
                [
                  "Длительность",
                  pair.a.duration_ms ? formatDuration(pair.a.duration_ms) : "—",
                  pair.b.duration_ms ? formatDuration(pair.b.duration_ms) : "—",
                ],
                ["Путь", pair.a.rel_path, pair.b.rel_path],
              ].map(([label, a, b], i) => (
                <Fragment key={i}>
                  <div>{label}</div>
                  <div>{a}</div>
                  <div>{b}</div>
                </Fragment>
              ))}
              <div>Решение</div>
              <div>
                <button
                  disabled={busy}
                  className="btn btn-secondary"
                  onClick={() => resolve("keep_a")}
                >
                  Оставить A
                </button>
              </div>
              <div>
                <button
                  disabled={busy}
                  className="btn btn-secondary"
                  onClick={() => resolve("keep_b")}
                >
                  Оставить B
                </button>
              </div>
            </div>
            <div className="review-actions">
              <button
                disabled={busy}
                className="btn btn-secondary"
                onClick={() => resolve("keep_both")}
              >
                Оставить обе
              </button>
              <button
                disabled={busy}
                className="btn btn-quiet"
                onClick={() => resolve("mark_different")}
              >
                Разные записи
              </button>
              <button
                disabled={busy}
                className="btn btn-quiet"
                onClick={() => resolve("ignore")}
              >
                Игнорировать
              </button>
            </div>
            <p
              className="muted"
              style={{ fontSize: 11, marginTop: 16, display: "flex", gap: 7 }}
            >
              <ShieldCheck size={14} />
              Решение отмечается в каталоге. Файлы остаются на диске.
            </p>
          </div>
        </div>
      )}
    </div>
  );
}
