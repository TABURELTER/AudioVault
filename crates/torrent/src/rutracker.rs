//! RuTracker search provider.
//!
//! * Logs in with the user's own credentials (stored by the caller in the OS
//!   keyring; never logged or persisted here). Session cookies live only in
//!   memory for the lifetime of the client.
//! * If the site answers with a CAPTCHA the login fails with
//!   [`TorrentError::CaptchaRequired`]; no attempt is made to solve or bypass it.
//! * Requests are serialized and spaced at least [`MIN_REQUEST_INTERVAL`] apart.
//! * The site uses windows-1251: query strings and form bodies are encoded in
//!   cp1251, responses are decoded from cp1251.
//!
//! HTML parsing is isolated in pure functions ([`parse_search_page`],
//! [`classify_page`]) covered by fixture tests. If RuTracker changes its
//! markup, only these functions need adjusting.

use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use encoding_rs::WINDOWS_1251;
use percent_encoding::{NON_ALPHANUMERIC, percent_encode};
use scraper::{ElementRef, Html, Selector};
use tokio::sync::Mutex;

use crate::provider::{SearchProvider, SearchQuery, SearchResult};
use crate::{Result, TorrentError};

pub const DEFAULT_BASE_URL: &str = "https://rutracker.org/forum/";
pub const MIN_REQUEST_INTERVAL: Duration = Duration::from_millis(1500);
const SITE: &str = "RuTracker";
const MAX_TORRENT_BYTES: usize = 20 * 1024 * 1024;

/// What kind of page the server returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageKind {
    LoggedIn,
    LoginForm,
    Captcha,
    WrongCredentials,
    CloudflareChallenge,
}

pub struct RutrackerClient {
    http: reqwest::Client,
    base: url::Url,
    last_request: Mutex<Option<Instant>>,
    logged_in: Mutex<bool>,
    cookie_jar: Arc<reqwest::cookie::Jar>,
}

impl RutrackerClient {
    pub fn new(base_url: Option<&str>, proxy_url: Option<&str>, cookie: Option<&str>) -> Result<Self> {
        let mut base = base_url.unwrap_or(DEFAULT_BASE_URL).trim().to_string();
        if !base.ends_with('/') {
            base.push('/');
        }
        let base = url::Url::parse(&base).map_err(|e| TorrentError::InvalidSource(format!("bad base URL: {e}")))?;
        if base.scheme() != "https" && base.scheme() != "http" {
            return Err(TorrentError::InvalidSource("base URL must be http(s)".into()));
        }

        let cookie_jar = Arc::new(reqwest::cookie::Jar::default());
        let mut has_cookie = false;
        if let Some(c) = cookie.filter(|c| !c.trim().is_empty()) {
            Self::inject_cookies(&cookie_jar, &base, c);
            has_cookie = true;
        }

        let mut builder = reqwest::Client::builder()
            .cookie_provider(cookie_jar.clone())
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36")
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(15));

        if let Some(proxy) = proxy_url.filter(|p| !p.trim().is_empty()) {
            builder = builder.proxy(reqwest::Proxy::all(proxy.trim())?);
        } else {
            // Check if local Clash/Mihomo port 7897 is active
            if std::net::TcpStream::connect_timeout(
                &std::net::SocketAddr::from(([127, 0, 0, 1], 7897)),
                Duration::from_millis(150),
            )
            .is_ok()
            {
                tracing::info!("RutrackerClient: auto-routing via active local Clash proxy 127.0.0.1:7897");
                if let Ok(proxy) = reqwest::Proxy::all("http://127.0.0.1:7897") {
                    builder = builder.proxy(proxy);
                }
            } else {
                // Ignore harmful Windows environment proxies (such as ai.comss.one) that block torrents
                builder = builder.no_proxy();
            }
        }

