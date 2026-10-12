//! Torrent search provider abstraction.

use async_trait::async_trait;
use serde::Serialize;

use crate::Result;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SearchResult {
    pub provider: String,
    /// Provider-specific topic id (used to fetch the `.torrent`).
    pub topic_id: String,
    pub title: String,
    pub category: String,
    pub size_bytes: u64,
    pub seeders: i64,
    pub leechers: i64,
    pub downloads: i64,
    /// Unix seconds, if the provider reports it.
    pub added_at: Option<i64>,
    /// Web page of the topic, for "open in browser".
    pub topic_url: String,
    /// True when the release is marked verified/approved by the tracker.
    pub approved: bool,
}

#[derive(Debug, Clone, Default)]
pub struct SearchQuery {
    pub text: String,
    /// Restrict to provider categories (forum ids); empty = all.
    pub categories: Vec<String>,
}

#[async_trait]
pub trait SearchProvider: Send + Sync {
    /// Stable identifier, stored in `torrent_transfer.provider`.
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    async fn is_logged_in(&self) -> bool;
    async fn login(&self, username: &str, password: &str) -> Result<()>;
    async fn search(&self, query: &SearchQuery) -> Result<Vec<SearchResult>>;
    /// Downloads the `.torrent` file for a topic.
    async fn fetch_torrent(&self, topic_id: &str) -> Result<Vec<u8>>;
}
