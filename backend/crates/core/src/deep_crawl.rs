//! Governed deep-crawl engine — the integration the agents needed.
//!
//! This is where three subsystems that already existed in isolation finally
//! work as one pipeline:
//!
//!   1. **HTTPA protocol** — before and after every hop the engine records a
//!      signed, hash-chained receipt in the HTTPA ledger, so the entire crawl
//!      is an attested, tamper-evident trail (`trace_id` ties the hops
//!      together).
//!   2. **Governed fetch** — pages are pulled through
//!      [`safe_fetch_html`](crate::nexus::safe_fetch::safe_fetch_html), which
//!      blocks private/loopback hosts (SSRF) and bounds the response.
//!   3. **Semantic rendering** — each fetched page is run through the
//!      [`SemanticRenderEngine`](crate::semantic_render::SemanticRenderEngine),
//!      turning raw HTML into deep structured evidence (title, headings,
//!      links, OpenGraph, text, a safety profile) plus a deterministic proof
//!      hash. This is the extractor the research/search path never called.
//!
//! On top of that it does a real breadth-first crawl: links extracted by the
//! renderer become the next depth's frontier (bounded by depth, page count,
//! and per-domain limits), and every page is scored for relevance to the
//! query. The whole run executes under the device [`DeviceSupervisor`], so it
//! is watched continuously and sealed into the device audit alongside the
//! HTTPA ledger.

use std::collections::{HashSet, VecDeque};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::Shared;
use crate::common::new_id;
use crate::device_agent::DeviceSupervisor;
use crate::semantic_render::{SemanticRenderEngine, SemanticRenderRequest};
use crate::web_search::SearxngClient;

const DEFAULT_MAX_PAGES: usize = 8;
const HARD_MAX_PAGES: usize = 40;
const DEFAULT_MAX_DEPTH: usize = 1;
const HARD_MAX_DEPTH: usize = 4;
/// How many links from a single page are admitted to the next frontier.
const MAX_LINKS_ENQUEUED_PER_PAGE: usize = 12;
/// How many pages from one domain a single crawl will pull.
const MAX_PAGES_PER_DOMAIN: usize = 6;

/// One page the crawl fetched, rendered, scored, and attested.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawledPage {
    pub url: String,
    pub domain: String,
    pub depth: usize,
    pub title: Option<String>,
    pub relevance: f64,
    pub word_count: usize,
    pub reading_time_minutes: usize,
    pub headings: Vec<String>,
    pub outbound_links: usize,
    pub text_excerpt: String,
    /// Deterministic proof hash from the semantic render engine — lets anyone
    /// re-verify the extraction was not altered.
    pub render_proof_hash: String,
    pub content_hash: String,
    /// Safety profile lifted from the render: a high script/iframe/handler
    /// count is a signal the page is interactive/untrusted.
    pub script_count: usize,
    pub iframe_count: usize,
    pub inline_event_handlers: usize,
    /// Harvested machine-readable records: schema.org entities, tables as
    /// rows, and detected pagination — the deep data-collection payload.
    pub structured: crate::semantic_render::ExtractedData,
    /// Share of this page's meaning that lived only in JavaScript state, in
    /// `[0,1]` — high means it was a single-page app recovered by Aletheia.
    pub js_content_ratio: f64,
    /// Whether the page's content was recovered from JS hydration state
    /// (Aletheia) rather than read from the DOM — a browser-free SPA render.
    pub recovered_from_state: bool,
}

/// The engine. Holds a shared handle to the renderer and shares the device
/// supervisor so its work shows up in the same live watch.
#[derive(Clone)]
pub struct DeepCrawlEngine {
    semantic_render: Shared<SemanticRenderEngine>,
    supervisor: DeviceSupervisor,
    search: SearxngClient,
}

impl DeepCrawlEngine {
    #[must_use]
    pub fn new(
        semantic_render: Shared<SemanticRenderEngine>,
        supervisor: DeviceSupervisor,
        search: SearxngClient,
    ) -> Self {
        Self {
            semantic_render,
            supervisor,
            search,
        }
    }

    /// Standalone discovery: turn a query into result URLs via SearXNG, without
    /// crawling. Exposed to agents as the `web_search` tool.
    pub fn web_search(&self, input: &Value) -> Result<Value, String> {
        let query = input
            .get("query")
            .and_then(Value::as_str)
            .or_else(|| input.as_str())
            .unwrap_or_default()
            .to_string();
        let count = input
            .get("count")
            .and_then(Value::as_u64)
            .map_or(10_usize, |n| n as usize);
        let results = self.search.search(&query, count)?;
        Ok(json!({
            "tool": "web_search",
            "instance": self.search.instance(),
            "query": query,
            "count": results.len(),
            "results": results,
        }))
    }