        let http = builder.build()?;
        Ok(Self {
            http,
            base,
            last_request: Mutex::new(None),
            logged_in: Mutex::new(has_cookie),
            cookie_jar,
        })
    }

    fn inject_cookies(jar: &reqwest::cookie::Jar, base: &url::Url, cookie_str: &str) {
        let cookie_str = cookie_str.trim();
        if cookie_str.is_empty() {
            return;
        }
        if cookie_str.contains(';') || cookie_str.contains('=') {
            for part in cookie_str.split(';') {
                let part = part.trim();
                if !part.is_empty() {
                    jar.add_cookie_str(part, base);
                }
            }
        } else {
            // Bare token passed as bb_data
            jar.add_cookie_str(&format!("bb_data={cookie_str}"), base);
        }
        if !cookie_str.contains("bb_ssl") {
            jar.add_cookie_str("bb_ssl=1", base);
        }
    }

    pub async fn set_session_cookie(&self, cookie: &str) {
        Self::inject_cookies(&self.cookie_jar, &self.base, cookie);
        *self.logged_in.lock().await = true;
    }

    /// Checks if current session is authenticated by fetching index.php and looking for logged-in user.
    pub async fn check_session(&self) -> Result<String> {
        let (_, body) = self.get_bytes(self.url("index.php")?).await?;
        let html = cp1251_decode(&body);
        if matches!(classify_page(&html), PageKind::CloudflareChallenge) {
            *self.logged_in.lock().await = false;
            return Err(TorrentError::CloudflareChallenge);
        }

        let user = {
            let doc = Html::parse_document(&html);
            doc.select(&sel("#logged-in-username")).next().map(|el| text_of(&el))
        };

        if let Some(u) = user {
            *self.logged_in.lock().await = true;
            Ok(u)
        } else {
            *self.logged_in.lock().await = false;
            Err(TorrentError::NotLoggedIn(SITE.into()))
        }
    }

    fn url(&self, path: &str) -> Result<url::Url> {
        self.base.join(path).map_err(|e| TorrentError::InvalidSource(e.to_string()))
    }

    async fn throttle(&self) {
        let mut last = self.last_request.lock().await;
        if let Some(t) = *last {
            let elapsed = t.elapsed();
            if elapsed < MIN_REQUEST_INTERVAL {
                tokio::time::sleep(MIN_REQUEST_INTERVAL - elapsed).await;
            }
        }
        *last = Some(Instant::now());
    }

    async fn get_bytes(&self, url: url::Url) -> Result<(reqwest::header::HeaderMap, Vec<u8>)> {
        self.throttle().await;
        let resp = self.http.get(url).send().await?;
        let status = resp.status();
        let headers = resp.headers().clone();
        let body = resp.bytes().await?.to_vec();
        if !status.is_success() && status.as_u16() != 403 {
            return Err(TorrentError::UnexpectedResponse { site: SITE.into(), detail: format!("HTTP {status}") });
        }
        Ok((headers, body))
    }

    pub fn topic_url(&self, topic_id: &str) -> String {
        self.url(&format!("viewtopic.php?t={topic_id}")).map(|u| u.to_string()).unwrap_or_default()
    }
}

/// Percent-encodes a value as windows-1251 bytes.
pub fn cp1251_encode(value: &str) -> String {
    let (bytes, _, _) = WINDOWS_1251.encode(value);
    percent_encode(&bytes, NON_ALPHANUMERIC).to_string()
}

pub fn cp1251_decode(bytes: &[u8]) -> String {
    // The site declares charset=windows-1251; tolerate UTF-8 mirrors.
    if let Ok(s) = std::str::from_utf8(bytes)
        && (s.contains("charset=utf-8") || s.contains("charset=UTF-8"))
    {
        return s.to_string();
    }
    WINDOWS_1251.decode(bytes).0.into_owned()
}

fn sel(s: &str) -> Selector {
    // Selectors are compile-time literals; a failure is a programming error caught by tests.
    Selector::parse(s).unwrap_or_else(|e| panic!("invalid selector {s}: {e:?}"))
}

/// Classifies a page returned after login or a protected request.
pub fn classify_page(html: &str) -> PageKind {
    if html.contains("Just a moment...")
        || html.contains("challenges.cloudflare.com")
        || html.contains("cf-mitigated")
        || html.contains("Enable JavaScript and cookies to continue")
    {
        return PageKind::CloudflareChallenge;
    }
    let doc = Html::parse_document(html);
    if doc.select(&sel("#logged-in-username")).next().is_some() {
        return PageKind::LoggedIn;
    }
    let has_captcha = doc.select(&sel("input[name=cap_sid]")).next().is_some()
        || doc.select(&sel("img[src*=captcha]")).next().is_some();
    if has_captcha {
        return PageKind::Captcha;
    }
    let text = doc.root_element().text().collect::<String>().to_lowercase();
    if text.contains("неверный пароль") || text.contains("неверное/неактивное имя") || text.contains("wrong username") {
        return PageKind::WrongCredentials;
    }
    PageKind::LoginForm
}

