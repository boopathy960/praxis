use std::collections::BTreeMap;
use std::net::IpAddr;
use std::sync::OnceLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::common::{AppError, now_ms, sha3_hex};

const MAX_RENDER_BYTES: usize = 2 * 1024 * 1024;
const MAX_CACHE_ENTRIES: usize = 256;
const RENDER_PROFILE: &str = "enterprise_agent_semantic_render_v2";
/// Average adult reading speed used for the reading-time estimate.
const WORDS_PER_MINUTE: usize = 220;

#[derive(Debug, Clone, Deserialize)]
pub struct SemanticRenderRequest {
    pub url: String,
    pub html: String,
    #[serde(default = "default_content_type")]
    pub content_type: String,
    #[serde(default)]
    pub notarize_to_chain: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticRenderReport {
    pub render_id: String,
    pub render_profile: String,
    pub proof_hash: String,
    pub url: String,
    pub normalized_url: String,
    pub content_type: String,
    pub content_hash: String,
    pub title: Option<String>,
    #[serde(default)]
    pub meta_description: Option<String>,
    #[serde(default)]
    pub canonical_url: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub open_graph: BTreeMap<String, String>,
    pub headings: Vec<String>,
    pub links: Vec<String>,
    #[serde(default)]
    pub text_excerpt: String,
    #[serde(default)]
    pub word_count: usize,
    #[serde(default)]
    pub reading_time_minutes: usize,
    #[serde(default)]
    pub image_count: usize,
    #[serde(default)]
    pub table_count: usize,
    #[serde(default)]
    pub json_ld_blocks: usize,
    /// Harvested structured data: typed schema.org entities, tables as records,
    /// and detected pagination links — the deep-collection payload agents read.
    #[serde(default)]
    pub structured: ExtractedData,
    /// Content recovered from JavaScript hydration state (Next.js/Nuxt/Apollo/…)
    /// by the Aletheia engine — what a JS-rendered ("needs a browser") page
    /// actually says, extracted without executing any JavaScript.
    #[serde(default)]
    pub recovered: RecoveredContent,
    pub forms: usize,
    pub script_count: usize,
    pub iframe_count: usize,
    pub inline_event_handlers: usize,
    pub warnings: Vec<String>,
    #[serde(default)]
    pub from_cache: bool,
    pub rendered_at_ms: i64,
}

/// One structured record harvested from a page: a schema.org JSON-LD node, an
/// OpenGraph object, or a microdata item. `fields` keeps the raw typed values
/// (price, rating, author, date, …) so an agent gets a record, not prose.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StructuredEntity {
    /// schema.org @type (e.g. "Product", "Article", "ItemList") or "opengraph".
    pub entity_type: String,
    /// Where it came from: "json-ld" | "opengraph".
    pub source: String,
    pub fields: BTreeMap<String, serde_json::Value>,
}

/// A `<table>` extracted as structured records: header row + data rows.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StructuredTable {
    #[serde(default)]
    pub caption: Option<String>,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub row_count: usize,
}

/// Everything machine-readable harvested from a page. This is the deep
/// data-collection payload: agents read `entities`/`tables` to build datasets,
/// and the crawler follows `pagination_links` to collect across pages.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtractedData {
    pub entities: Vec<StructuredEntity>,
    pub tables: Vec<StructuredTable>,
    pub entity_count: usize,
    pub table_count: usize,
    /// Distinct schema.org @types seen (e.g. ["Product","Offer","BreadcrumbList"]).
    pub entity_types: Vec<String>,
    /// Detected "next page" URLs for deep multi-page collection.
    pub pagination_links: Vec<String>,
}

/// Content the Aletheia engine recovered from a page's JS hydration state — the
/// body of a single-page app that a plain DOM read would see as blank, plus the
/// pagination cursors that let a crawler follow the feed with no browser.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RecoveredContent {
    /// Reconstructed page text, from the state the framework would have rendered.
    #[serde(default)]
    pub text: String,
    /// Strongest labelled content leaves (title, author, price, …) with paths.
    #[serde(default)]
    pub fields: Vec<crate::aletheia::RecoveredField>,
    /// Continuation tokens (Relay cursors, next URLs, offset params) for
    /// browser-free deep crawling of listings and infinite feeds.
    #[serde(default)]
    pub continuations: Vec<crate::aletheia::Continuation>,
    /// How many JSON state blobs were parsed out of the page.
    #[serde(default)]
    pub state_blobs: usize,
    /// Characters of content recovered from state.
    #[serde(default)]
    pub recovered_chars: usize,
    /// Share of the page's meaning that lived only in JS state, in `[0,1]`:
    /// ~0 for a static page, ~1 for a pure SPA whose DOM body is empty.
    #[serde(default)]
    pub js_content_ratio: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SemanticRenderStats {
    pub total_renders: u64,
    pub notarized_renders: u64,
    pub cache_hits: u64,
    pub last_rendered_at_ms: i64,
}