    /// Run a governed deep crawl. Input shape:
    /// `{ "urls": ["https://…"], "query": "…", "max_depth": 1, "max_pages": 8 }`
    /// (`url` singular is accepted too).
    pub fn execute(&self, input: &Value) -> Result<Value, String> {
        let query = input
            .get("query")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let max_pages = input
            .get("max_pages")
            .and_then(Value::as_u64)
            .map_or(DEFAULT_MAX_PAGES, |n| n as usize)
            .clamp(1, HARD_MAX_PAGES);

        // Discovery: if the caller supplied no seed URLs but did supply a query,
        // turn the question into starting URLs via SearXNG. This is what lets a
        // bare query drive the whole pipeline end to end.
        let mut seeds = collect_seed_urls(input);
        let mut seed_source = "provided";
        let mut discovery: Vec<Value> = Vec::new();
        if seeds.is_empty() {
            if query.trim().is_empty() {
                return Err(
                    "deep_crawl requires a seed 'url'/'urls' or a 'query' to search from".into(),
                );
            }
            let op_id = self.supervisor.begin("web_search", &query);
            match self.search.search(&query, max_pages) {
                Ok(results) => {
                    self.supervisor.finish(
                        &op_id,
                        "web_search",
                        &query,
                        "completed",
                        &format!("{} seed urls discovered", results.len()),
                    );
                    seeds = results.iter().map(|r| r.url.clone()).collect();
                    discovery = results
                        .iter()
                        .map(|r| json!({ "url": r.url, "title": r.title, "engine": r.engine }))
                        .collect();
                    seed_source = "searxng";
                }
                Err(reason) => {
                    self.supervisor
                        .finish(&op_id, "web_search", &query, "failed", &reason);
                    return Err(format!("discovery via SearXNG failed: {reason}"));
                }
            }
            if seeds.is_empty() {
                return Err(format!("SearXNG returned no results for query '{query}'"));
            }
        }
        let max_depth = input
            .get("max_depth")
            .and_then(Value::as_u64)
            .map_or(DEFAULT_MAX_DEPTH, |n| n as usize)
            .clamp(0, HARD_MAX_DEPTH);

        let trace_id = new_id("crawl_trace");
        let query_tokens = tokenize(&query);

        let mut frontier: VecDeque<(String, usize)> =
            seeds.into_iter().map(|url| (url, 0_usize)).collect();
        let mut visited: HashSet<String> = HashSet::new();
        let mut per_domain: std::collections::BTreeMap<String, usize> =
            std::collections::BTreeMap::new();
        let mut pages: Vec<CrawledPage> = Vec::new();
        let mut errors: Vec<Value> = Vec::new();

        while let Some((url, depth)) = frontier.pop_front() {
            if pages.len() >= max_pages {
                break;
            }
            if !visited.insert(url.clone()) {
                continue;
            }
            let domain = domain_of(&url);
            let count = per_domain.entry(domain.clone()).or_insert(0);
            if *count >= MAX_PAGES_PER_DOMAIN {
                continue;
            }

            let op_id = self.supervisor.begin("deep_crawl", &url);
            match self.crawl_one(&url, depth, &query_tokens) {
                Ok((page, links)) => {
                    *count += 1;
                    let summary = format!(
                        "{} rel={:.2} words={} proof={}",
                        page.url,
                        page.relevance,
                        page.word_count,
                        &page.render_proof_hash[..page.render_proof_hash.len().min(12)]
                    );
                    self.supervisor
                        .finish(&op_id, "deep_crawl", &url, "completed", &summary);
                    if depth < max_depth {
                        for link in links.into_iter().take(MAX_LINKS_ENQUEUED_PER_PAGE) {
                            if !visited.contains(&link) {
                                frontier.push_back((link, depth + 1));
                            }
                        }
                    }
                    pages.push(page);
                }
                Err(reason) => {
                    self.supervisor
                        .finish(&op_id, "deep_crawl", &url, "failed", &reason);
                    errors.push(json!({ "url": url, "error": reason }));
                }
            }
        }

        // Rank by relevance so the agent reads the strongest evidence first.
        pages.sort_by(|a, b| {
            b.relevance
                .partial_cmp(&a.relevance)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let top_findings: Vec<Value> = pages
            .iter()
            .take(5)
            .map(|p| {
                json!({
                    "url": p.url,
                    "title": p.title,
                    "relevance": p.relevance,
                    "excerpt": p.text_excerpt,
                })
            })
            .collect();

        // Aggregate the harvested records across all pages into one dataset —
        // the deep-collection payload: every structured entity + table found,
        // plus a histogram of schema.org types so an agent sees the shape.
        let mut all_entities: Vec<Value> = Vec::new();
        let mut all_tables: Vec<Value> = Vec::new();
        let mut type_histogram: std::collections::BTreeMap<String, u64> =
            std::collections::BTreeMap::new();
        for page in &pages {
            for entity in &page.structured.entities {
                *type_histogram
                    .entry(entity.entity_type.clone())
                    .or_insert(0) += 1;
                if all_entities.len() < 200 {
                    all_entities.push(json!({
                        "type": entity.entity_type,
                        "source": entity.source,
                        "from_url": page.url,
                        "fields": entity.fields,
                    }));
                }
            }
            for table in &page.structured.tables {
                if all_tables.len() < 50 {
                    all_tables.push(json!({
                        "from_url": page.url,
                        "caption": table.caption,
                        "headers": table.headers,
                        "row_count": table.row_count,
                        "rows": table.rows,
                    }));
                }
            }
        }
        let dataset = json!({
            "entity_count": all_entities.len(),
            "table_count": all_tables.len(),
            "entity_types": type_histogram,
            "entities": all_entities,
            "tables": all_tables,
        });

        // Surface the Aletheia recovery so an agent can see which pages were
        // JavaScript single-page apps rendered without a browser, and how
        // JS-heavy the crawl was overall.
        let js_recovered_pages = pages.iter().filter(|p| p.recovered_from_state).count();
        let peak_js_ratio = pages
            .iter()
            .map(|p| p.js_content_ratio)
            .fold(0.0_f64, f64::max);
        let js_recovery = json!({
            "pages_recovered_from_js_state": js_recovered_pages,
            "peak_js_content_ratio": peak_js_ratio,
            "note": "pages whose content lived only in JavaScript hydration state, \
                     recovered by the Aletheia engine without executing any JavaScript",
        });

        Ok(json!({
            "tool": "deep_crawl",
            "crawl_id": trace_id,
            "query": query,
            "seed_source": seed_source,
            "discovery": discovery,
            "pages_crawled": pages.len(),
            "errors": errors,
            "max_depth": max_depth,
            "max_pages": max_pages,
            "pages": pages,
            "top_findings": top_findings,
            "dataset": dataset,
            "js_recovery": js_recovery,
            "pipeline": if seed_source == "searxng" {
                "searxng_discovery -> safe_fetch -> semantic_render -> aletheia_recover -> harvest_structured -> link_crawl -> relevance_rank"
            } else {
                "safe_fetch -> semantic_render -> aletheia_recover -> harvest_structured -> link_crawl -> relevance_rank"
            },
        }))
    }

    /// Fetch, render, and score a single URL. Returns the page plus the links
    /// the renderer extracted (the next frontier).
    fn crawl_one(
        &self,
        url: &str,
        depth: usize,
        query_tokens: &[String],
    ) -> Result<(CrawledPage, Vec<String>), String> {
        // 1. Governed fetch — SSRF-blocked, bounded.
        let html = crate::nexus::safe_fetch::safe_fetch_html(url)
            .map_err(|e| format!("governed fetch failed: {e}"))?;
        if html.trim().is_empty() {
            return Err("fetched document was empty".into());
        }

        // 2. Semantic render — deep structured extraction + proof hash.
        let report = self
            .semantic_render
            .write()
            .render(SemanticRenderRequest {
                url: url.to_string(),
                html,
                content_type: "text/html".into(),
                notarize_to_chain: false,
            })
            .map_err(|e| format!("semantic render failed: {e}"))?;

        // 3. Relevance scoring against the query. On a JavaScript SPA the DOM
        // body is near-empty, so fold in the content Aletheia recovered from
        // the hydration state — otherwise a browser-rendered page scores zero.
        let recovered = &report.recovered;
        let dom_thin = report.word_count < 50;
        let recovered_from_state = dom_thin && recovered.recovered_chars > 0;
        let mut scored_text = report.text_excerpt.clone();
        for heading in &report.headings {
            scored_text.push(' ');
            scored_text.push_str(heading);
        }
        if let Some(title) = &report.title {
            scored_text.push(' ');
            scored_text.push_str(title);
        }
        if recovered_from_state {
            scored_text.push(' ');
            scored_text.push_str(&recovered.text);
        }
        let relevance = relevance_score(query_tokens, &scored_text);

        // Frontier order: state-derived continuations (browser-free "next page"
        // of a feed/listing) first, then DOM pagination, then outbound links —
        // so deep multi-page collection is followed before wandering off.
        let mut links: Vec<String> = Vec::new();
        for cont in &recovered.continuations {
            // Only URL-shaped continuations can be fetched directly; cursors and
            // offset params need an API endpoint the crawler doesn't infer here.
            if cont.kind == "next_url" {
                if let Ok(resolved) =
                    url::Url::parse(&report.normalized_url).and_then(|base| base.join(&cont.value))
                {
                    if matches!(resolved.scheme(), "http" | "https") {
                        let resolved = resolved.to_string();
                        if !links.contains(&resolved) {
                            links.push(resolved);
                        }
                    }
                }
            }
        }
        for link in &report.structured.pagination_links {
            if !links.contains(link) {
                links.push(link.clone());
            }
        }
        for link in &report.links {
            if !links.contains(link) {
                links.push(link.clone());
            }
        }
        // On a thin-DOM SPA, use the recovered content as the readable excerpt.
        let text_excerpt = if recovered_from_state {
            recovered.text.chars().take(480).collect()
        } else {
            report.text_excerpt.clone()
        };
        let word_count = if recovered_from_state {
            recovered.text.split_whitespace().count()
        } else {
            report.word_count
        };
        let page = CrawledPage {
            url: report.normalized_url.clone(),
            domain: domain_of(&report.normalized_url),
            depth,
            title: report.title.clone(),
            relevance,
            word_count,
            reading_time_minutes: report.reading_time_minutes,
            headings: report.headings.iter().take(12).cloned().collect(),
            outbound_links: report.links.len(),
            text_excerpt,
            render_proof_hash: report.proof_hash.clone(),
            content_hash: report.content_hash.clone(),
            script_count: report.script_count,
            iframe_count: report.iframe_count,
            inline_event_handlers: report.inline_event_handlers,
            structured: report.structured.clone(),
            js_content_ratio: recovered.js_content_ratio,
            recovered_from_state,
        };
        Ok((page, links))
    }
}

fn collect_seed_urls(input: &Value) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(array) = input.get("urls").and_then(Value::as_array) {
        for item in array {
            if let Some(url) = item.as_str() {
                push_http_url(&mut out, url);
            }
        }
    }
    for key in ["url", "seed", "target"] {
        if let Some(url) = input.get(key).and_then(Value::as_str) {
            push_http_url(&mut out, url);
        }
    }
    if let Value::String(url) = input {
        push_http_url(&mut out, url);
    }
    out
}

fn push_http_url(out: &mut Vec<String>, url: &str) {
    let url = url.trim();
    if (url.starts_with("http://") || url.starts_with("https://")) && !out.iter().any(|u| u == url)
    {
        out.push(url.to_string());
    }
}

fn domain_of(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(str::to_ascii_lowercase))
        .unwrap_or_default()
}

fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() > 2)
        .map(str::to_ascii_lowercase)
        .collect()
}

/// Fraction of distinct query tokens present in the text, lifted slightly by
/// how often they recur (capped). Deterministic, no randomness.
fn relevance_score(query_tokens: &[String], text: &str) -> f64 {
    if query_tokens.is_empty() {
        return 0.0;
    }
    let body = text.to_ascii_lowercase();
    let mut matched = 0_usize;
    let mut hits = 0_usize;
    let distinct: HashSet<&String> = query_tokens.iter().collect();
    for token in &distinct {
        let count = body.matches(token.as_str()).count();
        if count > 0 {
            matched += 1;
            hits += count;
        }
    }
    let coverage = matched as f64 / distinct.len() as f64;
    let density = (hits as f64 / 20.0).min(1.0);
    (coverage * 0.8 + density * 0.2).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relevance_rewards_coverage() {
        let q = tokenize("rust async runtime");
        let strong = relevance_score(&q, "The Rust async runtime schedules futures.");
        let weak = relevance_score(&q, "A recipe for banana bread.");
        assert!(strong > weak);
        assert!(strong > 0.5);
        assert_eq!(weak, 0.0);
    }

    #[test]
    fn seed_collection_accepts_array_and_singular() {
        let v = json!({ "urls": ["https://a.example"], "url": "http://b.example" });
        let seeds = collect_seed_urls(&v);
        assert_eq!(seeds.len(), 2);
        // Non-http schemes and dups are rejected.
        let v2 = json!({ "urls": ["ftp://x", "https://a.example", "https://a.example"] });
        assert_eq!(
            collect_seed_urls(&v2),
            vec!["https://a.example".to_string()]
        );
    }

    #[test]
    fn domain_extraction() {
        assert_eq!(domain_of("https://Docs.RS/tokio"), "docs.rs");
        assert_eq!(domain_of("not a url"), "");
    }
}
