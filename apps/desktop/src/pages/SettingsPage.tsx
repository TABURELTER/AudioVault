import { useEffect, useState } from "react";
import { Eye, EyeOff, FolderOpen, Check, RefreshCw, Moon, Sun, Trash2, AlertTriangle } from "lucide-react";
import { api } from "../api/client";
import type { Settings } from "../api/types";
import { EmptyState, LoadingRows } from "../components/Primitives";

interface Props {
  currentTheme?: "dark" | "light";
  onThemeChange?: (theme: "dark" | "light") => void;
  onCatalogReset?: () => void;
}

export function SettingsPage({ currentTheme, onThemeChange, onCatalogReset }: Props) {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [theme, setTheme] = useState<"dark" | "light">(() => {
    if (currentTheme) return currentTheme;
    try {
      const saved = localStorage.getItem("audiovault-theme");
      if (saved === "light" || saved === "dark") return saved;
    } catch {}
    return "dark";
  });

  const handleSelectTheme = (newTheme: "dark" | "light") => {
    setTheme(newTheme);
    try {
      localStorage.setItem("audiovault-theme", newTheme);
      localStorage.setItem("audiovault-theme-chosen", "true");
    } catch {}
    document.documentElement.setAttribute("data-theme", newTheme);
    onThemeChange?.(newTheme);
  };
  const [retry, setRetry] = useState(0);
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [cookie, setCookie] = useState("");
  const [proxy, setProxy] = useState("");
  const [baseUrl, setBaseUrl] = useState("https://rutracker.org/forum/");
  const [ffmpeg, setFfmpeg] = useState("");
  const [showPassword, setShowPassword] = useState(false);
  const [showCookie, setShowCookie] = useState(false);
  const [guide, setGuide] = useState(false);
  const [busy, setBusy] = useState("");
  const [showResetModal, setShowResetModal] = useState(false);
  const [resetting, setResetting] = useState(false);
  const [message, setMessage] = useState<{ text: string; ok: boolean } | null>(
    null,
  );

  const handleResetCatalog = async () => {
    setResetting(true);
    try {
      await api.resetCatalog();
      setMessage({
        ok: true,
        text: "Локальное хранилище и база данных успешно сброшены.",
      });
      setShowResetModal(false);
      onCatalogReset?.();
    } catch (e) {
      console.error("Failed to reset catalog:", e);
      setMessage({
        ok: false,
        text: "Ошибка при сбросе каталога: " + String(e),
      });
    } finally {
      setResetting(false);
    }
  };
  useEffect(() => {
    let alive = true;
    api
      .getSettings()
      .then((s) => {
        if (!alive) return;
        setSettings(s);
        setUsername(s.rutracker_username || "");
        setCookie(s.rutracker_cookie || "");
        setProxy(s.rutracker_proxy || "");
        setBaseUrl(s.rutracker_base_url || "https://rutracker.org/forum/");
        setFfmpeg(s.ffmpeg_path || "");
      })
      .catch(() => {
        if (alive)
          setMessage({
            text: "Не удалось загрузить настройки. Попробуйте ещё раз.",
            ok: false,
          });
      });
    return () => {
      alive = false;
    };
  }, [retry]);
  const saveConnection = async (test = false) => {
    setBusy(test ? "test" : "connection");
    setMessage(null);
    try {
      await api.saveSettings({
        rutracker_username: username.trim(),
        rutracker_cookie: cookie.trim(),
        rutracker_proxy: proxy.trim(),
        rutracker_base_url: baseUrl.trim(),
      });
      if (password && username.trim()) {
        await api.saveCredentials(username.trim(), password);
        setPassword("");
      }
      if (test) {
        const user = await api.testRutrackerConnection();
        setMessage({
          ok: true,
          text: `Подключение установлено${user ? ` · ${user}` : ""}`,
        });
      } else setMessage({ ok: true, text: "Настройки подключения сохранены" });
    } catch (e) {
      const text = String(e);
      setMessage({
        ok: false,
        text: /Cloudflare|Turnstile|bb_data|капч/i.test(text)
          ? "Трекер требует проверку в браузере. Обновите cookie bb_data и повторите подключение."
          : /not logged|авторизован/i.test(text)
            ? "Сессия истекла. Проверьте логин или обновите cookie bb_data."
            : "Не удалось сохранить или проверить подключение. Проверьте адрес, прокси и данные сессии.",
      });
    } finally {
      setBusy("");
    }
  };
  const saveFfmpeg = async () => {
    setBusy("ffmpeg");
    setMessage(null);
    try {
      await api.saveSettings({ ffmpeg_path: ffmpeg.trim() });
      const s = await api.getSettings();
      setSettings(s);
      setMessage({
        ok: s.ffmpeg_ok,
        text: s.ffmpeg_ok
          ? "FFmpeg найден. Путь сохранён."
          : "Путь сохранён, но FFmpeg не найден. Проверьте расположение файла.",
      });
    } catch {
      setMessage({ ok: false, text: "Не удалось сохранить путь к FFmpeg." });
    } finally {
      setBusy("");
    }
  };
  const reveal = async (path: string) => {
    try {
      await api.revealFolder(path);
    } catch {
      setMessage({
        ok: false,
        text: "Не удалось открыть папку. Проверьте, существует ли путь.",
      });
    }
  };
  if (!settings)
    return message ? (
      <EmptyState
        title="Настройки не загрузились"
        description={message.text}
        action={
          <button
            className="btn btn-secondary"
            onClick={() => {
              setMessage(null);
              setRetry((value) => value + 1);
            }}
          >
            Повторить
          </button>
        }
      />
    ) : (
      <LoadingRows />
    );
  return (
    <div className="settings-page">
      {message && (
        <div
          className={`notice ${message.ok ? "" : "error"}`}
          role="status"
          style={
            message.ok
              ? { background: "var(--success-bg)", color: "var(--success)" }
              : undefined
          }
        >
          {message.ok && <Check size={14} />}
          {message.text}
          <button
            className="plain-icon"
            aria-label="Скрыть сообщение"
            onClick={() => setMessage(null)}
          >
            ×
          </button>
        </div>
      )}

      {/* Theme Section */}
      <section className="settings-section">
        <div>
          <h2>Тема оформления</h2>
          <p>Цветовая палитра и комфорт чтения для глаз.</p>
        </div>
        <div className="settings-fields">
          <div className="theme-selector" style={{ margin: 0 }}>
            <div
              className={`theme-card ${theme === "dark" ? "active" : ""}`}
              onClick={() => handleSelectTheme("dark")}
              role="button"
              tabIndex={0}
              aria-pressed={theme === "dark"}
            >
              <div
                className="theme-preview-box"
                style={{
                  background: "#131316",
                  padding: "8px",
                  flexDirection: "column",
                  justifyContent: "space-between",
                  border: "1px solid #36364a",
                }}
              >
                <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
                  <div style={{ width: 14, height: 14, borderRadius: 3, background: "#f59e0b" }} />
                  <div style={{ height: 6, width: "60%", borderRadius: 2, background: "#2e2e3d" }} />
                </div>
                <div
                  style={{
                    background: "#1c1c25",
                    border: "1px solid #36364a",
                    borderRadius: 4,
                    padding: "5px 6px",
                    display: "flex",
                    justifyContent: "space-between",
                    alignItems: "center",
                  }}
                >
                  <div style={{ height: 6, width: "50%", borderRadius: 2, background: "#ffffff" }} />
                  <div style={{ width: 12, height: 6, borderRadius: 2, background: "#10b981" }} />
                </div>
              </div>
              <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
                <Moon size={16} color="var(--accent)" />
                <strong>Тёмная тема</strong>
                {theme === "dark" && <Check size={16} color="var(--accent)" />}
              </div>
              <p>Тёплый бархатный графит & античное золото</p>
            </div>

            <div
              className={`theme-card ${theme === "light" ? "active" : ""}`}
              onClick={() => handleSelectTheme("light")}
              role="button"
              tabIndex={0}
              aria-pressed={theme === "light"}
            >
              <div
                className="theme-preview-box"
                style={{
                  background: "#f7f6f2",
                  padding: "8px",
                  flexDirection: "column",
                  justifyContent: "space-between",
                  border: "1px solid #d6cebf",
                }}
              >
                <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
                  <div style={{ width: 14, height: 14, borderRadius: 3, background: "#d97706" }} />
                  <div style={{ height: 6, width: "60%", borderRadius: 2, background: "#e2ddd4" }} />
                </div>
                <div
                  style={{
                    background: "#ffffff",
                    border: "1px solid #d6cebf",
                    borderRadius: 4,
                    padding: "5px 6px",
                    display: "flex",
                    justifyContent: "space-between",
                    alignItems: "center",
                  }}
                >
                  <div style={{ height: 6, width: "50%", borderRadius: 2, background: "#181512" }} />
                  <div style={{ width: 12, height: 6, borderRadius: 2, background: "#059669" }} />
                </div>
              </div>
              <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
                <Sun size={16} color="var(--accent)" />
                <strong>Светлая тема</strong>
                {theme === "light" && <Check size={16} color="var(--accent)" />}
              </div>
              <p>Книжная слоновая кость & тёплый янтарь</p>
            </div>
          </div>
        </div>
      </section>

      <section className="settings-section">
        <div>
          <h2>Подключение к RuTracker</h2>
          <p>Учётная запись, сессия браузера и параметры сети.</p>
        </div>
        <div className="settings-fields">
          <div className="form-row">
            <label htmlFor="username">Логин</label>
            <input
              id="username"
              className="input-text"
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              autoComplete="username"
              placeholder="Имя пользователя"
            />
          </div>
          <div className="form-row">
            <label htmlFor="password">Пароль</label>
            <div style={{ display: "flex", gap: 6 }}>
              <input
                id="password"
                className="input-text"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                type={showPassword ? "text" : "password"}
                autoComplete="new-password"
                placeholder={
                  settings.rutracker_username
                    ? "Новый пароль (необязательно)"
                    : "Пароль"
                }
              />
              <button
                className="plain-icon"
                aria-label={showPassword ? "Скрыть пароль" : "Показать пароль"}
                onClick={() => setShowPassword(!showPassword)}
              >
                {showPassword ? <EyeOff size={15} /> : <Eye size={15} />}
              </button>
            </div>
          </div>
          <div className="form-row">
            <label htmlFor="cookie">Cookie bb_data</label>
            <div>
              <div style={{ display: "flex", gap: 6 }}>
                <input
                  id="cookie"
                  type={showCookie ? "text" : "password"}
                  className="input-text"
                  value={cookie}
                  onChange={(e) => setCookie(e.target.value)}
                  autoComplete="off"
                  placeholder="Сессия из вашего браузера"
                />
                <button
                  className="plain-icon"
                  aria-label={showCookie ? "Скрыть cookie" : "Показать cookie"}
                  onClick={() => setShowCookie(!showCookie)}
                >
                  {showCookie ? <EyeOff size={15} /> : <Eye size={15} />}
                </button>
              </div>
              <p>
                Используйте сессию браузера, если вход блокируется проверкой
                трекера.
              </p>
              <button
                className="btn btn-quiet"
                style={{ paddingLeft: 0 }}
                onClick={() => setGuide(!guide)}
                aria-expanded={guide}
              >
                {guide ? "Скрыть инструкцию" : "Как получить cookie"}
              </button>
              {guide && (
                <ol className="settings-guide">
                  <li>
                    Откройте rutracker.org в браузере и войдите в аккаунт.
                  </li>
                  <li>Нажмите F12 → Application → Cookies → rutracker.org.</li>
                  <li>Скопируйте значение bb_data и вставьте в это поле.</li>
                </ol>
              )}
            </div>
          </div>
          <div className="form-row">
            <label htmlFor="proxy">Прокси</label>
            <div>
              <input
                id="proxy"
                className="input-text"
                value={proxy}
                onChange={(e) => setProxy(e.target.value)}
                placeholder="HTTP / SOCKS5 · автоматический выбор"
              />
              <p>Пустое поле: активный Clash или прямое соединение.</p>
              {!proxy && (
                <button
                  className="btn btn-quiet"
                  style={{ paddingLeft: 0 }}
                  onClick={() => setProxy("http://127.0.0.1:7897")}
                >
                  Использовать Clash · 127.0.0.1:7897
                </button>
              )}
            </div>
          </div>
          <div className="form-row">
            <label htmlFor="base-url">Адрес трекера</label>
            <input
              id="base-url"
              className="input-text"
              value={baseUrl}
              onChange={(e) => setBaseUrl(e.target.value)}
              placeholder="https://rutracker.org/forum/"
            />
          </div>
          <p className="muted" style={{ fontSize: 11 }}>
            Пароль хранится в Windows Credential Manager. Cookie сессии хранится
            в локальном каталоге.
          </p>
          <div className="settings-actions">
            <button
              className="btn btn-secondary"
              disabled={!!busy}
              onClick={() => saveConnection(true)}
            >
              <RefreshCw size={13} className={busy === "test" ? "spin" : ""} />
              Проверить подключение
            </button>
            <button
              className="btn btn-primary"
              disabled={!!busy || (!!password && !username.trim())}
              onClick={() => saveConnection()}
            >
              {busy === "connection" ? "Сохранение…" : "Сохранить"}
            </button>
          </div>
        </div>
      </section>
      <section className="settings-section">
        <div>
          <h2>Конвертация</h2>
          <p>FFmpeg собирает главы и конвертирует аудио.</p>
        </div>
        <div className="settings-fields">
          <div className="form-row">
            <label htmlFor="ffmpeg">Путь к FFmpeg</label>
            <div>
              <input
                id="ffmpeg"
                className="input-text"
                value={ffmpeg}
                onChange={(e) => setFfmpeg(e.target.value)}
                placeholder="C:\ffmpeg\bin\ffmpeg.exe"
              />
              <p
                style={{
                  color: settings.ffmpeg_ok
                    ? "var(--success)"
                    : "var(--warning)",
                }}
              >
                {settings.ffmpeg_ok ? "FFmpeg обнаружен" : "FFmpeg не найден"}
                {settings.ffmpeg_version && ` · ${settings.ffmpeg_version}`}
              </p>
              <p>Можно оставить пустым, если FFmpeg доступен автоматически.</p>
            </div>
          </div>
          <div className="settings-actions">
            <button
              className="btn btn-secondary"
              disabled={!!busy}
              onClick={saveFfmpeg}
            >
              {busy === "ffmpeg" ? "Проверка…" : "Проверить и сохранить"}
            </button>
          </div>
        </div>
      </section>
      <section className="settings-section">
        <div>
          <h2>Локальные данные</h2>
          <p>Каталог и диагностические журналы на этом компьютере.</p>
        </div>
        <div className="settings-fields">
          {[
            ["Каталог", settings.db_path],
            ["Журналы", settings.logs_path],
          ].map(([label, path]) => (
            <div className="form-row" key={label}>
              <label>{label}</label>
              <div>
                <p className="settings-path">
                  {path || "Расположение по умолчанию"}
                </p>
                <button
                  className="btn btn-quiet"
                  disabled={!path}
                  onClick={() => reveal(path)}
                >
                  <FolderOpen size={13} />
                  Открыть папку
                </button>
              </div>
            </div>
          ))}
        </div>
      </section>
      <section className="settings-section danger-zone" style={{ borderLeft: "3px solid var(--danger)", background: "rgba(239, 68, 68, 0.05)" }}>
        <div>
          <h2 style={{ color: "var(--danger)", display: "flex", alignItems: "center", gap: "8px" }}>
            <AlertTriangle size={18} />
            Опасная зона
          </h2>
          <p>Необратимые операции с локальной базой данных.</p>
        </div>
        <div className="settings-fields">
          <div className="form-row">
            <label style={{ color: "var(--danger)" }}>Сброс каталога</label>
            <div>
              <p style={{ marginBottom: "12px", lineHeight: 1.5, fontSize: "14px" }}>
                Полная очистка локальной базы данных AudioVault: удаляет все проиндексированные книги, электронные книги, найденные дубликаты и историю задач.
              </p>
              <div
                style={{
                  padding: "12px 16px",
                  background: "rgba(239, 68, 68, 0.08)",
                  border: "1.5px solid rgba(239, 68, 68, 0.3)",
                  borderRadius: "8px",
                  marginBottom: "16px",
                }}
              >
                <strong style={{ color: "var(--danger)", display: "block", marginBottom: "4px", fontSize: "14.5px" }}>
                  ⚠️ Ваши файлы на дисках останутся нетронутыми!
                </strong>
                <span style={{ fontSize: "13.5px", color: "var(--text-secondary)", lineHeight: 1.5 }}>
                  Будет очищена только локальная база метаданных и поисковый индекс на этом компьютере. Сами аудиокниги и электронные книги на ваших накопителях НЕ будут удалены или изменены.
                </span>
              </div>
              <button
                className="btn btn-danger"
                style={{ minHeight: "38px", padding: "8px 18px", fontSize: "14px" }}
                disabled={resetting}
                onClick={() => setShowResetModal(true)}
              >
                <Trash2 size={15} />
                Сбросить локальное хранилище книг…
              </button>
            </div>
          </div>
        </div>
      </section>

      {/* Confirmation Modal */}
      {showResetModal && (
        <div className="modal-backdrop" role="dialog" aria-modal="true">
          <div className="modal" style={{ width: "min(520px, calc(100vw - 32px))", padding: "28px" }}>
            <div style={{ display: "flex", alignItems: "flex-start", gap: "14px" }}>
              <div
                style={{
                  width: "44px",
                  height: "44px",
                  borderRadius: "50%",
                  background: "rgba(239, 68, 68, 0.15)",
                  display: "grid",
                  placeItems: "center",
                  color: "var(--danger)",
                  flexShrink: 0,
                }}
              >
                <AlertTriangle size={24} />
              </div>
              <div style={{ flex: 1 }}>
                <h3 style={{ fontSize: "19px", fontWeight: 700, margin: "0 0 8px", color: "var(--text-primary)" }}>
                  Сбросить локальное хранилище?
                </h3>
                <p style={{ fontSize: "14.5px", color: "var(--text-secondary)", lineHeight: 1.5, margin: "0 0 16px" }}>
                  Вы собираетесь полностью очистить каталог AudioVault на этом компьютере. Все проиндексированные записи аудиокниг, электронных книг и очереди задач будут удалены.
                </p>
                <div
                  style={{
                    padding: "10px 14px",
                    background: "rgba(52, 211, 153, 0.1)",
                    border: "1px solid rgba(52, 211, 153, 0.25)",
                    borderRadius: "6px",
                    marginBottom: "20px",
                  }}
                >
                  <p style={{ margin: 0, fontSize: "13.5px", color: "var(--success)" }}>
                    ✓ <strong>Безопасно:</strong> файлы на накопителях (аудио, fb2, pdf, epub и др.) не будут затронуты.
                  </p>
                </div>
                <div style={{ display: "flex", justifyContent: "flex-end", gap: "10px" }}>
                  <button
                    className="btn btn-quiet"
                    onClick={() => setShowResetModal(false)}
                    disabled={resetting}
                    style={{ minHeight: "38px", padding: "8px 16px", fontSize: "14px" }}
                  >
                    Отмена
                  </button>
                  <button
                    className="btn btn-danger"
                    onClick={handleResetCatalog}
                    disabled={resetting}
                    style={{ minHeight: "38px", padding: "8px 18px", fontSize: "14px" }}
                  >
                    <Trash2 size={15} />
                    {resetting ? "Очистка базы…" : "Да, полностью сбросить"}
                  </button>
                </div>
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