#[derive(Debug, Default, Clone)]
pub struct SemanticRenderEngine {
    total_renders: u64,
    notarized_renders: u64,
    cache_hits: u64,
    last_rendered_at_ms: i64,
    /// Reports keyed by `(normalized_url, content_hash)` so re-rendering
    /// unchanged content is free and returns an identical proof hash.
    cache: BTreeMap<String, SemanticRenderReport>,
}

impl SemanticRenderEngine {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn render(
        &mut self,
        request: SemanticRenderRequest,
    ) -> Result<SemanticRenderReport, AppError> {
        if request.url.trim().is_empty() {
            return Err(AppError::Validation("render URL must not be empty".into()));
        }
        if request.html.trim().is_empty() {
            return Err(AppError::Validation("render HTML must not be empty".into()));
        }
        if request.html.len() > MAX_RENDER_BYTES {
            return Err(AppError::Validation(format!(
                "render HTML exceeds {MAX_RENDER_BYTES} bytes"
            )));
        }

        let normalized_url = Url::parse(request.url.trim())
            .map_err(|error| AppError::Validation(format!("invalid render URL: {error}")))?;
        if !matches!(normalized_url.scheme(), "http" | "https") {
            return Err(AppError::Validation(
                "semantic render supports only http/https URLs".into(),
            ));
        }
        let host = normalized_url.host_str().unwrap_or_default();
        if is_private_host(host) {
            return Err(AppError::Forbidden(
                "private or loopback render URLs are blocked".into(),
            ));
        }

        let content_hash = sha3_hex(request.html.as_bytes());
        let cache_key = format!("{}:{content_hash}", normalized_url);
        if let Some(cached) = self.cache.get(&cache_key) {
            self.cache_hits += 1;
            let mut report = cached.clone();
            report.from_cache = true;
            return Ok(report);
        }

        let rendered_at_ms = now_ms();
        let render_id = sha3_hex(
            format!(
                "{}:{content_hash}:{rendered_at_ms}",
                normalized_url.as_str()
            )
            .as_bytes(),
        )[..24]
            .to_string();

        let html = &request.html;
        let script_count = regex_script().find_iter(html).count();
        let iframe_count = regex_iframe().find_iter(html).count();
        let inline_event_handlers = regex_event_handler().find_iter(html).count();
        let forms = regex_form().find_iter(html).count();
        let image_count = regex_img().find_iter(html).count();
        let table_count = regex_table().find_iter(html).count();
        let json_ld_blocks = regex_json_ld().find_iter(html).count();

        let mut warnings = Vec::new();
        if script_count > 0 {
            warnings
                .push("html contains script tags; render proof is semantic extraction only".into());
        }
        if iframe_count > 0 {
            warnings.push("html contains iframes; embedded content is not fetched".into());
        }
        if inline_event_handlers > 0 {
            warnings.push("html contains inline event handlers".into());
        }

        let title = extract_title(html);
        let meta_description = extract_meta_content(html, "description");
        let canonical_url = extract_canonical(html, &normalized_url);
        let language = extract_language(html);
        let open_graph = extract_open_graph(html);
        let headings = extract_headings(html);
        let links = extract_links(html, &normalized_url);
        let body_text = extract_body_text(html);
        let word_count = body_text.split_whitespace().count();
        let reading_time_minutes = word_count.div_ceil(WORDS_PER_MINUTE).max(1);
        let text_excerpt: String = body_text.chars().take(480).collect();
        let structured = extract_structured(html, &normalized_url, &open_graph);

        // Aletheia: recover content from JS hydration state (Next.js/Nuxt/Apollo/
        // Redux/embedded JSON) with no browser. This is what lets the engine
        // "render" a single-page app whose DOM body is otherwise empty.
        let state_blobs = extract_state_blobs(html);
        let recovery = crate::aletheia::recover(&state_blobs);
        let js_content_ratio =
            crate::aletheia::js_content_ratio(body_text.len(), recovery.recovered_chars);
        if recovery.recovered_chars > 0 && word_count < 50 {
            warnings.push(format!(
                "sparse DOM ({word_count} words) but {} chars recovered from JS state — \
                 page is a single-page app; using Aletheia-recovered content",
                recovery.recovered_chars
            ));
        }
        let recovered = RecoveredContent {
            text: recovery.recovered_text,
            fields: recovery.recovered_fields,
            continuations: recovery.continuations,
            state_blobs: recovery.state_blobs,
            recovered_chars: recovery.recovered_chars,
            js_content_ratio,
        };

        let proof_hash = sha3_hex(
            serde_json::json!({
                "render_profile": RENDER_PROFILE,
                "normalized_url": normalized_url.to_string(),
                "content_type": request.content_type.clone(),
                "content_hash": content_hash,
                "title": title,
                "meta_description": meta_description,
                "canonical_url": canonical_url,
                "language": language,
                "open_graph": open_graph,
                "headings": headings,
                "links": links,
                "word_count": word_count,
                "image_count": image_count,
                "table_count": table_count,
                "json_ld_blocks": json_ld_blocks,
                "forms": forms,
                "script_count": script_count,
                "iframe_count": iframe_count,
                "inline_event_handlers": inline_event_handlers,
                "warnings": warnings,
                // The recovered content is part of the attested extraction, so
                // its digest binds into the proof hash too.
                "recovered_chars": recovered.recovered_chars,
                "recovered_state_blobs": recovered.state_blobs,
                "recovered_text_hash": sha3_hex(recovered.text.as_bytes()),
                "recovered_continuations": recovered.continuations.len(),
            })
            .to_string()
            .as_bytes(),
        );

        self.total_renders += 1;
        self.last_rendered_at_ms = rendered_at_ms;

        let report = SemanticRenderReport {
            render_id,
            render_profile: RENDER_PROFILE.into(),
            proof_hash,
            url: request.url,
            normalized_url: normalized_url.to_string(),
            content_type: request.content_type,
            content_hash,
            title,
            meta_description,
            canonical_url,
            language,
            open_graph,
            headings,
            links,
            text_excerpt,
            word_count,
            reading_time_minutes,
            image_count,
            table_count,
            json_ld_blocks,
            structured,
            recovered,
            forms,
            script_count,
            iframe_count,
            inline_event_handlers,
            warnings,
            from_cache: false,
            rendered_at_ms,
        };
        if self.cache.len() >= MAX_CACHE_ENTRIES {
            // BTreeMap pops in key order; good enough as a bounded eviction.
            let oldest = self.cache.keys().next().cloned();
            if let Some(key) = oldest {
                self.cache.remove(&key);
            }
        }
        self.cache.insert(cache_key, report.clone());
        Ok(report)
    }