fn attr_i64(el: &ElementRef<'_>, name: &str) -> Option<i64> {
    el.value().attr(name).and_then(|v| v.trim().parse::<i64>().ok())
}

fn text_of(el: &ElementRef<'_>) -> String {
    el.text().collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Parses "1.37 GB", "700 MB", "1,5 ГБ" etc.
pub fn parse_size_text(s: &str) -> Option<u64> {
    let s = s.replace('\u{a0}', " ").replace(',', ".");
    let s = s.trim().trim_end_matches('↓').trim();
    let idx = s.find(|c: char| !(c.is_ascii_digit() || c == '.'))?;
    let (num, unit) = s.split_at(idx);
    let n: f64 = num.trim().parse().ok()?;
    let unit = unit.trim().to_lowercase();
    let mul = match unit.as_str() {
        "b" | "б" => 1.0,
        "kb" | "кб" => 1024.0,
        "mb" | "мб" => 1024.0 * 1024.0,
        "gb" | "гб" => 1024.0 * 1024.0 * 1024.0,
        "tb" | "тб" => 1024.0f64.powi(4),
        _ => return None,
    };
    Some((n * mul) as u64)
}

fn topic_id_from_href(href: &str) -> Option<String> {
    let idx = href.find("t=")?;
    let id: String = href[idx + 2..].chars().take_while(|c| c.is_ascii_digit()).collect();
    (!id.is_empty()).then_some(id)
}

/// Parses `tracker.php` search results.
pub fn parse_search_page(html: &str, topic_url: impl Fn(&str) -> String) -> Vec<SearchResult> {
    let doc = Html::parse_document(html);
    let row_sel = sel("#tor-tbl tr.hl-tr, #tor-tbl tr[data-topic_id]");
    let title_sel = sel("a.tLink, a.tt-text, a[href*='viewtopic.php?t=']");
    let cat_sel = sel(".f-name a, a.gen.f");
    let size_sel = sel("td.tor-size");
    let seed_sel = sel(".seedmed");
    let leech_sel = sel(".leechmed");
    let dl_sel = sel("td.number-format");
    let td_sel = sel("td");
    let approved_sel = sel(".tor-approved, .tor-icon.tor-approved");

    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for row in doc.select(&row_sel) {
        let Some(title_el) = row.select(&title_sel).next() else { continue };
        let topic_id = row
            .value()
            .attr("data-topic_id")
            .map(str::to_string)
            .or_else(|| title_el.value().attr("data-topic_id").map(str::to_string))
            .or_else(|| title_el.value().attr("href").and_then(topic_id_from_href));
        let Some(topic_id) = topic_id else { continue };
        if !seen.insert(topic_id.clone()) {
            continue;
        }
        let title = text_of(&title_el);
        if title.is_empty() {
            continue;
        }
        let category = row.select(&cat_sel).next().map(|e| text_of(&e)).unwrap_or_default();
        let size_bytes = row
            .select(&size_sel)
            .next()
            .and_then(|td| {
                attr_i64(&td, "data-ts_text").map(|v| v.max(0) as u64).or_else(|| parse_size_text(&text_of(&td)))
            })
            .unwrap_or(0);
        let seeders = row
            .select(&seed_sel)
            .next()
            .and_then(|e| attr_i64(&e, "data-ts_text").or_else(|| text_of(&e).parse().ok()))
            .unwrap_or(0)
            .max(0);
        let leechers = row.select(&leech_sel).next().and_then(|e| text_of(&e).parse().ok()).unwrap_or(0);
        let downloads = row
            .select(&dl_sel)
            .next()
            .and_then(|e| text_of(&e).replace([' ', ','], "").parse().ok())
            .unwrap_or(0);
        let added_at = row.select(&td_sel).last().and_then(|td| attr_i64(&td, "data-ts_text")).filter(|v| *v > 946_684_800);
        let approved = row.select(&approved_sel).next().is_some();
        out.push(SearchResult {
            provider: "rutracker".into(),
            topic_url: topic_url(&topic_id),
            topic_id,
            title,
            category,
            size_bytes,
            seeders,
            leechers,
            downloads,
            added_at,
            approved,
        });
    }
    out
}

fn is_torrent_payload(headers: &reqwest::header::HeaderMap, body: &[u8]) -> bool {
    let ct = headers.get(reqwest::header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("");
    body.first() == Some(&b'd') && (ct.contains("bittorrent") || ct.contains("octet-stream") || body.starts_with(b"d8:announce") || body.starts_with(b"d4:info") || body.starts_with(b"d13:announce-list") || body.starts_with(b"d7:comment") || body.starts_with(b"d10:created by"))
}

#[async_trait]
impl SearchProvider for RutrackerClient {
    fn id(&self) -> &'static str {
        "rutracker"
    }

    fn display_name(&self) -> &'static str {
        SITE
    }

    async fn is_logged_in(&self) -> bool {
        *self.logged_in.lock().await
    }

    async fn login(&self, username: &str, password: &str) -> Result<()> {
        if username.trim().is_empty() || password.is_empty() {
            return Err(TorrentError::InvalidCredentials);
        }
        let body = format!(
            "login_username={}&login_password={}&login={}",
            cp1251_encode(username.trim()),
            cp1251_encode(password),
            cp1251_encode("вход")
        );
        self.throttle().await;
        let resp = self
            .http
            .post(self.url("login.php")?)
            .header(reqwest::header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .header(reqwest::header::REFERER, self.url("index.php")?.to_string())
            .body(body)
            .send()
            .await?;
        let status = resp.status();
        let bytes = resp.bytes().await?;
        if !status.is_success() && status.as_u16() != 403 {
            return Err(TorrentError::UnexpectedResponse { site: SITE.into(), detail: format!("HTTP {status}") });
        }
        let html = cp1251_decode(&bytes);
        let kind = classify_page(&html);
        let mut flag = self.logged_in.lock().await;
        *flag = kind == PageKind::LoggedIn;
        match kind {
            PageKind::LoggedIn => {
                tracing::info!("rutracker: logged in");
                Ok(())
            }
            PageKind::Captcha => Err(TorrentError::CaptchaRequired),
            PageKind::CloudflareChallenge => Err(TorrentError::CloudflareChallenge),
            PageKind::WrongCredentials => Err(TorrentError::InvalidCredentials),
            PageKind::LoginForm => Err(TorrentError::UnexpectedResponse {
                site: SITE.into(),
                detail: "login form returned without an explanation".into(),
            }),
        }
    }

    async fn search(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let text = query.text.trim();
        if text.is_empty() {
            return Ok(Vec::new());
        }
        let mut path = format!("tracker.php?nm={}", cp1251_encode(text));
        if !query.categories.is_empty() {
            let cats: Vec<&str> =
                query.categories.iter().map(|c| c.trim()).filter(|c| c.chars().all(|ch| ch.is_ascii_digit())).collect();
            if !cats.is_empty() {
                path.push_str(&format!("&f={}", cats.join(",")));
            }
        }
        let (_, body) = self.get_bytes(self.url(&path)?).await?;
        let html = cp1251_decode(&body);
        let results = parse_search_page(&html, |id| self.topic_url(id));
        if results.is_empty() {
            match classify_page(&html) {
                PageKind::LoggedIn => {}
                PageKind::Captcha => {
                    *self.logged_in.lock().await = false;
                    return Err(TorrentError::CaptchaRequired);
                }
                PageKind::CloudflareChallenge => {
                    *self.logged_in.lock().await = false;
                    return Err(TorrentError::CloudflareChallenge);
                }
                _ => {
                    *self.logged_in.lock().await = false;
                    return Err(TorrentError::NotLoggedIn(SITE.into()));
                }
            }
        }
        Ok(results)
    }

    async fn fetch_torrent(&self, topic_id: &str) -> Result<Vec<u8>> {
        if topic_id.is_empty() || !topic_id.chars().all(|c| c.is_ascii_digit()) {
            return Err(TorrentError::InvalidSource(format!("bad topic id {topic_id}")));
        }
        let (headers, body) = self.get_bytes(self.url(&format!("dl.php?t={topic_id}"))?).await?;
        if body.len() > MAX_TORRENT_BYTES {
            return Err(TorrentError::UnexpectedResponse { site: SITE.into(), detail: "torrent file too large".into() });
        }
        if is_torrent_payload(&headers, &body) {
            return Ok(body);
        }
        let html = cp1251_decode(&body);
        match classify_page(&html) {
            PageKind::Captcha => Err(TorrentError::CaptchaRequired),
            PageKind::CloudflareChallenge => Err(TorrentError::CloudflareChallenge),
            PageKind::LoggedIn => Err(TorrentError::UnexpectedResponse {
                site: SITE.into(),
                detail: "the site returned a page instead of a .torrent file".into(),
            }),
            _ => {
                *self.logged_in.lock().await = false;
                Err(TorrentError::NotLoggedIn(SITE.into()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEARCH: &str = include_str!("../tests/fixtures/rutracker_search.html");
    const LOGIN_CAPTCHA: &str = include_str!("../tests/fixtures/rutracker_login_captcha.html");
    const LOGIN_WRONG: &str = include_str!("../tests/fixtures/rutracker_login_wrong.html");

    #[test]
    fn parses_search_results() {
        let r = parse_search_page(SEARCH, |id| format!("https://rutracker.org/forum/viewtopic.php?t={id}"));
        assert_eq!(r.len(), 3, "{r:#?}");
        let first = &r[0];
        assert_eq!(first.topic_id, "5212345");
        assert_eq!(first.title, "Глуховский Дмитрий - Метро 2033 [Кузнецов Сергей, 2015, 128 kbps, MP3]");
        assert_eq!(first.category, "Аудиокниги: Фантастика, фэнтези");
        assert_eq!(first.size_bytes, 734_003_200);
        assert_eq!(first.seeders, 41);
        assert_eq!(first.leechers, 2);
        assert_eq!(first.downloads, 12345);
        assert_eq!(first.added_at, Some(1_431_936_000));
        assert!(first.approved);
        assert_eq!(first.topic_url, "https://rutracker.org/forum/viewtopic.php?t=5212345");
        // Second row has no data attributes: falls back to text parsing.
        let second = &r[1];
        assert_eq!(second.topic_id, "6000001");
        assert_eq!(second.size_bytes, (1.5 * 1024.0 * 1024.0 * 1024.0) as u64);
        assert_eq!(second.seeders, 7);
        assert!(!second.approved);
        assert_eq!(r[2].seeders, 0);
    }

    #[test]
    fn classifies_pages() {
        assert_eq!(classify_page(SEARCH), PageKind::LoggedIn);
        assert_eq!(classify_page(LOGIN_CAPTCHA), PageKind::Captcha);
        assert_eq!(classify_page(LOGIN_WRONG), PageKind::WrongCredentials);
        assert_eq!(classify_page("<html><body><form action=login.php></form></body></html>"), PageKind::LoginForm);
        assert_eq!(
            classify_page("<!DOCTYPE html><html><head><title>Just a moment...</title></head><body><script src=\"https://challenges.cloudflare.com\"></script></body></html>"),
            PageKind::CloudflareChallenge
        );
    }

    #[test]
    fn cp1251_roundtrip() {
        assert_eq!(cp1251_encode("вход"), "%E2%F5%EE%E4");
        assert_eq!(cp1251_encode("a b"), "a%20b");
        let (bytes, _, _) = WINDOWS_1251.encode("Метро");
        assert_eq!(cp1251_decode(&bytes), "Метро");
    }

    #[test]
    fn size_parsing() {
        assert_eq!(parse_size_text("700 MB"), Some(700 * 1024 * 1024));
        assert_eq!(parse_size_text("1,5\u{a0}GB ↓"), Some((1.5 * 1024.0 * 1024.0 * 1024.0) as u64));
        assert_eq!(parse_size_text("12 parsecs"), None);
    }

    #[test]
    fn torrent_payload_detection() {
        let mut h = reqwest::header::HeaderMap::new();
        h.insert(reqwest::header::CONTENT_TYPE, "application/x-bittorrent".parse().unwrap());
        assert!(is_torrent_payload(&h, b"d8:announce..."));
        assert!(!is_torrent_payload(&h, b"<html>"));
    }

    #[test]
    fn client_rejects_bad_base() {
        assert!(RutrackerClient::new(Some("ftp://x/"), None, None).is_err());
        let c = RutrackerClient::new(Some("https://rutracker.org/forum"), None, None).unwrap();
        assert_eq!(c.topic_url("5"), "https://rutracker.org/forum/viewtopic.php?t=5");
    }
}
