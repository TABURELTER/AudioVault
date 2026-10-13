import { useMemo, useState } from "react";
import {
  ScanSearch,
  ShieldCheck,
  ShieldAlert,
  HardDrive,
  CheckCircle2,
  Sparkles,
  Layers,
  Headphones,
  BookOpen,
  ArrowUpDown,
  Filter,
} from "lucide-react";
import type { DuplicatePair, DuplicateCopySide } from "../api/types";
import { api, formatBytes, formatDuration } from "../api/client";
import { duplicateEvidence, duplicateVerdict } from "../api/presentation";
import { EmptyState, SearchField, getFormatClass } from "../components/Primitives";
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
  duplicateKind: "audio" | "ebook" | "cross_media";
  formatPair: string;
  formatsList: string[];
  isProtected: boolean;
  protectionReason: string;
  totalSize: number;
  potentialSavings: number;
  maxDurationMs: number;
}

type FilterMode =
  | "all"
  | "audio"
  | "ebook"
  | "fb2_pdf"
  | "fb2_epub"
  | "fb2_fb2"
  | "cross_media"
  | string;

type SortMode =
  | "score_desc"
  | "savings_desc"
  | "size_desc"
  | "title_asc"
  | "author_asc"
  | "format_asc"
  | "duration_desc"
  | "copies_desc"
  | "volume_asc";

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
  const [filterMode, setFilterMode] = useState<FilterMode>("all");
  const [sortMode, setSortMode] = useState<SortMode>("score_desc");
  const [searchQuery, setSearchQuery] = useState("");

  // Cluster duplicate pairs within the same duplicate_kind (so cross_media pairs don't pollute pure audio/ebook clusters)
  const allClusters = useMemo(() => {
    if (!duplicates.length) return [];

    // Key by `${kind}:${copy_id}` so audio+audio, ebook+ebook, and cross_media stay cleanly separated
    const copyMap = new Map<string, ClusterCopy>();
    const pairGroups: Array<{ pair: DuplicatePair; keyA: string; keyB: string }> = [];

    for (const p of duplicates) {
      const kind = p.duplicate_kind || "audio";
      const keyA = `${kind}:${p.a.copy_id}`;
      const keyB = `${kind}:${p.b.copy_id}`;

      if (!copyMap.has(keyA)) {
        copyMap.set(keyA, { ...p.a, pairIds: [p.id] });
      } else {
        copyMap.get(keyA)!.pairIds.push(p.id);
      }

      if (!copyMap.has(keyB)) {
        copyMap.set(keyB, { ...p.b, pairIds: [p.id] });
      } else {
        copyMap.get(keyB)!.pairIds.push(p.id);
      }

      pairGroups.push({ pair: p, keyA, keyB });
    }

    const parent = new Map<string, string>();
    const find = (k: string): string => {
      if (!parent.has(k)) parent.set(k, k);
      if (parent.get(k) !== k) {
        parent.set(k, find(parent.get(k)!));
      }
      return parent.get(k)!;
    };

    const union = (a: string, b: string) => {
      const rootA = find(a);
      const rootB = find(b);
      if (rootA !== rootB) {
        parent.set(rootA, rootB);
      }
    };

    for (const pg of pairGroups) {
      union(pg.keyA, pg.keyB);
    }

    const clusterBuckets = new Map<string, DuplicatePair[]>();
    for (const pg of pairGroups) {
      const root = find(pg.keyA);
      if (!clusterBuckets.has(root)) {
        clusterBuckets.set(root, []);
      }
      clusterBuckets.get(root)!.push(pg.pair);
    }

    const result: DuplicateCluster[] = [];
    let idx = 0;
    for (const [rootKey, pairs] of clusterBuckets.entries()) {
      idx++;
      const kind = (pairs[0].duplicate_kind || "audio") as "audio" | "ebook" | "cross_media";
      const uniqueCopyKeys = new Set<string>();
      const pairIds = pairs.map((p) => p.id);
      let maxScore = 0;
      let verdict = pairs[0].verdict;
      let evidence = pairs[0].evidence;
      let title = pairs[0].title;
      let authors = pairs[0].authors;
      let isProtected = false;
      let protectionReason = "";

      for (const p of pairs) {
        uniqueCopyKeys.add(`${kind}:${p.a.copy_id}`);
        uniqueCopyKeys.add(`${kind}:${p.b.copy_id}`);
        if (p.score > maxScore) {
          maxScore = p.score;
          verdict = p.verdict;
          evidence = p.evidence;
          title = p.title;
          authors = p.authors;
        }
        if (p.is_protected) {
          isProtected = true;
          if (p.protection_reason) protectionReason = p.protection_reason;
        }
      }

      const copiesList: ClusterCopy[] = Array.from(uniqueCopyKeys)
        .map((ck) => copyMap.get(ck)!)
        .filter(Boolean);

      // Additional safety check: if cluster has both an audio copy and an ebook/FB2 copy, protect it!
      const hasAudio = copiesList.some((c) => (c.item_type || "audio") === "audio");
      const hasEbookOrFb2 = copiesList.some(
        (c) =>
          c.item_type === "ebook" ||
          c.format.toLowerCase().includes("fb2"),
      );
      if (hasAudio && hasEbookOrFb2) {
        isProtected = true;
        if (!protectionReason) {
          protectionReason =
            "Дубликат «Аудиокнига + FB2 (эл. книга)» защищён от удаления: аудиозапись и текстовая версия не заменяют друг друга.";
        }
      }

      const formatsList = Array.from(
        new Set(
          copiesList.map((c) =>
            c.format.toUpperCase().replace("FB2.ZIP", "FB2"),
          ),
        ),
      ).sort();

      let formatPair = pairs[0].format_pair || formatsList.join(" + ");
      if (!pairs[0].format_pair) {
        if (formatsList.length === 1) {
          formatPair = `${formatsList[0]} + ${formatsList[0]}`;
        } else {
          formatPair = formatsList.join(" + ");
        }
      }

      const totalSize = copiesList.reduce((acc, c) => acc + (c.size || 0), 0);
      const maxCopySize = copiesList.reduce((acc, c) => Math.max(acc, c.size || 0), 0);
      const potentialSavings = Math.max(0, totalSize - maxCopySize);
      const maxDurationMs = copiesList.reduce(
        (acc, c) => Math.max(acc, c.duration_ms || 0),
        0,
      );

      result.push({
        id: `cluster-${idx}-${rootKey}`,
        title,
        authors,
        maxScore,
        verdict,
        evidence,
        pairIds,
        copies: copiesList,
        duplicateKind: hasAudio && hasEbookOrFb2 ? "cross_media" : kind,
        formatPair,
        formatsList,
        isProtected,
        protectionReason,
        totalSize,
        potentialSavings,
        maxDurationMs,
      });
    }

    return result;
  }, [duplicates]);

  // Compute filter counts and dynamic format combinations
  const filterStats = useMemo(() => {
    let audioCount = 0;
    let ebookCount = 0;
    let fb2PdfCount = 0;
    let fb2EpubCount = 0;
    let fb2Fb2Count = 0;
    let crossMediaCount = 0;
    const customCombos = new Map<string, number>();

    for (const c of allClusters) {
      if (c.duplicateKind === "audio") audioCount++;
      if (c.duplicateKind === "ebook") ebookCount++;
      if (c.duplicateKind === "cross_media" || c.isProtected) crossMediaCount++;

      const normPair = c.formatPair.toUpperCase();
      if (
        c.duplicateKind === "ebook" &&
        normPair.includes("FB2") &&
        normPair.includes("PDF")
      ) {
        fb2PdfCount++;
      } else if (
        c.duplicateKind === "ebook" &&
        normPair.includes("FB2") &&
        normPair.includes("EPUB")
      ) {
        fb2EpubCount++;
      } else if (c.duplicateKind === "ebook" && normPair === "FB2 + FB2") {
        fb2Fb2Count++;
      } else if (c.duplicateKind !== "cross_media") {
        customCombos.set(c.formatPair, (customCombos.get(c.formatPair) || 0) + 1);
      }
    }

    return {
      all: allClusters.length,
      audio: audioCount,
      ebook: ebookCount,
      fb2_pdf: fb2PdfCount,
      fb2_epub: fb2EpubCount,
      fb2_fb2: fb2Fb2Count,
      cross_media: crossMediaCount,
      customCombos: Array.from(customCombos.entries()).sort((a, b) => b[1] - a[1]),
    };
  }, [allClusters]);

  // Filter and sort clusters
  const clusters = useMemo(() => {
    const q = searchQuery.trim().toLowerCase();
    const filtered = allClusters.filter((c) => {
      if (q) {
        const hay = `${c.title} ${c.authors} ${c.formatPair} ${c.copies
          .map((cp) => `${cp.volume_name} ${cp.narrator} ${cp.rel_path}`)
          .join(" ")}`.toLowerCase();
        if (!hay.includes(q)) return false;
      }

      const normPair = c.formatPair.toUpperCase();
      switch (filterMode) {
        case "all":
          return true;
        case "audio":
          return c.duplicateKind === "audio";
        case "ebook":
          return c.duplicateKind === "ebook";
        case "fb2_pdf":
          return (
            c.duplicateKind === "ebook" &&
            normPair.includes("FB2") &&
            normPair.includes("PDF")
          );
        case "fb2_epub":
          return (
            c.duplicateKind === "ebook" &&
            normPair.includes("FB2") &&
            normPair.includes("EPUB")
          );
        case "fb2_fb2":
          return c.duplicateKind === "ebook" && normPair === "FB2 + FB2";
        case "cross_media":
          return c.duplicateKind === "cross_media" || c.isProtected;
        default:
          return c.formatPair === filterMode;
      }
    });

    filtered.sort((a, b) => {
      switch (sortMode) {
        case "score_desc":
          return b.maxScore - a.maxScore || b.potentialSavings - a.potentialSavings;
        case "savings_desc":
          return b.potentialSavings - a.potentialSavings;
        case "size_desc":
          return b.totalSize - a.totalSize;
        case "title_asc":
          return (a.title || "").localeCompare(b.title || "", "ru");
        case "author_asc":
          return (a.authors || "").localeCompare(b.authors || "", "ru");
        case "format_asc":
          return a.formatPair.localeCompare(b.formatPair, "ru");
        case "duration_desc":
          return b.maxDurationMs - a.maxDurationMs;
        case "copies_desc":
          return b.copies.length - a.copies.length || b.potentialSavings - a.potentialSavings;
        case "volume_asc": {
          const volA = a.copies[0]?.volume_name || "";
          const volB = b.copies[0]?.volume_name || "";
          return volA.localeCompare(volB, "ru");
        }
        default:
          return 0;
      }
    });

    return filtered;
  }, [allClusters, filterMode, sortMode, searchQuery]);

  const activeCluster =
    clusters.find((c) => c.id === selectedClusterId) || clusters[0];

  // Selected copies to KEEP for the active cluster
  const activeKeptIds = useMemo(() => {
    if (!activeCluster) return new Set<number>();
    // Protected clusters ALWAYS keep all copies!
    if (activeCluster.isProtected) {
      return new Set(activeCluster.copies.map((c) => c.copy_id));
    }
    if (savedCopyIds[activeCluster.id]) {
      return savedCopyIds[activeCluster.id];
    }
    // Default: keep the highest-quality copy (for ebooks: prefer FB2 over EPUB over PDF; for audio: online + largest size)
    const bestCopy = [...activeCluster.copies].sort((x, y) => {
      if (x.online !== y.online) return x.online ? -1 : 1;
      if (activeCluster.duplicateKind === "ebook") {
        const rank = (fmt: string) => {
          const f = fmt.toLowerCase();
          if (f === "fb2" || f === "fb2.zip") return 3;
          if (f === "epub") return 2;
          return 1;
        };
        const rDiff = rank(y.format) - rank(x.format);
        if (rDiff !== 0) return rDiff;
      }
      return y.size - x.size;
    })[0];
    return new Set(bestCopy ? [bestCopy.copy_id] : []);
  }, [activeCluster, savedCopyIds]);

  const toggleKeepCopy = (clusterId: string, copyId: number) => {
    if (activeCluster?.isProtected) return;
    setSavedCopyIds((prev) => {
      const current = new Set(prev[clusterId] || activeKeptIds);
      if (current.has(copyId)) {
        // Prevent unchecking ALL copies so the user doesn't accidentally delete the last copy
        if (current.size <= 1) return prev;
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
    if (!activeCluster || activeCluster.isProtected) return;
    const bestCopy = [...activeCluster.copies].sort((x, y) => {
      if (x.online !== y.online) return x.online ? -1 : 1;
      if (activeCluster.duplicateKind === "ebook") {
        const rank = (fmt: string) => {
          const f = fmt.toLowerCase();
          if (f === "fb2" || f === "fb2.zip") return 3;
          if (f === "epub") return 2;
          return 1;
        };
        const rDiff = rank(y.format) - rank(x.format);
        if (rDiff !== 0) return rDiff;
      }
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

    // Safety guard: Protected scenarios (Audio + FB2) can NEVER delete anything
    if (activeCluster.isProtected) {
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

    const unselectedCopies = activeCluster.copies.filter(
      (c) => !activeKeptIds.has(c.copy_id),
    );

    if (unselectedCopies.length === 0) {
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

    const toDelete: ItemToDelete[] = unselectedCopies.map((c) => ({
      id: c.copy_id,
      type: c.item_type === "ebook" ? ("ebook" as const) : ("book" as const),
      title: `${activeCluster.title} (${c.format.toUpperCase()} · ${c.volume_name})`,
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
    if (!activeCluster || activeCluster.isProtected) return;
    setBusy(true);
    try {
      const unselectedCopies = activeCluster.copies.filter(
        (c) => !activeKeptIds.has(c.copy_id),
      );

      const audioCopyIdsToDelete = unselectedCopies
        .filter((c) => (c.item_type || "audio") === "audio" && c.copy_id > 0)
        .map((c) => c.copy_id);

      const ebookIdsToDelete = unselectedCopies
        .filter((c) => c.item_type === "ebook")
        .map((c) => Math.abs(c.copy_id));

      if (audioCopyIdsToDelete.length > 0 || ebookIdsToDelete.length > 0) {
        await api.executeCopyDeletion(audioCopyIdsToDelete, ebookIdsToDelete);
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
            {allClusters.length} групп совпадений (показано: {clusters.length})
          </strong>
          <br />
          Сравнивайте аудиофайлы, электронные книги (FB2, PDF, EPUB) и кросс-форматные пары. Сценарии «Аудиокнига + FB2» автоматически защищены от удаления.
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

      {/* Rich Filter & Sorting Controls Bar */}
      <div className="duplicates-controls-bar">
        <div
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            gap: 12,
            flexWrap: "wrap",
          }}
        >
          <div style={{ width: "min(340px, 100%)" }}>
            <SearchField
              value={searchQuery}
              onChange={setSearchQuery}
              placeholder="Фильтр по названию, автору, формату или диску..."
            />
          </div>

          <div style={{ display: "flex", alignItems: "center", gap: 8, flexWrap: "wrap" }}>
            <ArrowUpDown size={15} color="var(--text-muted)" />
            <span style={{ fontSize: 12.5, color: "var(--text-secondary)", fontWeight: 600 }}>
              Сортировка:
            </span>
            <select
              aria-label="Сортировка дубликатов"
              value={sortMode}
              onChange={(e) => setSortMode(e.target.value as SortMode)}
              style={{ minWidth: 230 }}
            >
              <option value="score_desc">По сходству (высокое → низкое)</option>
              <option value="savings_desc">По экономии места (размер лишних копий)</option>
              <option value="size_desc">По общему размеру группы</option>
              <option value="title_asc">По названию (А → Я)</option>
              <option value="author_asc">По автору (А → Я)</option>
              <option value="format_asc">По комбинации форматов (FB2, MP3, PDF…)</option>
              <option value="duration_desc">По длительности аудиозаписи</option>
              <option value="copies_desc">По количеству копий</option>
              <option value="volume_asc">По накопителю</option>
            </select>
          </div>
        </div>

        <div className="duplicates-filter-chips" role="tablist" aria-label="Фильтр типов дубликатов">
          <span
            style={{
              display: "inline-flex",
              alignItems: "center",
              gap: 5,
              fontSize: 12,
              color: "var(--text-muted)",
              marginRight: 4,
              fontWeight: 600,
            }}
          >
            <Filter size={13} /> Тип:
          </span>

          <button
            type="button"
            className={`dup-filter-chip ${filterMode === "all" ? "active" : ""}`}
            onClick={() => setFilterMode("all")}
          >
            <span>Все</span>
            <span className="chip-badge">{filterStats.all}</span>
          </button>

          <button
            type="button"
            className={`dup-filter-chip ${filterMode === "audio" ? "active" : ""}`}
            onClick={() => setFilterMode("audio")}
          >
            <Headphones size={13} />
            <span>Только аудиофайлы</span>
            <span className="chip-badge">{filterStats.audio}</span>
          </button>

          <button
            type="button"
            className={`dup-filter-chip ${filterMode === "ebook" ? "active" : ""}`}
            onClick={() => setFilterMode("ebook")}
          >
            <BookOpen size={13} />
            <span>Только эл. книги</span>
            <span className="chip-badge">{filterStats.ebook}</span>
          </button>

          <button
            type="button"
            className={`dup-filter-chip ${filterMode === "fb2_pdf" ? "active" : ""}`}
            onClick={() => setFilterMode("fb2_pdf")}
          >
            <span>FB2 + PDF</span>
            <span className="chip-badge">{filterStats.fb2_pdf}</span>
          </button>

          <button
            type="button"
            className={`dup-filter-chip ${filterMode === "fb2_epub" ? "active" : ""}`}
            onClick={() => setFilterMode("fb2_epub")}
          >
            <span>FB2 + EPUB</span>
            <span className="chip-badge">{filterStats.fb2_epub}</span>
          </button>

          <button
            type="button"
            className={`dup-filter-chip ${filterMode === "fb2_fb2" ? "active" : ""}`}
            onClick={() => setFilterMode("fb2_fb2")}
          >
            <span>FB2 + FB2</span>
            <span className="chip-badge">{filterStats.fb2_fb2}</span>
          </button>

          {filterStats.customCombos.map(([combo, count]) => (
            <button
              key={combo}
              type="button"
              className={`dup-filter-chip ${filterMode === combo ? "active" : ""}`}
              onClick={() => setFilterMode(combo)}
            >
              <span>{combo}</span>
              <span className="chip-badge">{count}</span>
            </button>
          ))}

          <button
            type="button"
            className={`dup-filter-chip protected-chip ${
              filterMode === "cross_media" ? "active" : ""
            }`}
            onClick={() => setFilterMode("cross_media")}
            title="Аудиокниги, у которых также есть текстовая версия FB2 (защищены от удаления)"
          >
            <ShieldCheck size={13} />
            <span>Аудио + FB2 (защищено)</span>
            <span className="chip-badge">{filterStats.cross_media}</span>
          </button>
        </div>
      </div>

      {!activeCluster ? (
        <EmptyState
          title={
            allClusters.length > 0
              ? "Нет дубликатов по выбранному фильтру"
              : "Нет дубликатов для проверки"
          }
          description={
            allClusters.length > 0
              ? "Попробуйте переключить фильтр категорий («Все», «Только аудиофайлы», «FB2 + PDF») или очистить строку поиска."
              : "Запустите анализ коллекции, чтобы найти совпадающие аудиокниги и электронные книги на разных дисках."
          }
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
                  <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: 6 }}>
                    <strong title={c.title} style={{ flex: 1, minWidth: 0 }}>
                      {c.title}
                    </strong>
                    <span
                      className="badge"
                      style={{
                        fontSize: 10,
                        padding: "1px 6px",
                        background: c.isProtected
                          ? "rgba(245, 158, 11, 0.16)"
                          : c.duplicateKind === "ebook"
                          ? "rgba(16, 185, 129, 0.16)"
                          : "rgba(99, 102, 241, 0.16)",
                        color: c.isProtected
                          ? "#fbbf24"
                          : c.duplicateKind === "ebook"
                          ? "#34d399"
                          : "#a5b4fc",
                        flexShrink: 0,
                      }}
                    >
                      {c.formatPair}
                    </span>
                  </div>
                  <small>{c.authors || "Автор не указан"}</small>
                  <small
                    style={{
                      display: "flex",
                      alignItems: "center",
                      justifyContent: "space-between",
                      gap: 5,
                      color: c.isProtected ? "#fbbf24" : "var(--accent-hover)",
                    }}
                  >
                    <span style={{ display: "inline-flex", alignItems: "center", gap: 4 }}>
                      {c.isProtected ? <ShieldCheck size={11} /> : <Layers size={11} />}
                      {c.copies.length} коп. ({diskNames})
                    </span>
                    {!c.isProtected && c.potentialSavings > 0 && (
                      <span style={{ color: "var(--text-muted)", fontSize: 11 }}>
                        −{formatBytes(c.potentialSavings)}
                      </span>
                    )}
                  </small>
                </button>
              );
            })}
          </div>

          {/* Right Workspace: Multi-Drive Comparison Cards */}
          <div className="duplicate-review" style={{ display: "flex", flexDirection: "column", gap: 14 }}>
            <div>
              <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", flexWrap: "wrap", gap: 10 }}>
                <div style={{ display: "flex", alignItems: "center", gap: 10, flexWrap: "wrap" }}>
                  <h2>{activeCluster.title}</h2>
                  <span className="badge" style={{ fontSize: 11.5, padding: "3px 8px" }}>
                    {activeCluster.formatPair}
                  </span>
                </div>
                {!activeCluster.isProtected && (
                  <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                    <button
                      className="btn btn-secondary"
                      onClick={() => handleSelectBestQualityOnly(activeCluster.id)}
                      style={{ fontSize: 12, height: 32 }}
                      title="Выбрать одну наилучшую копию по формату и размеру"
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
                )}
              </div>
              <p style={{ marginTop: 2, color: "var(--text-secondary)" }}>
                {activeCluster.authors || "Автор не указан"} · {activeCluster.copies.length} копий на{" "}
                {new Set(activeCluster.copies.map((c) => c.volume_name)).size} накопителях
                {!activeCluster.isProtected && activeCluster.potentialSavings > 0 && (
                  <span style={{ color: "#86efac", marginLeft: 8, fontWeight: 600 }}>
                    · Можно освободить {formatBytes(activeCluster.potentialSavings)}
                  </span>
                )}
              </p>
            </div>

            {/* Protected Scenario Banner (Audio + FB2) */}
            {activeCluster.isProtected && (
              <div className="protected-scenario-banner" role="status">
                <ShieldAlert size={20} style={{ flexShrink: 0, marginTop: 1 }} />
                <div>
                  <strong style={{ display: "block", marginBottom: 2 }}>
                    Защита сценария «Аудиокнига + FB2»: удаление заблокировано
                  </strong>
                  <span>
                    {activeCluster.protectionReason ||
                      "Аудиокнига и электронная книга в формате FB2 дополняют друг друга и не являются избыточным дубликатом. Удаление в этом сценарии отключено."}
                  </span>
                </div>
              </div>
            )}

            <div className="evidence">
              <strong>
                {Math.round(activeCluster.maxScore * 100)}% сходства ·{" "}
                {duplicateVerdict(activeCluster.verdict)}
              </strong>
              <p style={{ marginTop: 6 }}>
                {duplicateEvidence(
                  duplicates.find((p) => p.id === activeCluster.pairIds[0]) || duplicates[0],
                )}
              </p>
            </div>

            {/* Copies Grid */}
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
                const isEbook = copy.item_type === "ebook";
                return (
                  <div
                    key={`${copy.item_type || "audio"}-${copy.copy_id}`}
                    style={{
                      background: isKept ? "var(--bg-surface)" : "rgba(239, 68, 68, 0.04)",
                      border: isKept
                        ? "1.5px solid var(--accent)"
                        : "1.5px dashed var(--border-card)",
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
                          gap: 9,
                          cursor: activeCluster.isProtected ? "not-allowed" : "pointer",
                          fontWeight: 700,
                          fontSize: 13.5,
                          color: isKept ? "var(--text-primary)" : "var(--text-muted)",
                        }}
                      >
                        <input
                          type="checkbox"
                          checked={isKept}
                          disabled={activeCluster.isProtected}
                          onChange={() => toggleKeepCopy(activeCluster.id, copy.copy_id)}
                        />
                        <span>
                          {activeCluster.isProtected
                            ? "Защищена от удаления"
                            : isKept
                            ? "Сохранить копию"
                            : "Будет удалена"}
                        </span>
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
                      <small
                        style={{
                          color: copy.online ? "#86efac" : "#fca5a5",
                          fontWeight: 650,
                          fontSize: 11,
                        }}
                      >
                        {copy.online ? "Онлайн" : "Отключён"}
                      </small>
                    </div>

                    <dl
                      style={{
                        margin: 0,
                        fontSize: 12,
                        display: "grid",
                        gridTemplateColumns: "90px 1fr",
                        gap: "6px 8px",
                      }}
                    >
                      <dt style={{ color: "var(--text-muted)" }}>Тип:</dt>
                      <dd style={{ margin: 0, fontWeight: 600 }}>
                        {isEbook ? "Электронная книга" : "Аудиокнига"}
                        {copy.has_fb2 && !isEbook && (
                          <span
                            style={{
                              marginLeft: 6,
                              fontSize: 10,
                              padding: "1px 5px",
                              borderRadius: 4,
                              background: "rgba(16, 185, 129, 0.16)",
                              color: "#34d399",
                            }}
                            title="При удалении аудиопапки вложенный файл .fb2 будет автоматически сохранён"
                          >
                            + FB2 внутри (сохранится)
                          </span>
                        )}
                      </dd>

                      {!isEbook && (
                        <>
                          <dt style={{ color: "var(--text-muted)" }}>Чтец:</dt>
                          <dd style={{ margin: 0, fontWeight: 600 }}>
                            {copy.narrator || "Не указан"}
                          </dd>
                        </>
                      )}

                      <dt style={{ color: "var(--text-muted)" }}>Формат:</dt>
                      <dd style={{ margin: 0, display: "flex", alignItems: "center", gap: 6 }}>
                        <span
                          className={`badge badge-format ${getFormatClass(copy.format)}`}
                          style={{ fontSize: 10.5, padding: "1px 6px" }}
                        >
                          {copy.format.toUpperCase()}
                        </span>
                        {copy.bitrate_kbps && copy.bitrate_kbps > 0 ? (
                          <span style={{ color: "var(--text-secondary)", fontSize: 11 }}>
                            {copy.bitrate_kbps} кбит/с
                          </span>
                        ) : null}
                      </dd>

                      <dt style={{ color: "var(--text-muted)" }}>Размер:</dt>
                      <dd style={{ margin: 0, fontWeight: 600 }}>{formatBytes(copy.size)}</dd>

                      {!isEbook && (
                        <>
                          <dt style={{ color: "var(--text-muted)" }}>Время:</dt>
                          <dd style={{ margin: 0 }}>
                            {copy.duration_ms ? formatDuration(copy.duration_ms) : "—"}
                          </dd>
                        </>
                      )}

                      <dt style={{ color: "var(--text-muted)" }}>Путь:</dt>
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
                {!activeCluster.isProtected ? (
                  <>
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
                  </>
                ) : (
                  <button
                    disabled={busy}
                    className="btn btn-primary"
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
                    style={{ gap: 6 }}
                  >
                    <ShieldCheck size={15} />
                    <span>Подтвердить: оставить аудиокнигу и FB2</span>
                  </button>
                )}
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
              style={{
                fontSize: 11,
                display: "flex",
                alignItems: "center",
                gap: 7,
                margin: "2px 0 0",
              }}
            >
              <ShieldCheck size={14} />
              Сохранённые копии остаются в каталоге. Вложенные файлы .fb2 в папках с аудиокнигами никогда не удаляются.
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
