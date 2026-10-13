import React, { useEffect, useState } from "react";
import {
  Download,
  ExternalLink,
  Sparkles,
  X,
  CheckCircle2,
  AlertCircle,
} from "lucide-react";
import { listen } from "@tauri-apps/api/event";
import type { UpdateCheckResult } from "../api/types";
import { api } from "../api/client";

interface Props {
  updateInfo: UpdateCheckResult | null;
  onClose: () => void;
}

interface UpdateProgressPayload {
  downloaded: number;
  total: number | null;
  percent: number;
}

function formatMb(bytes: number): string {
  return (bytes / (1024 * 1024)).toFixed(1) + " МБ";
}

export const UpdateModal: React.FC<Props> = ({ updateInfo, onClose }) => {
  const [installing, setInstalling] = useState(false);
  const [progress, setProgress] = useState<UpdateProgressPayload | null>(null);
  const [installedMsg, setInstalledMsg] = useState("");
  const [errorMsg, setErrorMsg] = useState("");

  useEffect(() => {
    if (!installing) return;
    let unlisten: (() => void) | undefined;
    let active = true;

    listen<UpdateProgressPayload>("update://progress", (event) => {
      if (active) {
        setProgress(event.payload);
      }
    })
      .then((fn) => {
        if (active) unlisten = fn;
        else fn();
      })
      .catch(() => {});

    return () => {
      active = false;
      if (unlisten) unlisten();
    };
  }, [installing]);

  if (!updateInfo || !updateInfo.update_available) return null;

  const handleInstallNow = async () => {
    setInstalling(true);
    setProgress({ downloaded: 0, total: updateInfo.asset_size ?? null, percent: 0 });
    setErrorMsg("");
    try {
      const targetUrl = updateInfo.download_url || updateInfo.html_url;
      if (
        updateInfo.download_url &&
        /\.(msi|exe|dmg|appimage|deb)$/i.test(updateInfo.asset_name || updateInfo.download_url)
      ) {
        const savedPath = await api.downloadAndInstallUpdate(
          updateInfo.download_url,
          updateInfo.asset_name || "AudioVault-Setup.exe",
        );
        setInstalledMsg(
          `Установщик скачан (${savedPath}) и запущен. Приложение сейчас закроется для завершения обновления и создания ярлыка на рабочем столе.`,
        );
      } else {
        await api.openExternalUrl(targetUrl);
        onClose();
      }
    } catch (e) {
      console.error("Update install error:", e);
      setErrorMsg(
        "Не удалось автоматически скачать установщик. Вы можете открыть страницу релиза на GitHub и скачать обновление вручную.",
      );
    } finally {
      setInstalling(false);
    }
  };

  const handleOpenReleasePage = async () => {
    try {
      await api.openExternalUrl(updateInfo.html_url);
    } catch {
      window.open(updateInfo.html_url, "_blank", "noopener,noreferrer");
    }
  };

  const handleSkipVersion = async () => {
    try {
      await api.skipUpdateVersion(updateInfo.latest_version);
    } catch (e) {
      console.error("skipUpdateVersion error:", e);
    }
    onClose();
  };

  const formattedDate = updateInfo.published_at
    ? new Date(updateInfo.published_at).toLocaleDateString("ru-RU", {
        day: "numeric",
        month: "long",
        year: "numeric",
      })
    : null;

  return (
    <div
      className="modal-backdrop"
      onClick={() => {
        if (!installing) onClose();
      }}
      role="dialog"
      aria-modal="true"
      aria-labelledby="update-modal-title"
    >
      <div
        className="modal update-modal"
        onClick={(e) => e.stopPropagation()}
      >
        <div
          style={{
            display: "flex",
            alignItems: "flex-start",
            justifyContent: "space-between",
            gap: 12,
          }}
        >
          <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
            <div
              style={{
                width: 42,
                height: 42,
                borderRadius: 12,
                background: "var(--accent-gradient)",
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                color: "#fff",
                flexShrink: 0,
              }}
            >
              <Sparkles size={22} />
            </div>
            <div>
              <h3 id="update-modal-title" style={{ margin: 0, fontSize: 17 }}>
                Доступно обновление AudioVault
              </h3>
              <p style={{ margin: "3px 0 0", fontSize: 13 }}>
                Хотите автоматически скачать и установить новую версию с GitHub?
              </p>
            </div>
          </div>
          <button
            type="button"
            className="plain-icon"
            onClick={onClose}
            disabled={installing}
            aria-label="Закрыть"
          >
            <X size={18} />
          </button>
        </div>

        <div
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            gap: 10,
            padding: "10px 14px",
            borderRadius: 8,
            background: "var(--bg-surface)",
            border: "1px solid var(--border-subtle)",
            marginTop: 14,
            flexWrap: "wrap",
          }}
        >
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span style={{ fontSize: 12.5, color: "var(--text-muted)" }}>
              Текущая версия:
            </span>
            <strong style={{ fontSize: 13 }}>v{updateInfo.current_version}</strong>
            <span style={{ color: "var(--text-muted)" }}>→</span>
            <span
              className="badge"
              style={{
                background: "rgba(34, 197, 94, 0.16)",
                color: "#4ade80",
                fontWeight: 700,
                fontSize: 12.5,
                padding: "2px 8px",
              }}
            >
              v{updateInfo.latest_version}
            </span>
          </div>
          {formattedDate && (
            <small style={{ color: "var(--text-muted)", fontSize: 12 }}>
              Выпущено: {formattedDate}
            </small>
          )}
        </div>

        {updateInfo.asset_name && (
          <div
            style={{
              marginTop: 8,
              fontSize: 12,
              color: "var(--text-muted)",
              display: "flex",
              alignItems: "center",
              justifyContent: "space-between",
              gap: 8,
            }}
          >
            <span>
              Пакет установки: <strong style={{ color: "var(--text-secondary)" }}>{updateInfo.asset_name}</strong>
            </span>
            {updateInfo.asset_size ? (
              <span>Размер: {formatMb(updateInfo.asset_size)}</span>
            ) : null}
          </div>
        )}

        {updateInfo.release_notes && (
          <div className="update-notes-box">
            <strong
              style={{
                display: "block",
                marginBottom: 6,
                color: "var(--text-primary)",
                fontSize: 12.5,
              }}
            >
              Что нового в релизе {updateInfo.release_name || `v${updateInfo.latest_version}`}:
            </strong>
            {updateInfo.release_notes}
          </div>
        )}

        {installing && progress && (
          <div style={{ marginTop: 14 }}>
            <div
              style={{
                display: "flex",
                justifyContent: "space-between",
                fontSize: 12,
                color: "var(--text-secondary)",
                marginBottom: 6,
              }}
            >
              <span>Скачивание установщика…</span>
              <span>
                {progress.percent}%
                {progress.total
                  ? ` (${formatMb(progress.downloaded)} / ${formatMb(progress.total)})`
                  : progress.downloaded > 0
                    ? ` (${formatMb(progress.downloaded)})`
                    : ""}
              </span>
            </div>
            <div
              style={{
                width: "100%",
                height: 8,
                borderRadius: 999,
                background: "var(--bg-surface)",
                overflow: "hidden",
                border: "1px solid var(--border-subtle)",
              }}
            >
              <div
                style={{
                  width: `${Math.max(4, progress.percent)}%`,
                  height: "100%",
                  background: "var(--accent)",
                  transition: "width 0.2s ease",
                }}
              />
            </div>
          </div>
        )}

        {installedMsg && (
          <div
            style={{
              marginTop: 12,
              padding: "10px 12px",
              borderRadius: 8,
              background: "rgba(34, 197, 94, 0.14)",
              border: "1px solid rgba(34, 197, 94, 0.35)",
              color: "#86efac",
              fontSize: 12.5,
              display: "flex",
              alignItems: "center",
              gap: 8,
            }}
          >
            <CheckCircle2 size={16} />
            <span>{installedMsg}</span>
          </div>
        )}

        {errorMsg && (
          <div
            style={{
              marginTop: 12,
              padding: "10px 12px",
              borderRadius: 8,
              background: "rgba(239, 68, 68, 0.14)",
              border: "1px solid rgba(239, 68, 68, 0.35)",
              color: "#fca5a5",
              fontSize: 12.5,
              display: "flex",
              alignItems: "center",
              gap: 8,
            }}
          >
            <AlertCircle size={16} />
            <span>{errorMsg}</span>
          </div>
        )}

        <div
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            gap: 10,
            marginTop: 18,
            paddingTop: 14,
            borderTop: "1px solid var(--border-subtle)",
            flexWrap: "wrap",
          }}
        >
          <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
            <button
              type="button"
              className="btn btn-quiet"
              onClick={handleSkipVersion}
              disabled={installing}
              style={{ fontSize: 12 }}
            >
              Пропустить v{updateInfo.latest_version}
            </button>
            <button
              type="button"
              className="btn btn-quiet"
              onClick={onClose}
              disabled={installing}
              style={{ fontSize: 12 }}
            >
              Напомнить позже
            </button>
          </div>

          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <button
              type="button"
              className="btn btn-secondary"
              onClick={handleOpenReleasePage}
              style={{ fontSize: 12.5 }}
            >
              <ExternalLink size={14} />
              <span>GitHub Release</span>
            </button>
            <button
              type="button"
              className="btn btn-primary"
              onClick={handleInstallNow}
              disabled={installing}
              style={{ fontSize: 13 }}
            >
              <Download size={14} />
              <span>
                {installing ? "Загрузка и запуск…" : "Обновить сейчас"}
              </span>
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};
