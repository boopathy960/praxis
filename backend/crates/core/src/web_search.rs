//! SearXNG-backed web search — the discovery layer.
//!
//! Everything downstream of this file (HTTPA receipts, governed fetch, semantic
//! rendering, link-following crawl, ranking) could already turn a *URL* into
//! deep evidence. What was missing was the step that turns a *question* into
//! the first URLs. That is what a search engine does, and it is the one thing
//! the local stack cannot synthesize — it needs an index of the web.
//!
//! [SearXNG](https://docs.searxng.org) is a self-hostable metasearch front-end:
//! it relays Google/Bing/DuckDuckGo/etc. and returns a clean JSON list of
//! result URLs, with no third-party API key. Point [`SearxngClient`] at an
//! instance (yours via `ASTRA_SEARXNG_URL`, default `http://localhost:8888`)
//! and a bare query produces the seed URLs the crawl pipeline consumes.
//!
//! Note: the SearXNG JSON output format is *off by default* in stock configs.
//! A self-hosted instance must enable it (`search.formats: [html, json]` in
//! `settings.yml`). Most public instances disable JSON, which is exactly why
//! self-hosting is the recommended path.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::common::{AppError, run_blocking_io};

/// Default instance: the standard SearXNG Docker port on localhost. Self-host
/// expectation, because that is where JSON output is reliably enabled.
const DEFAULT_INSTANCE: &str = "http://localhost:8888";
const DEFAULT_TIMEOUT_MS: u64 = 12_000;
const USER_AGENT: &str =
    "Mozilla/5.0 (compatible; AstraResearchAgent/1.0; +https://github.com/astra)";

/// A single discovery result — a URL the crawl pipeline can seed from.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WebSearchResult {
    pub url: String,
    pub title: String,
    pub snippet: String,
    pub engine: String,
}

/// Thin client over a SearXNG instance's JSON search endpoint.
#[derive(Debug, Clone)]
pub struct SearxngClient {
    instance_url: String,
    timeout_ms: u64,
}

impl Default for SearxngClient {
    fn default() -> Self {
        Self::from_env()
    }
}

impl SearxngClient {
    /// Build from the environment. `ASTRA_SEARXNG_URL` overrides the instance;
    /// `ASTRA_SEARXNG_TIMEOUT_MS` overrides the request timeout.
    #[must_use]
    pub fn from_env() -> Self {
        let instance_url = std::env::var("ASTRA_SEARXNG_URL")
            .ok()
            .map(|raw| raw.trim().trim_end_matches('/').to_string())
            .filter(|raw| !raw.is_empty())
            .unwrap_or_else(|| DEFAULT_INSTANCE.to_string());
        let timeout_ms = std::env::var("ASTRA_SEARXNG_TIMEOUT_MS")
            .ok()
            .and_then(|raw| raw.parse::<u64>().ok())
            .filter(|ms| *ms >= 1_000)
            .unwrap_or(DEFAULT_TIMEOUT_MS);
        Self {
            instance_url,
            timeout_ms,
        }
    }

    #[must_use]
    pub fn instance(&self) -> &str {
        &self.instance_url
    }

    /// Run a query against the instance and return up to `count` result URLs.
    pub fn search(&self, query: &str, count: usize) -> Result<Vec<WebSearchResult>, String> {
        let query = query.trim();
        if query.is_empty() {
            return Err("web_search requires a non-empty query".into());
        }
        let count = count.clamp(1, 50);

        // Build {instance}/search?q=…&format=json. Parse via Url so the query is
        // correctly percent-encoded and the scheme is validated.
        let mut request = url::Url::parse(&format!("{}/search", self.instance_url))
            .map_err(|error| format!("invalid SearXNG instance URL: {error}"))?;
        if !matches!(request.scheme(), "http" | "https") {
            return Err("SearXNG instance must be an http/https URL".into());
        }
        request
            .query_pairs_mut()
            .append_pair("q", query)
            .append_pair("format", "json")
            .append_pair("safesearch", "1");
        let request_url = request.to_string();
        let instance = self.instance_url.clone();
        let timeout = self.timeout_ms;

        // Blocking reqwest on a dedicated thread (its own runtime), so this is
        // safe to call from anywhere including async handlers. See the
        // reqwest-blocking discipline used by nexus::safe_fetch.
        let body = run_blocking_io(move || {
            let client = reqwest::blocking::Client::builder()
                .timeout(Duration::from_millis(timeout))
                .user_agent(USER_AGENT)
                .build()
                .map_err(|error| {
                    AppError::Internal(format!("searxng client build failed: {error}"))
                })?;
            let response = client.get(&request_url).send().map_err(|error| {
                AppError::Internal(format!(
                    "searxng request to {instance} failed: {error} (is the instance running and reachable?)"
                ))
            })?;
            let status = response.status();
            let text = response.text().map_err(|error| {
                AppError::Internal(format!("searxng body read failed: {error}"))
            })?;
            if !status.is_success() {
                return Err(AppError::Internal(format!(
                    "searxng returned HTTP {status}; if this is 403/404 the instance likely has the JSON format disabled (set search.formats: [html, json] in settings.yml)"
                )));
            }
            Ok(text)
        })
        .map_err(|error| error.to_string())?;

        parse_results(&body, count)
    }
}

/// Pure parse of a SearXNG JSON response body into result URLs. Separated from
/// the network call so it can be unit-tested deterministically.
pub fn parse_results(body: &str, count: usize) -> Result<Vec<WebSearchResult>, String> {
    let parsed: Value = serde_json::from_str(body).map_err(|error| {
        format!("searxng response was not JSON: {error} (the instance may have JSON output disabled)")
    })?;
    let results = parsed
        .get("results")
        .and_then(Value::as_array)
        .ok_or("searxng JSON had no 'results' array")?;
    let mut out = Vec::new();
    for item in results {
        let url = item
            .get("url")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string();
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            continue;
        }
        if out.iter().any(|r: &WebSearchResult| r.url == url) {
            continue;
        }
        out.push(WebSearchResult {
            url,
            title: item
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            snippet: item
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            engine: item
                .get("engine")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        });
        if out.len() >= count {
            break;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "query": "rust async",
        "results": [
            {"url": "https://tokio.rs/", "title": "Tokio", "content": "An async runtime.", "engine": "google"},
            {"url": "https://tokio.rs/", "title": "Tokio dup", "content": "dup", "engine": "bing"},
            {"url": "ftp://skip.me", "title": "skip", "content": "", "engine": "x"},
            {"url": "https://docs.rs/tokio", "title": "docs", "content": "API docs", "engine": "ddg"}
        ]
    }"#;

    #[test]
    fn parses_and_dedupes_results() {
        let results = parse_results(SAMPLE, 10).unwrap();
        // ftp skipped, duplicate tokio.rs collapsed → 2 unique http(s) results.
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].url, "https://tokio.rs/");
        assert_eq!(results[0].title, "Tokio");
        assert_eq!(results[1].url, "https://docs.rs/tokio");
    }

    #[test]
    fn respects_count_cap() {
        let results = parse_results(SAMPLE, 1).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn non_json_is_a_clear_error() {
        let err = parse_results("<html>403 forbidden</html>", 5).unwrap_err();
        assert!(err.contains("not JSON"));
    }

    #[test]
    fn missing_results_array_errors() {
        let err = parse_results(r#"{"query":"x"}"#, 5).unwrap_err();
        assert!(err.contains("no 'results'"));
    }

    #[test]
    fn instance_from_env_default() {
        // No env set in this test process → default localhost instance.
        let client = SearxngClient::from_env();
        assert!(client.instance().starts_with("http"));
    }
}