    pub fn record_chain_commit(&mut self) {
        self.notarized_renders += 1;
    }

    #[must_use]
    pub fn stats(&self) -> SemanticRenderStats {
        SemanticRenderStats {
            total_renders: self.total_renders,
            notarized_renders: self.notarized_renders,
            cache_hits: self.cache_hits,
            last_rendered_at_ms: self.last_rendered_at_ms,
        }
    }
}

fn default_content_type() -> String {
    "text/html".into()
}

// ═══════════════════════════════════════════════════════════════
// Precompiled extraction patterns
// ═══════════════════════════════════════════════════════════════

macro_rules! static_regex {
    ($name:ident, $pattern:literal) => {
        fn $name() -> &'static Regex {
            static CELL: OnceLock<Regex> = OnceLock::new();
            CELL.get_or_init(|| Regex::new($pattern).expect("static regex should compile"))
        }
    };
}

static_regex!(regex_script, r#"(?is)<script\b"#);
static_regex!(regex_iframe, r#"(?is)<iframe\b"#);
static_regex!(regex_event_handler, r#"(?i)\son[a-z]+\s*="#);
static_regex!(regex_form, r#"(?is)<form\b"#);
static_regex!(regex_img, r#"(?is)<img\b"#);
static_regex!(regex_table, r#"(?is)<table\b"#);
static_regex!(
    regex_json_ld,
    r#"(?is)<script[^>]*type\s*=\s*["']application/ld\+json["']"#
);
static_regex!(regex_title, r#"(?is)<title[^>]*>(.*?)</title>"#);
static_regex!(regex_heading, r#"(?is)<h[1-6][^>]*>(.*?)</h[1-6]>"#);
static_regex!(
    regex_anchor,
    r#"(?is)<a\b[^>]*href\s*=\s*["']([^"']+)["'][^>]*>"#
);
static_regex!(regex_tag, r#"(?is)<[^>]+>"#);
static_regex!(regex_meta, r#"(?is)<meta\b[^>]*>"#);
static_regex!(regex_attr_name, r#"(?is)name\s*=\s*["']([^"']+)["']"#);
static_regex!(
    regex_attr_property,
    r#"(?is)property\s*=\s*["']([^"']+)["']"#
);
static_regex!(regex_attr_content, r#"(?is)content\s*=\s*["']([^"']*)["']"#);
static_regex!(
    regex_canonical,
    r#"(?is)<link\b[^>]*rel\s*=\s*["']canonical["'][^>]*href\s*=\s*["']([^"']+)["']"#
);
static_regex!(
    regex_html_lang,
    r#"(?is)<html\b[^>]*lang\s*=\s*["']([^"']+)["']"#
);
static_regex!(
    regex_strip_script_blocks,
    r#"(?is)<(script|style|noscript)\b[^>]*>.*?</(script|style|noscript)>"#
);
static_regex!(
    regex_json_ld_block,
    r#"(?is)<script[^>]*type\s*=\s*["']application/ld\+json["'][^>]*>(.*?)</script>"#
);
static_regex!(regex_table_block, r#"(?is)<table\b[^>]*>(.*?)</table>"#);
static_regex!(regex_table_row, r#"(?is)<tr\b[^>]*>(.*?)</tr>"#);
static_regex!(regex_table_cell, r#"(?is)<t[hd]\b[^>]*>(.*?)</t[hd]>"#);
static_regex!(regex_caption, r#"(?is)<caption\b[^>]*>(.*?)</caption>"#);
static_regex!(
    regex_link_next,
    r#"(?is)<link\b[^>]*rel\s*=\s*["']next["'][^>]*href\s*=\s*["']([^"']+)["']"#
);
static_regex!(
    regex_anchor_text,
    r#"(?is)<a\b[^>]*href\s*=\s*["']([^"']+)["'][^>]*>(.*?)</a>"#
);
// JSON hydration state carriers (Aletheia input). `<script type="application/
// json">…</script>` covers Next.js (__NEXT_DATA__), Remix, SvelteKit, schema
// blocks, and most modern frameworks; the assignment form covers Nuxt, Apollo,
// and Redux, which attach state to a `window.__X__ = {…}` global.
static_regex!(
    regex_json_script,
    r#"(?is)<script[^>]*type\s*=\s*["']application/json["'][^>]*>(.*?)</script>"#
);
static_regex!(
    regex_state_assign,
    r#"(?is)<script[^>]*>\s*(?:window\.)?(?:__NUXT__|__APOLLO_STATE__|__PRELOADED_STATE__|__INITIAL_STATE__|__NEXT_DATA__|__remixContext|__sveltekit[a-z0-9_]*)\s*=\s*(\{.*?\})\s*;?\s*</script>"#
);

fn extract_title(html: &str) -> Option<String> {
    regex_title()
        .captures(html)
        .and_then(|captures| captures.get(1))
        .map(|value| strip_tags(value.as_str()))
        .filter(|value| !value.is_empty())
}

fn extract_headings(html: &str) -> Vec<String> {
    regex_heading()
        .captures_iter(html)
        .filter_map(|captures| captures.get(1))
        .map(|value| strip_tags(value.as_str()))
        .filter(|value| !value.is_empty())
        .take(24)
        .collect()
}

fn extract_links(html: &str, base_url: &Url) -> Vec<String> {
    regex_anchor()
        .captures_iter(html)
        .filter_map(|captures| captures.get(1))
        .filter_map(|value| base_url.join(value.as_str()).ok())
        .filter(|url| matches!(url.scheme(), "http" | "https"))
        .map(|url| url.to_string())
        .take(48)
        .collect()
}

// ── Structured-data harvesting (deep data collection) ────────────
//
// Turns HTML into machine-readable records, not prose: schema.org JSON-LD
// entities, OpenGraph objects, `<table>`s as header+rows, and pagination
// links. This is what lets an agent collect a dataset across pages instead of
// re-summarizing text every hop.

const MAX_ENTITIES: usize = 60;
const MAX_ENTITY_FIELDS: usize = 40;
const MAX_TABLES: usize = 25;
const MAX_TABLE_ROWS: usize = 200;
const MAX_PAGINATION: usize = 5;

fn extract_structured(
    html: &str,
    base_url: &Url,
    open_graph: &BTreeMap<String, String>,
) -> ExtractedData {
    let mut entities = extract_json_ld_entities(html);
    if let Some(og) = entity_from_open_graph(open_graph) {
        entities.push(og);
    }
    let tables = extract_tables(html);
    let pagination_links = extract_pagination_links(html, base_url);
    let mut entity_types: Vec<String> = entities
        .iter()
        .map(|entity| entity.entity_type.clone())
        .collect();
    entity_types.sort();
    entity_types.dedup();
    ExtractedData {
        entity_count: entities.len(),
        table_count: tables.len(),
        entity_types,
        entities,
        tables,
        pagination_links,
    }
}

fn extract_json_ld_entities(html: &str) -> Vec<StructuredEntity> {
    let mut out = Vec::new();
    for caps in regex_json_ld_block().captures_iter(html) {
        if out.len() >= MAX_ENTITIES {
            break;
        }
        let Some(raw) = caps.get(1) else { continue };
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(raw.as_str().trim()) {
            collect_jsonld_entities(&value, 0, &mut out);
        }
    }
    out.truncate(MAX_ENTITIES);
    out
}

fn collect_jsonld_entities(
    value: &serde_json::Value,
    depth: usize,
    out: &mut Vec<StructuredEntity>,
) {
    if out.len() >= MAX_ENTITIES || depth > 6 {
        return;
    }
    match value {
        serde_json::Value::Array(items) => {
            for item in items {
                collect_jsonld_entities(item, depth, out);
            }
        }
        serde_json::Value::Object(map) => {
            if let Some(graph) = map.get("@graph") {
                collect_jsonld_entities(graph, depth + 1, out);
            }
            if let Some(type_value) = map.get("@type") {
                let entity_type = jsonld_type_string(type_value);
                if !entity_type.is_empty() {
                    let mut fields = BTreeMap::new();
                    for (key, val) in map {
                        if key.starts_with('@') || fields.len() >= MAX_ENTITY_FIELDS {
                            continue;
                        }
                        fields.insert(key.clone(), simplify_jsonld_value(val));
                    }
                    out.push(StructuredEntity {
                        entity_type,
                        source: "json-ld".into(),
                        fields,
                    });
                }
            }
            // Recurse into nested objects/arrays (offers, itemListElement,
            // author, …) to capture embedded entities — but never back into
            // @graph (already handled above).
            for (key, val) in map {
                if key == "@graph" || key.starts_with('@') {
                    continue;
                }
                if val.is_array() || val.is_object() {
                    collect_jsonld_entities(val, depth + 1, out);
                }
            }
        }
        _ => {}
    }
}

fn jsonld_type_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(items) => items
            .iter()
            .find_map(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string(),
        _ => String::new(),
    }
}

fn simplify_jsonld_value(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::String(s) => serde_json::Value::String(s.chars().take(600).collect()),
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.iter().take(25).map(simplify_jsonld_value).collect())
        }
        serde_json::Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (key, val) in map.iter().take(30) {
                out.insert(key.clone(), simplify_jsonld_value(val));
            }
            serde_json::Value::Object(out)
        }
        other => other.clone(),
    }
}

fn entity_from_open_graph(open_graph: &BTreeMap<String, String>) -> Option<StructuredEntity> {
    if open_graph.is_empty() {
        return None;
    }
    let entity_type = open_graph
        .get("type")
        .cloned()
        .unwrap_or_else(|| "opengraph".into());
    let mut fields = BTreeMap::new();
    for (key, val) in open_graph {
        fields.insert(
            key.clone(),
            serde_json::Value::String(val.chars().take(600).collect()),
        );
    }
    Some(StructuredEntity {
        entity_type,
        source: "opengraph".into(),
        fields,
    })
}

fn extract_tables(html: &str) -> Vec<StructuredTable> {
    let mut out = Vec::new();
    for table_caps in regex_table_block().captures_iter(html) {
        if out.len() >= MAX_TABLES {
            break;
        }
        let Some(inner) = table_caps.get(1).map(|m| m.as_str()) else {
            continue;
        };
        let caption = regex_caption()
            .captures(inner)
            .and_then(|c| c.get(1))
            .map(|m| strip_tags(m.as_str()))
            .filter(|s| !s.is_empty());
        let mut headers: Vec<String> = Vec::new();
        let mut rows: Vec<Vec<String>> = Vec::new();
        for row_caps in regex_table_row().captures_iter(inner) {
            if rows.len() >= MAX_TABLE_ROWS {
                break;
            }
            let Some(row_html) = row_caps.get(1).map(|m| m.as_str()) else {
                continue;
            };
            let cells: Vec<String> = regex_table_cell()
                .captures_iter(row_html)
                .filter_map(|c| c.get(1))
                .map(|m| strip_tags(m.as_str()))
                .collect();
            if cells.is_empty() {
                continue;
            }
            if headers.is_empty() && row_html.to_ascii_lowercase().contains("<th") {
                headers = cells;
            } else {
                rows.push(cells);
            }
        }
        // No <th> header row: promote the first data row to headers.
        if headers.is_empty() && !rows.is_empty() {
            headers = rows.remove(0);
        }
        if headers.is_empty() && rows.is_empty() {
            continue;
        }
        let row_count = rows.len();
        out.push(StructuredTable {
            caption,
            headers,
            rows,
            row_count,
        });
    }
    out
}

fn extract_pagination_links(html: &str, base_url: &Url) -> Vec<String> {
    let mut candidates: Vec<String> = Vec::new();
    for caps in regex_link_next().captures_iter(html) {
        if let Some(href) = caps.get(1) {
            candidates.push(href.as_str().to_string());
        }
    }
    for caps in regex_anchor_text().captures_iter(html) {
        if candidates.len() >= 24 {
            break;
        }
        let Some(href) = caps.get(1).map(|m| m.as_str()) else {
            continue;
        };
        if href.is_empty() {
            continue;
        }
        let text = caps
            .get(2)
            .map(|m| strip_tags(m.as_str()).to_ascii_lowercase())
            .unwrap_or_default();
        let href_lower = href.to_ascii_lowercase();
        let looks_next = text == "next"
            || text.starts_with("next")
            || text.contains('›')
            || text.contains('»')
            || text.contains('→')
            || href_lower.contains("page=")
            || href_lower.contains("/page/")
            || href_lower.contains("?p=")
            || href_lower.contains("&p=");
        if looks_next {
            candidates.push(href.to_string());
        }
    }
    let mut out: Vec<String> = Vec::new();
    for raw in candidates {
        if out.len() >= MAX_PAGINATION {
            break;
        }
        if let Ok(url) = base_url.join(&raw) {
            if matches!(url.scheme(), "http" | "https") {
                let resolved = url.to_string();
                if !out.contains(&resolved) {
                    out.push(resolved);
                }
            }
        }
    }
    out
}

/// Pull parseable JSON hydration-state blobs out of the page for Aletheia:
/// every `<script type="application/json">` payload and every `window.__X__ =
/// {…}` framework-state assignment. JSON-LD blocks are handled separately by
/// the structured harvester, so they are skipped here. Bounded so a hostile
/// page can't force unbounded parsing.
fn extract_state_blobs(html: &str) -> Vec<serde_json::Value> {
    const MAX_BLOBS: usize = 12;
    const MAX_BLOB_BYTES: usize = 1_500_000;
    let mut blobs = Vec::new();
    let mut push = |raw: &str, blobs: &mut Vec<serde_json::Value>| {
        let raw = raw.trim();
        if raw.len() > MAX_BLOB_BYTES || raw.is_empty() {
            return;
        }
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) {
            if value.is_object() || value.is_array() {
                blobs.push(value);
            }
        }
    };
    for caps in regex_json_script().captures_iter(html) {
        if blobs.len() >= MAX_BLOBS {
            break;
        }
        // Skip JSON-LD (application/ld+json), harvested by extract_structured.
        let whole = caps.get(0).map(|m| m.as_str()).unwrap_or_default();
        if whole.contains("ld+json") {
            continue;
        }
        if let Some(body) = caps.get(1) {
            push(body.as_str(), &mut blobs);
        }
    }
    for caps in regex_state_assign().captures_iter(html) {
        if blobs.len() >= MAX_BLOBS {
            break;
        }
        if let Some(body) = caps.get(1) {
            push(body.as_str(), &mut blobs);
        }
    }
    blobs
}

fn extract_meta_content(html: &str, name: &str) -> Option<String> {
    for tag in regex_meta().find_iter(html) {
        let tag = tag.as_str();
        let matches_name = regex_attr_name()
            .captures(tag)
            .and_then(|captures| captures.get(1))
            .is_some_and(|value| value.as_str().eq_ignore_ascii_case(name));
        if matches_name {
            return regex_attr_content()
                .captures(tag)
                .and_then(|captures| captures.get(1))
                .map(|value| strip_tags(value.as_str()))
                .filter(|value| !value.is_empty());
        }
    }
    None
}

fn extract_open_graph(html: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for tag in regex_meta().find_iter(html) {
        let tag = tag.as_str();
        let Some(property) = regex_attr_property()
            .captures(tag)
            .and_then(|captures| captures.get(1))
            .map(|value| value.as_str().to_ascii_lowercase())
        else {
            continue;
        };
        if !property.starts_with("og:") || out.len() >= 16 {
            continue;
        }
        if let Some(content) = regex_attr_content()
            .captures(tag)
            .and_then(|captures| captures.get(1))
            .map(|value| strip_tags(value.as_str()))
            .filter(|value| !value.is_empty())
        {
            out.entry(property).or_insert(content);
        }
    }
    out
}

fn extract_canonical(html: &str, base_url: &Url) -> Option<String> {
    regex_canonical()
        .captures(html)
        .and_then(|captures| captures.get(1))
        .and_then(|value| base_url.join(value.as_str()).ok())
        .map(|url| url.to_string())
}

fn extract_language(html: &str) -> Option<String> {
    regex_html_lang()
        .captures(html)
        .and_then(|captures| captures.get(1))
        .map(|value| value.as_str().trim().to_string())
        .filter(|value| !value.is_empty())
}

/// Visible body text with script/style/noscript blocks removed.
fn extract_body_text(html: &str) -> String {
    let without_blocks = regex_strip_script_blocks().replace_all(html, " ");
    strip_tags(&without_blocks)
}

fn strip_tags(raw: &str) -> String {
    let without_tags = regex_tag().replace_all(raw, " ");
    without_tags
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Blocks loopback, RFC1918, link-local, CGNAT, unspecified, and IPv6
/// private ranges — the SSRF guard shared by every fetch path.
pub fn is_private_host(host: &str) -> bool {
    if matches!(host, "localhost" | "127.0.0.1" | "::1" | "0.0.0.0") {
        return true;
    }
    host.parse::<IpAddr>().ok().is_some_and(|ip| match ip {
        IpAddr::V4(addr) => {
            let octets = addr.octets();
            addr.is_loopback()
                || addr.is_unspecified()
                || addr.is_broadcast()
                || octets[0] == 10
                || (octets[0] == 172 && (16..=31).contains(&octets[1]))
                || (octets[0] == 192 && octets[1] == 168)
                || (octets[0] == 169 && octets[1] == 254)
                || (octets[0] == 100 && (64..=127).contains(&octets[1]))
        }
        IpAddr::V6(addr) => {
            addr.is_loopback()
                || addr.is_unspecified()
                || addr.is_unique_local()
                || addr.is_unicast_link_local()
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = r#"<html lang="en"><head>
        <title>Hello</title>
        <meta name="description" content="A test page about rendering.">
        <meta property="og:title" content="Hello OG">
        <meta property="og:type" content="article">
        <link rel="canonical" href="https://example.com/canonical">
        <script type="application/ld+json">{"@type":"Article"}</script>
    </head><body>
        <h1>World</h1>
        <p>Readable body copy for the semantic extraction test.</p>
        <img src="/a.png"><table><tr><td>x</td></tr></table>
        <a href="/x">x</a>
    </body></html>"#;

    #[test]
    fn semantic_render_hashes_and_extracts_html() {
        let mut engine = SemanticRenderEngine::new();
        let report = engine
            .render(SemanticRenderRequest {
                url: "https://example.com/path".into(),
                html: "<html><head><title>Hello</title></head><body><h1>World</h1><a href=\"/x\">x</a></body></html>".into(),
                content_type: "text/html".into(),
                notarize_to_chain: false,
            })
            .expect("render should succeed");
        assert_eq!(report.title.as_deref(), Some("Hello"));
        assert_eq!(report.headings, vec!["World"]);
        assert_eq!(report.links, vec!["https://example.com/x"]);
        assert_eq!(report.content_hash.len(), 64);
        assert_eq!(report.proof_hash.len(), 64);
    }

    #[test]
    fn semantic_render_extracts_rich_metadata() {
        let mut engine = SemanticRenderEngine::new();
        let report = engine
            .render(SemanticRenderRequest {
                url: "https://example.com/article".into(),
                html: PAGE.into(),
                content_type: "text/html".into(),
                notarize_to_chain: false,
            })
            .expect("render should succeed");
        assert_eq!(
            report.meta_description.as_deref(),
            Some("A test page about rendering.")
        );
        assert_eq!(
            report.canonical_url.as_deref(),
            Some("https://example.com/canonical")
        );
        assert_eq!(report.language.as_deref(), Some("en"));
        assert_eq!(
            report.open_graph.get("og:title").map(String::as_str),
            Some("Hello OG")
        );
        assert_eq!(
            report.open_graph.get("og:type").map(String::as_str),
            Some("article")
        );
        assert_eq!(report.json_ld_blocks, 1);
        assert_eq!(report.image_count, 1);
        assert_eq!(report.table_count, 1);
        assert!(report.word_count > 5);
        assert!(report.reading_time_minutes >= 1);
        assert!(report.text_excerpt.contains("Readable body copy"));
        // JSON-LD payload must not leak into visible text.
        assert!(!report.text_excerpt.contains("@type"));
    }

    #[test]
    fn harvests_structured_entities_tables_and_pagination() {
        let mut engine = SemanticRenderEngine::new();
        let html = r##"<html><head>
          <script type="application/ld+json">
          {"@context":"https://schema.org","@type":"Product","name":"Acme Laptop",
           "brand":"Acme","offers":{"@type":"Offer","price":"799.00","priceCurrency":"USD"},
           "aggregateRating":{"@type":"AggregateRating","ratingValue":"4.4","reviewCount":"120"}}
          </script>
          <link rel="next" href="/products?page=2">
          </head><body>
          <table>
            <tr><th>Model</th><th>Price</th></tr>
            <tr><td>A1</td><td>799</td></tr>
            <tr><td>A2</td><td>899</td></tr>
          </table>
          </body></html>"##;
        let report = engine
            .render(SemanticRenderRequest {
                url: "https://shop.example/products".into(),
                html: html.into(),
                content_type: "text/html".into(),
                notarize_to_chain: false,
            })
            .expect("render");
        let structured = &report.structured;
        // Product plus its nested Offer and AggregateRating become entities.
        assert!(structured.entity_types.contains(&"Product".to_string()));
        assert!(structured.entity_types.contains(&"Offer".to_string()));
        let product = structured
            .entities
            .iter()
            .find(|e| e.entity_type == "Product")
            .expect("product entity");
        assert_eq!(
            product.fields.get("name").and_then(|v| v.as_str()),
            Some("Acme Laptop")
        );
        assert!(product.fields.get("offers").is_some());
        // The table is parsed into a header row + 2 data rows.
        assert_eq!(structured.tables.len(), 1);
        assert_eq!(structured.tables[0].headers, vec!["Model", "Price"]);
        assert_eq!(structured.tables[0].row_count, 2);
        assert_eq!(structured.tables[0].rows[0], vec!["A1", "799"]);
        // The next-page link is detected for deep multi-page collection.
        assert!(
            structured
                .pagination_links
                .iter()
                .any(|l| l.contains("page=2"))
        );
    }

    #[test]
    fn semantic_render_caches_identical_content() {
        let mut engine = SemanticRenderEngine::new();
        let request = || SemanticRenderRequest {
            url: "https://example.com/article".into(),
            html: PAGE.into(),
            content_type: "text/html".into(),
            notarize_to_chain: false,
        };
        let first = engine.render(request()).expect("first render");
        let second = engine.render(request()).expect("second render");
        assert!(!first.from_cache);
        assert!(second.from_cache);
        assert_eq!(first.proof_hash, second.proof_hash);
        assert_eq!(engine.stats().cache_hits, 1);
        assert_eq!(engine.stats().total_renders, 1);
    }

    #[test]
    fn semantic_render_blocks_private_hosts() {
        let mut engine = SemanticRenderEngine::new();
        let error = engine
            .render(SemanticRenderRequest {
                url: "http://127.0.0.1/admin".into(),
                html: "<html>private</html>".into(),
                content_type: "text/html".into(),
                notarize_to_chain: false,
            })
            .expect_err("private render URL should be blocked");

        assert!(error.to_string().contains("private or loopback"));
    }

    #[test]
    fn private_host_guard_covers_extended_ranges() {
        for host in [
            "169.254.169.254", // cloud metadata
            "100.64.0.1",      // CGNAT
            "0.0.0.0",
            "10.1.2.3",
            "172.20.0.1",
            "192.168.1.1",
            "fe80::1",
            "fd00::1",
        ] {
            assert!(is_private_host(host), "{host} should be private");
        }
        for host in ["93.184.216.34", "example.com", "8.8.8.8"] {
            assert!(!is_private_host(host), "{host} should be public");
        }
    }

    #[test]
    fn renders_a_javascript_spa_from_hydration_state_without_js() {
        // A single-page app: the DOM body is an empty mount point, and all the
        // content lives in the Next.js __NEXT_DATA__ state blob. A plain DOM
        // read sees nothing; Aletheia recovers the article.
        let mut engine = SemanticRenderEngine::new();
        let html = r##"<html lang="en"><head><title>App</title></head><body>
          <div id="__next"></div>
          <script id="__NEXT_DATA__" type="application/json">
          {"props":{"pageProps":{"post":{
             "headline":"Recovering Content Without a Browser",
             "body":"This article was delivered as embedded hydration state, not as server rendered html, yet it was extracted with no javascript engine at all.",
             "author":{"name":"Grace Hopper","id":"usr_2213"},
             "__typename":"Post"}}},
           "buildId":"b8f0a1c2d3e4"}
          </script>
          <script type="application/json">
          {"feed":{"pageInfo":{"hasNextPage":true,"endCursor":"Y3Vyc29yOjQw"}}}
          </script>
        </body></html>"##;
        let report = engine
            .render(SemanticRenderRequest {
                url: "https://spa.example/post".into(),
                html: html.into(),
                content_type: "text/html".into(),
                notarize_to_chain: false,
            })
            .expect("render should succeed");

        // DOM body is essentially empty, but content was recovered from state.
        assert!(report.word_count < 20, "DOM body should be near-empty");
        assert!(
            report.recovered.text.contains("embedded hydration state"),
            "recovered text: {}",
            report.recovered.text
        );
        assert!(report.recovered.recovered_chars > 80);
        assert_eq!(report.recovered.state_blobs, 2);
        // It's flagged as a JS-heavy page: nearly all meaning came from state.
        assert!(report.recovered.js_content_ratio > 0.6);
        // The pagination cursor is available for browser-free deep crawling…
        assert!(
            report
                .recovered
                .continuations
                .iter()
                .any(|c| c.kind == "relay_cursor" && c.value == "Y3Vyc29yOjQw")
        );
        // …and plumbing (build id, internal user id) is not treated as content.
        assert!(!report.recovered.text.contains("b8f0a1c2d3e4"));
        assert!(!report.recovered.text.contains("usr_2213"));
    }
}
