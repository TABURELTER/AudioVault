import { useMemo, useState } from "react";
import {
  ScanSearch,
  ShieldCheck,
  HardDrive,
  CheckCircle2,
  Sparkles,
  Layers,
} from "lucide-react";
import type { DuplicatePair, DuplicateCopySide } from "../api/types";
import { api, formatBytes, formatDuration } from "../api/client";
import { duplicateEvidence, duplicateVerdict } from "../api/presentation";
import { EmptyState } from "../components/Primitives";
import { DeletionConfirmModal, type ItemToDelete } from "../components/DeletionConfirmModal";

interface Props {
  duplicates: DuplicatePair[];
  onFindDuplicates: () => void;
  onResolve: (id: number, action: string) => Promise<void> | void;
  isSearching: boolean;
}

interface ClusterCopy extends DuplicateCopySide {
  pairIds: number[];
}

interface DuplicateCluster {
  id: string;
  title: string;
  authors: string;
  maxScore: number;
  verdict: string;
  evidence: string;
  pairIds: number[];
  copies: ClusterCopy[];
}

export function DuplicatesPage({
  duplicates,
  onFindDuplicates,
  onResolve,
  isSearching,
}: Props) {
  const [selectedClusterId, setSelectedClusterId] = useState<string | null>(null);
  const [savedCopyIds, setSavedCopyIds] = useState<Record<string, Set<number>>>({});
  const [busy, setBusy] = useState(false);
  const [deleteModalOpen, setDeleteModalOpen] = useState(false);
  const [itemsPendingDeletion, setItemsPendingDeletion] = useState<ItemToDelete[]>([]);

  // Cluster duplicate pairs by shared copy IDs or matching title+author
  const clusters = useMemo(() => {
    if (!duplicates.length) return [];

    const copyMap = new Map<number, ClusterCopy>();
    const pairGroups: Array<{ pair: DuplicatePair; copies: number[] }> = [];

    for (const p of duplicates) {
      if (!copyMap.has(p.a.copy_id)) {
        copyMap.set(p.a.copy_id, { ...p.a, pairIds: [p.id] });
      } else {
        copyMap.get(p.a.copy_id)!.pairIds.push(p.id);
      }

      if (!copyMap.has(p.b.copy_id)) {
        copyMap.set(p.b.copy_id, { ...p.b, pairIds: [p.id] });
      } else {
        copyMap.get(p.b.copy_id)!.pairIds.push(p.id);
      }

      pairGroups.push({ pair: p, copies: [p.a.copy_id, p.b.copy_id] });
    }

    // Disjoint-set union on copies
    const parent = new Map<number, number>();
    const find = (i: number): number => {
      if (!parent.has(i)) parent.set(i, i);
      if (parent.get(i) !== i) {
        parent.set(i, find(parent.get(i)!));
      }
      return parent.get(i)!;
    };

    const union = (i: number, j: number) => {
      const rootI = find(i);
      const rootJ = find(j);
      if (rootI !== rootJ) {
        parent.set(rootI, rootJ);
      }
    };

    for (const pg of pairGroups) {
      union(pg.copies[0], pg.copies[1]);
    }

    const clusterBuckets = new Map<number, DuplicatePair[]>();
    for (const pg of pairGroups) {
      const root = find(pg.copies[0]);
      if (!clusterBuckets.has(root)) {
        clusterBuckets.set(root, []);
      }
      clusterBuckets.get(root)!.push(pg.pair);
    }

    const result: DuplicateCluster[] = [];
    let idx = 0;
    for (const pairs of clusterBuckets.values()) {
      idx++;
      const uniqueCopyIds = new Set<number>();
      const pairIds = pairs.map((p) => p.id);
      let maxScore = 0;
      let verdict = pairs[0].verdict;
      let evidence = pairs[0].evidence;
      let title = pairs[0].title;
      let authors = pairs[0].authors;

      for (const p of pairs) {
        uniqueCopyIds.add(p.a.copy_id);
        uniqueCopyIds.add(p.b.copy_id);
        if (p.score > maxScore) {
          maxScore = p.score;
          verdict = p.verdict;
          evidence = p.evidence;
          title = p.title;
          authors = p.authors;
        }
      }

      const copiesList: ClusterCopy[] = Array.from(uniqueCopyIds)
        .map((cid) => copyMap.get(cid)!)
        .filter(Boolean);

      result.push({
        id: `cluster-${idx}-${copiesList[0]?.copy_id || 0}`,
        title,
        authors,
        maxScore,
        verdict,
        evidence,
        pairIds,
        copies: copiesList,
      });
    }

    return result;
  }, [duplicates]);

  const activeCluster =
    clusters.find((c) => c.id === selectedClusterId) || clusters[0];

  // Selected copies to KEEP for the active cluster
  const activeKeptIds = useMemo(() => {
    if (!activeCluster) return new Set<number>();
    if (savedCopyIds[activeCluster.id]) {
      return savedCopyIds[activeCluster.id];
    }
    // Default: keep the highest-quality copy (or first copy)
    const bestCopy = [...activeCluster.copies].sort((x, y) => {
      if (x.online !== y.online) return x.online ? -1 : 1;
      return y.size - x.size;
    })[0];
    return new Set(bestCopy ? [bestCopy.copy_id] : []);
  }, [activeCluster, savedCopyIds]);

  const toggleKeepCopy = (clusterId: string, copyId: number) => {
    setSavedCopyIds((prev) => {
      const current = new Set(prev[clusterId] || activeKeptIds);
      if (current.has(copyId)) {
        current.delete(copyId);
      } else {
        current.add(copyId);
      }
      return { ...prev, [clusterId]: current };
    });
  };

  const handleSelectAllCopies = (clusterId: string) => {
    if (!activeCluster) return;
    setSavedCopyIds((prev) => ({
      ...prev,
      [clusterId]: new Set(activeCluster.copies.map((c) => c.copy_id)),
    }));
  };

  const handleSelectBestQualityOnly = (clusterId: string) => {
    if (!activeCluster) return;
    const bestCopy = [...activeCluster.copies].sort((x, y) => {
      if (x.online !== y.online) return x.online ? -1 : 1;
      return y.size - x.size;
    })[0];
    if (bestCopy) {
      setSavedCopyIds((prev) => ({
        ...prev,
        [clusterId]: new Set([bestCopy.copy_id]),
      }));
    }
  };

  // Apply decision: Keep selected, mark or delete unselected
  const handleApplyDecision = async () => {
    if (!activeCluster || busy) return;

    const unselectedCopies = activeCluster.copies.filter(
      (c) => !activeKeptIds.has(c.copy_id),
    );

    if (unselectedCopies.length === 0) {
      // User chose to keep all copies!
      setBusy(true);
      try {
        for (const pid of activeCluster.pairIds) {
          await onResolve(pid, "keep_both");
        }
      } finally {
        setBusy(false);
      }
      return;
    }

    // Prompt deletion for unselected copies
    const toDelete: ItemToDelete[] = unselectedCopies.map((c) => ({
      id: c.book_id || c.copy_id,
      type: "book" as const,
      title: `${activeCluster.title} (${c.volume_name})`,
      authors: activeCluster.authors,
      volumeName: c.volume_name,
      volumeOnline: c.online,
      size: c.size,
      path: c.rel_path,
    }));

    setItemsPendingDeletion(toDelete);
    setDeleteModalOpen(true);
  };

  const handleConfirmDeletion = async () => {
    if (!activeCluster) return;
    setBusy(true);
    try {
      const unselectedCopies = activeCluster.copies.filter(
        (c) => !activeKeptIds.has(c.copy_id),
      );
      const bookIdsToDelete = unselectedCopies
        .map((c) => c.book_id)
        .filter((id): id is number => typeof id === "number" && id > 0);

      if (bookIdsToDelete.length > 0) {
        await api.executeDeletion(bookIdsToDelete, []);
      }

      for (const pid of activeCluster.pairIds) {
        await onResolve(pid, "keep_both");
      }
      setDeleteModalOpen(false);
    } catch (e) {
      console.error("handleConfirmDeletion error:", e);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="collection-page">
      <div className="page-intro">
        <p>
          <strong>
            {clusters.length} групп дубликатов на различных накопителях
          </strong>
          <br />
          Вы можете выбрать любое количество копий для сохранения или отметить лишние копии на удаление.
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

      {!activeCluster ? (
        <EmptyState
          title="Нет дубликатов для проверки"
          description="Запустите анализ коллекции, чтобы найти совпадающие или похожие аудиокниги на разных дисках."
        />
      ) : (
        <div className="duplicate-workspace">
          {/* Left List of Duplicate Groups */}
          <div className="duplicate-list" aria-label="Группы дубликатов">
            {clusters.map((c) => {
              const isSelected = c.id === activeCluster.id;
              const diskNames = Array.from(new Set(c.copies.map((cp) => cp.volume_name))).join(" | ");
              return (
                <button
                  key={c.id}
                  className={`duplicate-option ${isSelected ? "active" : ""}`}
                  aria-pressed={isSelected}
                  onClick={() => setSelectedClusterId(c.id)}
                >
                  <strong title={c.title}>{c.title}</strong>
                  <small>{c.authors || "Автор не указан"}</small>
                  <small style={{ display: "flex", alignItems: "center", gap: 5, color: "var(--accent-hover)" }}>
                    <Layers size={11} /> {c.copies.length} копии ({diskNames})
                  </small>
                </button>
              );
            })}
          </div>

          {/* Right Workspace: Multi-Drive Comparison Cards */}
          <div className="duplicate-review" style={{ display: "flex", flexDirection: "column", gap: 14 }}>
            <div>
              <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", flexWrap: "wrap", gap: 10 }}>
                <h2>{activeCluster.title}</h2>
                <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                  <button
                    className="btn btn-secondary"
                    onClick={() => handleSelectBestQualityOnly(activeCluster.id)}
                    style={{ fontSize: 12, height: 32 }}
                    title="Выбрать одну наилучшую копию по качеству и размеру"
                  >
                    <Sparkles size={13} color="var(--accent-hover)" />
                    <span>Лучшая копия</span>
                  </button>
                  <button
                    className="btn btn-secondary"
                    onClick={() => handleSelectAllCopies(activeCluster.id)}
                    style={{ fontSize: 12, height: 32 }}
                  >
                    <span>Выбрать все ({activeCluster.copies.length})</span>
                  </button>
                </div>
              </div>
              <p style={{ marginTop: 2, color: "var(--text-secondary)" }}>
                {activeCluster.authors || "Автор не указан"} · {activeCluster.copies.length} копий на {new Set(activeCluster.copies.map(c => c.volume_name)).size} накопителях
              </p>
            </div>

            <div className="evidence">
              <strong>
                {Math.round(activeCluster.maxScore * 100)}% сходства · {duplicateVerdict(activeCluster.verdict)}
              </strong>
              <p style={{ marginTop: 6 }}>{duplicateEvidence(duplicates.find(p => p.id === activeCluster.pairIds[0]) || duplicates[0])}</p>
            </div>

            {/* Copies Grid (Supports 2, 3, 4, 5+ copies across all disks) */}
            <div
              style={{
                display: "grid",
                gridTemplateColumns: `repeat(${Math.min(activeCluster.copies.length, 3)}, minmax(260px, 1fr))`,
                gap: 12,
                overflowX: "auto",
                padding: "4px 0",
              }}
            >
              {activeCluster.copies.map((copy, index) => {
                const isKept = activeKeptIds.has(copy.copy_id);
                return (
                  <div
                    key={copy.copy_id}
                    style={{
                      background: isKept ? "var(--bg-surface)" : "rgba(239, 68, 68, 0.04)",
                      border: isKept ? "1.5px solid var(--accent)" : "1.5px dashed var(--border-card)",
                      borderRadius: 10,
                      padding: "14px 16px",
                      display: "flex",
                      flexDirection: "column",
                      gap: 10,
                      position: "relative",
                      transition: "all 150ms ease",
                    }}
                  >
                    <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}>
                      <label
                        style={{
                          display: "flex",
                          alignItems: "center",
                          gap: 8,
                          cursor: "pointer",
                          fontWeight: 700,
                          fontSize: 13.5,
                          color: isKept ? "var(--text-primary)" : "var(--text-muted)",
                        }}
                      >
                        <input
                          type="checkbox"
                          checked={isKept}
                          onChange={() => toggleKeepCopy(activeCluster.id, copy.copy_id)}
                          style={{ width: 16, height: 16 }}
                        />
                        <span>{isKept ? "Сохранить копию" : "Будет удалена"}</span>
                      </label>

                      <span
                        className="badge"
                        style={{
                          background: isKept ? "var(--accent-gradient)" : "rgba(255,255,255,0.08)",
                          color: isKept ? "#ffffff" : "var(--text-muted)",
                          fontSize: 11,
                          padding: "2px 7px",
                        }}
                      >
                        Вариант {String.fromCharCode(65 + index)}
                      </span>
                    </div>

                    <div
                      style={{
                        padding: "8px 10px",
                        background: "var(--bg-card)",
                        borderRadius: 6,
                        display: "flex",
                        alignItems: "center",
                        justifyContent: "space-between",
                      }}
                    >
                      <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
                        <HardDrive size={14} color={copy.online ? "#86efac" : "#fca5a5"} />
                        <strong style={{ fontSize: 12.5 }}>{copy.volume_name}</strong>
                      </div>
                      <small style={{ color: copy.online ? "#86efac" : "#fca5a5", fontWeight: 650, fontSize: 11 }}>
                        {copy.online ? "Онлайн" : "Отключён"}
                      </small>
                    </div>

                    <dl
                      style={{
                        margin: 0,
                        fontSize: 12,
                        display: "grid",
                        gridTemplateColumns: "85px 1fr",
                        gap: "6px 8px",
                      }}
                    >
                      <dt style={{ color: "var(--text-muted)" }}>Чтец:</dt>
                      <dd style={{ margin: 0, fontWeight: 600 }}>{copy.narrator || "Не указан"}</dd>

                      <dt style={{ color: "var(--text-muted)" }}>Формат:</dt>
                      <dd style={{ margin: 0 }}>
                        <span className="badge badge-format" style={{ fontSize: 10, padding: "1px 5px" }}>
                          {copy.format}
                        </span>
                      </dd>

                      <dt style={{ color: "var(--text-muted)" }}>Размер:</dt>
                      <dd style={{ margin: 0, fontWeight: 600 }}>{formatBytes(copy.size)}</dd>

                      <dt style={{ color: "var(--text-muted)" }}>Время:</dt>
                      <dd style={{ margin: 0 }}>{copy.duration_ms ? formatDuration(copy.duration_ms) : "—"}</dd>

                      <dt style={{ color: "var(--text-muted)" }}>Папка:</dt>
                      <dd
                        style={{
                          margin: 0,
                          fontSize: 11,
                          color: "var(--text-muted)",
                          fontFamily: "monospace",
                          wordBreak: "break-all",
                        }}
                        title={copy.rel_path}
                      >
                        {copy.rel_path}
                      </dd>
                    </dl>
                  </div>
                );
              })}
            </div>

            {/* Bottom Decision Actions */}
            <div
              style={{
                display: "flex",
                alignItems: "center",
                justifyContent: "space-between",
                gap: 12,
                flexWrap: "wrap",
                marginTop: 8,
                paddingTop: 12,
                borderTop: "1px solid var(--border-subtle)",
              }}
            >
              <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                <button
                  disabled={busy}
                  className="btn btn-primary"
                  onClick={handleApplyDecision}
                  style={{ gap: 6 }}
                >
                  <CheckCircle2 size={15} />
                  <span>
                    Применить: сохранить {activeKeptIds.size} из {activeCluster.copies.length}
                  </span>
                </button>

                <button
                  disabled={busy}
                  className="btn btn-secondary"
                  onClick={async () => {
                    setBusy(true);
                    try {
                      for (const pid of activeCluster.pairIds) {
                        await onResolve(pid, "keep_both");
                      }
                    } finally {
                      setBusy(false);
                    }
                  }}
                >
                  Оставить все копии
                </button>
              </div>

              <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                <button
                  disabled={busy}
                  className="btn btn-quiet"
                  onClick={async () => {
                    setBusy(true);
                    try {
                      for (const pid of activeCluster.pairIds) {
                        await onResolve(pid, "mark_different");
                      }
                    } finally {
                      setBusy(false);
                    }
                  }}
                >
                  Разные книги
                </button>
                <button
                  disabled={busy}
                  className="btn btn-quiet"
                  onClick={async () => {
                    setBusy(true);
                    try {
                      for (const pid of activeCluster.pairIds) {
                        await onResolve(pid, "ignore");
                      }
                    } finally {
                      setBusy(false);
                    }
                  }}
                >
                  Игнорировать
                </button>
              </div>
            </div>

            <p
              className="muted"
              style={{ fontSize: 11, display: "flex", alignItems: "center", gap: 7, margin: "2px 0 0" }}
            >
              <ShieldCheck size={14} />
              Сохранённые копии остаются в каталоге. Если накопитель не подключён, задача удаления выполнится при его появлении в сети.
            </p>
          </div>
        </div>
      )}

      {/* Deletion Confirm Modal for Unselected Copies */}
      <DeletionConfirmModal
        isOpen={deleteModalOpen}
        items={itemsPendingDeletion}
        busy={busy}
        onConfirm={handleConfirmDeletion}
        onCancel={() => setDeleteModalOpen(false)}
      />
    </div>
  );
}
