import { useRef, useState } from "react";
import { Download, ArrowUp, ArrowDown, Check, Loader2 } from "lucide-react";
import { api, formatBytes } from "../api/client";
import type { Book, TorrentResult } from "../api/types";
import {
  EmptyState,
  SearchField,
  LoadingRows,
  VirtualList,
} from "../components/Primitives";
interface Props {
  onNavigateToDownloads: () => void;
  onNavigateToSettings?: () => void;
}
const norm = (s: string) =>
  s
    .toLocaleLowerCase("ru-RU")
    .replace(/[^\p{L}\p{N}]+/gu, " ")
    .trim();
export function RuTrackerPage({
  onNavigateToDownloads,
  onNavigateToSettings,
}: Props) {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<TorrentResult[]>([]);
  const [local, setLocal] = useState<Book[]>([]);
  const [loading, setLoading] = useState(false);
  const [searched, setSearched] = useState(false);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState<Set<string>>(new Set());
  const [downloaded, setDownloaded] = useState<Set<string>>(new Set());
  const request = useRef(0);
  const search = async () => {
    if (!query.trim() || loading) return;
    const id = ++request.current;
    setLoading(true);
    setError("");
    try {
      const [remote, catalog] = await Promise.allSettled([
        api.searchTorrents(query.trim()),
        api.searchBooks(query.trim()),
      ]);
      if (id !== request.current) return;
      if (remote.status === "rejected") throw remote.reason;
      setResults(remote.value);
      setLocal(catalog.status === "fulfilled" ? catalog.value : []);
      setSearched(true);
    } catch (e) {
      const msg = String(e);
      setError(
        /Cloudflare|Turnstile|bb_data|капч/i.test(msg)
          ? "Трекер требует проверку в браузере. Обновите cookie bb_data в настройках."
          : /not logged|NotLoggedIn/i.test(msg)
            ? "Для поиска требуется вход. Укажите учётную запись или cookie в настройках."
            : "Не удалось связаться с RuTracker. Проверьте соединение, адрес трекера и прокси.",
      );
    } finally {
      if (id === request.current) setLoading(false);
    }
  };
  const download = async (item: TorrentResult) => {
    setBusy((p) => new Set(p).add(item.id));
    try {
      const ok = await api.downloadTorrent(item.id);
      if (!ok) throw new Error();
      setDownloaded((p) => new Set(p).add(item.id));
    } catch {
      setError(
        "Не удалось добавить раздачу. Проверьте сессию трекера и повторите.",
      );
    } finally {
      setBusy((p) => {
        const next = new Set(p);
        next.delete(item.id);
        return next;
      });
    }
  };
  const catalogStatus = (item: TorrentResult) => {
    const title = norm(item.title);
    const match = local.find(
      (b) => norm(b.title).length >= 6 && title.includes(norm(b.title)),
    );
    if (!match) return "Совпадений не найдено";
    if (
      item.reader &&
      match.narrators &&
      norm(item.reader) !== norm(match.narrators)
    )
      return "Возможно, другой чтец";
    return norm(match.title) === title &&
      norm(match.authors) === norm(item.author)
      ? "Есть в фонотеке"
      : "Возможное совпадение";
  };
  return (
    <div className="collection-page">
      <form
        className="toolbar"
        onSubmit={(e) => {
          e.preventDefault();
          search();
        }}
      >
        <div className="toolbar-left" style={{ flex: 1 }}>
          <div style={{ flex: 1, maxWidth: 560 }}>
            <SearchField
              value={query}
              onChange={setQuery}
              loading={loading}
              placeholder="Название книги, автор или чтец на RuTracker"
              autoFocus
            />
          </div>
          <button
            className="btn btn-primary"
            disabled={loading || !query.trim()}
          >
            Найти
          </button>
        </div>
        <button
          type="button"
          className="btn btn-quiet"
          onClick={onNavigateToDownloads}
        >
          <Download size={14} />
          Загрузки
        </button>
      </form>
      <div className="page-intro">
        <p>
          Найдите новое издание и сравните с локальной коллекцией перед
          загрузкой.
        </p>
        <button className="btn btn-quiet" onClick={onNavigateToSettings}>
          Настройки подключения
        </button>
      </div>
      {error && (
        <div className="notice error" role="alert">
          <span>{error}</span>
          <button className="btn btn-secondary" onClick={onNavigateToSettings}>
            Настройки
          </button>
        </div>
      )}
      {loading && !results.length ? (
        <LoadingRows />
      ) : !results.length ? (
        <EmptyState
          title={searched ? "Раздачи не найдены" : "Пополните вашу коллекцию"}
          description={
            searched
              ? "Попробуйте другое название или фамилию автора."
              : "Поиск аудиокниг на RuTracker. Для подключения может потребоваться сессия вашего браузера."
          }
        />
      ) : (
        <VirtualList
          count={results.length}
          rowHeight={112}
          renderRow={(index) => {
            const item = results[index];
            const status = catalogStatus(item);
            return (
              <div className="torrent-row">
                <div>
                  <h3 title={item.title}>{item.title}</h3>
                  <p>
                    {item.author || "Автор не указан"}
                    {item.reader && ` · ${item.reader}`}
                  </p>
                  <div className="activity-metrics">
                    <span>{formatBytes(item.size_bytes)}</span>
                    {item.bitrate && (
                      <span className="badge badge-format">{item.bitrate}</span>
                    )}
                    <span title="Сиды">
                      <ArrowUp size={12} />
                      {item.seeds}
                    </span>
                    <span title="Пиры">
                      <ArrowDown size={12} />
                      {item.leeches}
                    </span>
                    {item.category && <span>{item.category}</span>}
                  </div>
                </div>
                <div className="torrent-actions">
                  <small
                    title="Предварительная сверка с первыми 50 результатами локального поиска. Проверьте сведения перед загрузкой."
                    style={{
                      color:
                        status === "Есть в фонотеке"
                          ? "var(--success)"
                          : "var(--text-muted)",
                    }}
                  >
                    {status}
                  </small>
                  <button
                    className="btn btn-secondary"
                    disabled={busy.has(item.id) || downloaded.has(item.id)}
                    onClick={() => download(item)}
                  >
                    {busy.has(item.id) ? (
                      <Loader2 size={13} className="spin" />
                    ) : downloaded.has(item.id) ? (
                      <Check size={13} />
                    ) : (
                      <Download size={13} />
                    )}
                    {downloaded.has(item.id) ? "В очереди" : "Загрузить"}
                  </button>
                </div>
              </div>
            );
          }}
        />
      )}
      <div className="collection-footer">
        <span>{results.length} раздач</span>
        <span>Сверка с каталогом — предварительная</span>
      </div>
    </div>
  );
}
