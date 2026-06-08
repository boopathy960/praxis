use std::collections::{BTreeMap, BTreeSet};

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;

use crate::crypto::hash::sha3_256_hex;
use crate::features::semantic_acquisition::{
    SemanticAcquisitionOutcome, SemanticAcquisitionReport, SemanticAcquisitionRequest,
};
use crate::features::semantic_render::SemanticLink;

const DEFAULT_WORKFLOW_MAX_PAGES: usize = 4;
const MAX_TABLES: usize = 4;
const MAX_TABLE_ROWS: usize = 16;
const MAX_COLLECTION_ITEMS: usize = 16;
const MAX_PAGINATION_CANDIDATES: usize = 8;

#[derive(Debug, Clone, Deserialize)]
pub struct SemanticWorkflowRequest {
    pub seed_urls: Vec<String>,
    #[serde(default)]
    pub allowed_domains: Vec<String>,
    #[serde(default = "default_follow_pagination")]
    pub follow_pagination: bool,
    #[serde(default = "default_workflow_max_pages")]
    pub max_pages: usize,
    #[serde(default = "default_continue_on_error")]
    pub continue_on_error: bool,
    #[serde(default = "default_respect_robots")]
    pub respect_robots: bool,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default = "default_max_response_bytes")]
    pub max_response_bytes: usize,
    #[serde(default = "default_max_retries")]
    pub max_retries: u8,
    #[serde(default = "default_segment_limit")]
    pub max_segments: usize,
    #[serde(default = "default_link_limit")]
    pub max_links: usize,
    #[serde(default = "default_requests_per_minute")]
    pub requests_per_minute: u32,
    #[serde(default = "default_persist_report")]
    pub persist_report: bool,
    #[serde(default = "default_user_agent")]
    pub user_agent: String,
    #[serde(default)]
    pub notarize_manifest_to_chain: bool,
    #[serde(default)]
    pub attestor: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SemanticAdapterSummary {
    pub table_count: usize,
    pub table_rows: usize,
    pub collection_items: usize,
    pub pagination_candidates: usize,
    pub schema_targets: usize,
    pub form_surfaces: usize,
    pub article_segments: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct SemanticTableExtract {
    pub table_id: String,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub row_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct SemanticCollectionItem {
    pub item_id: String,
    pub title: Option<String>,
    pub url: Option<String>,
    pub summary: String,
    pub field_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct SemanticPaginationCandidate {
    pub url: String,
    pub reason: String,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SemanticAdapterReport {
    pub adapter_kinds: Vec<String>,
    pub recommended_adapter: String,
    pub tables: Vec<SemanticTableExtract>,
    pub collection_items: Vec<SemanticCollectionItem>,
    pub pagination: Vec<SemanticPaginationCandidate>,
    pub schema_targets: Vec<String>,
    pub summary: SemanticAdapterSummary,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SemanticWorkflowPage {
    pub acquisition_id: String,
    pub url: String,
    pub title: String,
    pub content_type: String,
    pub response_class: String,
    pub adapter_report: SemanticAdapterReport,
    pub hostile_signal_count: usize,
    pub enshittification_score: f64,
    pub epistemic_score: f64,
    pub claim_count: usize,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SemanticWorkflowFailure {
    pub url: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SemanticWorkflowManifest {
    pub workflow_id: String,
    pub bundle_version: u8,
    pub seed_urls: Vec<String>,
    pub started_at: i64,
    pub completed_at: i64,
    pub follow_pagination: bool,
    pub requested_pages: usize,
    pub completed_pages: usize,
    pub failed_pages: usize,
    pub page_urls: Vec<String>,
    pub adapter_summary: BTreeMap<String, usize>,
    pub pages: Vec<SemanticWorkflowPage>,
    pub failures: Vec<SemanticWorkflowFailure>,
    pub manifest_hash: String,
    pub warnings: Vec<String>,
}

impl SemanticWorkflowRequest {
    #[must_use]
    pub fn page_request(&self, url: String) -> SemanticAcquisitionRequest {
        SemanticAcquisitionRequest {
            url,
            allowed_domains: self.allowed_domains.clone(),
            respect_robots: self.respect_robots,
            timeout_ms: self.timeout_ms,
            max_response_bytes: self.max_response_bytes,
            max_retries: self.max_retries,
            max_segments: self.max_segments,
            max_links: self.max_links,
            requests_per_minute: self.requests_per_minute,
            notarize_to_chain: false,
            attestor: self.attestor.clone(),
            persist_report: self.persist_report,
            user_agent: self.user_agent.clone(),
            force_content_type: None,
        }
    }
}

#[must_use]
pub fn build_workflow_id(seed_urls: &[String]) -> String {
    sha3_256_hex(
        format!(
            "{}:{}",
            seed_urls.join("|"),
            chrono::Utc::now().timestamp_millis()
        )
        .as_bytes(),
    )[..24]
        .to_string()
}

pub fn analyze_semantic_adapters(
    acquisition: &SemanticAcquisitionReport,
    outcome: &SemanticAcquisitionOutcome,
) -> SemanticAdapterReport {
    let tables = if outcome.response_class == "html" {
        extract_tables(&outcome.semantic_html)
    } else {
        Vec::new()
    };
    let collection_items = match outcome.response_class.as_str() {
        "json" => extract_json_collection_items(&outcome.source_body),
        "xml" => extract_feed_items(&outcome.source_body),
        _ => Vec::new(),
    };
    let pagination =
        detect_pagination_candidates(&acquisition.render.links, &acquisition.render.url);
    let schema_targets = acquisition
        .render
        .extraction_targets
        .iter()
        .map(|target| target.target_type.clone())
        .collect::<Vec<_>>();
    let mut adapter_kinds = Vec::new();
    let mut warnings = Vec::new();

    if !collection_items.is_empty() {
        adapter_kinds.push(match outcome.response_class.as_str() {
            "xml" => "feed_document".to_string(),
            _ => "json_collection".to_string(),
        });
    }
    if !tables.is_empty() {
        adapter_kinds.push("tabular_html".into());
    }
    if !pagination.is_empty() {
        adapter_kinds.push("paginated_collection".into());
    }
    if !acquisition.render.structured_data.is_empty() {
        adapter_kinds.push("structured_schema".into());
    }
    if !acquisition.render.forms.is_empty() {
        adapter_kinds.push("workflow_surface".into());
    }
    if acquisition.render.summary.segment_count >= 3 {
        adapter_kinds.push("article_content".into());
    }
    if adapter_kinds.is_empty() {
        adapter_kinds.push("generic_document".into());
        warnings.push("no_specialized_adapter_detected".into());
    }

    if outcome.response_class == "json" && collection_items.is_empty() {
        warnings.push("json_document_did_not_present_as_collection".into());
    }
    if outcome.response_class == "html"
        && acquisition
            .render
            .extraction_targets
            .iter()
            .any(|target| target.target_type == "tabular_content")
        && tables.is_empty()
    {
        warnings.push("table_target_detected_but_no_rows_were_extracted".into());
    }

    let summary = SemanticAdapterSummary {
        table_count: tables.len(),
        table_rows: tables.iter().map(|table| table.row_count).sum(),
        collection_items: collection_items.len(),
        pagination_candidates: pagination.len(),
        schema_targets: schema_targets.len(),
        form_surfaces: acquisition.render.forms.len(),
        article_segments: acquisition.render.summary.segment_count,
    };

    SemanticAdapterReport {
        recommended_adapter: adapter_kinds
            .first()
            .cloned()
            .unwrap_or_else(|| "generic_document".into()),
        adapter_kinds,
        tables,
        collection_items,
        pagination,
        schema_targets,
        summary,
        warnings,
    }
}

pub fn build_workflow_manifest(
    workflow_id: String,
    request: &SemanticWorkflowRequest,
    started_at: i64,
    pages: Vec<SemanticWorkflowPage>,
    failures: Vec<SemanticWorkflowFailure>,
) -> SemanticWorkflowManifest {
    let completed_at = chrono::Utc::now().timestamp_millis();
    let mut adapter_summary = BTreeMap::new();
    let mut warnings = Vec::new();
    for page in &pages {
        for adapter in &page.adapter_report.adapter_kinds {
            *adapter_summary.entry(adapter.clone()).or_insert(0) += 1;
        }
        warnings.extend(page.warnings.clone());
    }
    warnings.extend(
        failures
            .iter()
            .map(|failure| format!("failed:{}", failure.url)),
    );
    warnings.sort();
    warnings.dedup();

    let page_urls = pages
        .iter()
        .map(|page| page.url.clone())
        .collect::<Vec<_>>();
    let payload = serde_json::json!({
        "workflow_id": workflow_id,
        "artifact": "manifest",
        "bundle_version": 1,
        "seed_urls": request.seed_urls.clone(),
        "page_urls": page_urls.clone(),
        "adapter_summary": adapter_summary,
        "completed_pages": pages.len(),
        "failed_pages": failures.len(),
    });
    let manifest_hash = sha3_256_hex(payload.to_string().as_bytes());

    SemanticWorkflowManifest {
        workflow_id,
        bundle_version: 1,
        seed_urls: request.seed_urls.clone(),
        started_at,
        completed_at,
        follow_pagination: request.follow_pagination,
        requested_pages: request.max_pages,
        completed_pages: pages.len(),
        failed_pages: failures.len(),
        page_urls,
        adapter_summary,
        pages,
        failures,
        manifest_hash,
        warnings,
    }
}

#[must_use]
pub fn workflow_manifest_payload(manifest: &SemanticWorkflowManifest) -> serde_json::Value {
    serde_json::json!({
        "workflow_id": manifest.workflow_id,
        "artifact": "manifest",
        "bundle_version": manifest.bundle_version,
        "seed_urls": manifest.seed_urls,
        "page_urls": manifest.page_urls,
        "completed_pages": manifest.completed_pages,
        "failed_pages": manifest.failed_pages,
        "adapter_summary": manifest.adapter_summary,
        "manifest_hash": manifest.manifest_hash,
    })
}

fn default_follow_pagination() -> bool {
    true
}

fn default_workflow_max_pages() -> usize {
    DEFAULT_WORKFLOW_MAX_PAGES
}

fn default_continue_on_error() -> bool {
    true
}

fn default_respect_robots() -> bool {
    true
}

fn default_timeout_ms() -> u64 {
    12_000
}

fn default_max_response_bytes() -> usize {
    1_500_000
}

fn default_max_retries() -> u8 {
    2
}

fn default_segment_limit() -> usize {
    24
}

fn default_link_limit() -> usize {
    24
}

fn default_requests_per_minute() -> u32 {
    20
}

fn default_persist_report() -> bool {
    true
}

fn default_user_agent() -> String {
    "AstraAssistant/2.0 (+semantic-acquisition)".into()
}

fn extract_tables(html: &str) -> Vec<SemanticTableExtract> {
    let table_re = Regex::new(r#"(?is)<table\b[^>]*>(.*?)</table>"#).expect("table regex");
    let row_re = Regex::new(r#"(?is)<tr\b[^>]*>(.*?)</tr>"#).expect("row regex");
    let cell_re = Regex::new(r#"(?is)<t([hd])\b[^>]*>(.*?)</t[hd]>"#).expect("cell regex");

    table_re
        .captures_iter(html)
        .enumerate()
        .take(MAX_TABLES)
        .filter_map(|(table_index, caps)| {
            let table_html = caps.get(1)?.as_str();
            let mut headers = Vec::new();
            let mut rows = Vec::new();

            for row_caps in row_re.captures_iter(table_html).take(MAX_TABLE_ROWS + 1) {
                let row_html = row_caps.get(1).map_or("", |value| value.as_str());
                let mut row = Vec::new();
                let mut header_row = true;
                for cell_caps in cell_re.captures_iter(row_html) {
                    let kind = cell_caps.get(1).map_or("d", |value| value.as_str());
                    let text =
                        sanitize_fragment(cell_caps.get(2).map_or("", |value| value.as_str()));
                    if text.is_empty() {
                        continue;
                    }
                    if kind.eq_ignore_ascii_case("d") {
                        header_row = false;
                    }
                    row.push(text);
                }
                if row.is_empty() {
                    continue;
                }
                if headers.is_empty() && header_row {
                    headers = row;
                } else {
                    rows.push(row);
                }
            }

            (!headers.is_empty() || !rows.is_empty()).then(|| SemanticTableExtract {
                table_id: format!("table_{}", table_index + 1),
                headers,
                row_count: rows.len(),
                rows,
            })
        })
        .collect()
}

fn extract_json_collection_items(body: &str) -> Vec<SemanticCollectionItem> {
    let Ok(value) = serde_json::from_str::<Value>(body) else {
        return Vec::new();
    };
    let Some(items) = json_collection_root(&value) else {
        return Vec::new();
    };

    items
        .iter()
        .take(MAX_COLLECTION_ITEMS)
        .enumerate()
        .map(|(index, item)| SemanticCollectionItem {
            item_id: format!("item_{}", index + 1),
            title: pick_string_field(item, &["title", "name", "headline", "id"]),
            url: pick_string_field(item, &["url", "href", "link", "canonical_url"]),
            summary: summarize_value(item),
            field_count: item.as_object().map_or(1, |object| object.len()),
        })
        .collect()
}

fn extract_feed_items(body: &str) -> Vec<SemanticCollectionItem> {
    let entry_re =
        Regex::new(r#"(?is)<(item|entry)\b[^>]*>(.*?)</(item|entry)>"#).expect("entry regex");
    entry_re
        .captures_iter(body)
        .take(MAX_COLLECTION_ITEMS)
        .enumerate()
        .map(|(index, caps)| {
            let block = caps.get(2).map_or("", |value| value.as_str());
            let title = extract_xml_value(block, "title");
            let url = extract_xml_value(block, "link")
                .or_else(|| extract_xml_attribute(block, "link", "href"));
            let summary = extract_xml_value(block, "description")
                .or_else(|| extract_xml_value(block, "summary"))
                .or_else(|| extract_xml_value(block, "content"))
                .unwrap_or_else(|| sanitize_fragment(block));
            SemanticCollectionItem {
                item_id: format!("feed_{}", index + 1),
                title,
                url,
                summary: truncate_chars(&summary, 220),
                field_count: count_feed_fields(block),
            }
        })
        .collect()
}

fn detect_pagination_candidates(
    links: &[SemanticLink],
    base_url: &str,
) -> Vec<SemanticPaginationCandidate> {
    let base = Url::parse(base_url).ok();
    let mut seen = BTreeSet::new();
    let mut candidates = Vec::new();

    for link in links {
        let href = link.href.trim();
        if href.is_empty() || !seen.insert(href.to_string()) {
            continue;
        }
        let text = link.text.to_lowercase();
        let rel = link.rel.clone().unwrap_or_default().to_lowercase();
        let href_lower = href.to_lowercase();

        let candidate = if rel.contains("next") {
            Some(("rel=next".to_string(), 0.98))
        } else if matches!(
            text.as_str(),
            "next" | "next page" | "older" | "continue" | "more" | "load more"
        ) {
            Some((format!("anchor_text:{text}"), 0.84))
        } else if href_lower.contains("page=")
            || href_lower.contains("cursor=")
            || href_lower.contains("offset=")
        {
            Some(("query_pagination".to_string(), 0.68))
        } else {
            None
        };

        let Some((reason, confidence)) = candidate else {
            continue;
        };
        if let Some(base) = &base {
            if let Ok(url) = Url::parse(href) {
                if base.host_str() != url.host_str() {
                    continue;
                }
            }
        }

        candidates.push(SemanticPaginationCandidate {
            url: href.to_string(),
            reason,
            confidence,
        });
        if candidates.len() >= MAX_PAGINATION_CANDIDATES {
            break;
        }
    }

    candidates
}

fn json_collection_root<'a>(value: &'a Value) -> Option<&'a [Value]> {
    match value {
        Value::Array(items) => Some(items.as_slice()),
        Value::Object(object) => ["items", "data", "results", "entries", "records"]
            .iter()
            .find_map(|key| object.get(*key))
            .and_then(|value| value.as_array().map(Vec::as_slice)),
        _ => None,
    }
}

fn pick_string_field(value: &Value, keys: &[&str]) -> Option<String> {
    let object = value.as_object()?;
    keys.iter()
        .find_map(|key| object.get(*key))
        .and_then(|value| match value {
            Value::String(text) if !text.trim().is_empty() => Some(text.trim().to_string()),
            Value::Number(number) => Some(number.to_string()),
            _ => None,
        })
}

fn summarize_value(value: &Value) -> String {
    match value {
        Value::Object(object) => {
            pick_string_field(value, &["summary", "description", "snippet", "text"]).unwrap_or_else(
                || {
                    truncate_chars(
                        &object
                            .iter()
                            .take(5)
                            .map(|(key, value)| format!("{key}:{}", scalar_preview(value)))
                            .collect::<Vec<_>>()
                            .join(" | "),
                        220,
                    )
                },
            )
        }
        Value::Array(items) => truncate_chars(
            &items
                .iter()
                .take(4)
                .map(scalar_preview)
                .collect::<Vec<_>>()
                .join(", "),
            220,
        ),
        _ => truncate_chars(&scalar_preview(value), 220),
    }
}

fn scalar_preview(value: &Value) -> String {
    match value {
        Value::String(text) => sanitize_fragment(text),
        Value::Number(number) => number.to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Null => "null".into(),
        Value::Array(items) => format!("array({})", items.len()),
        Value::Object(object) => format!("object({})", object.len()),
    }
}

fn extract_xml_value(block: &str, tag: &str) -> Option<String> {
    let regex = Regex::new(&format!(r#"(?is)<{tag}\b[^>]*>(.*?)</{tag}>"#)).ok()?;
    regex
        .captures(block)
        .and_then(|caps| caps.get(1))
        .map(|value| sanitize_fragment(value.as_str()))
        .filter(|value| !value.is_empty())
}

fn extract_xml_attribute(block: &str, tag: &str, attribute: &str) -> Option<String> {
    let regex = Regex::new(&format!(
        r#"(?is)<{tag}\b[^>]*\b{attribute}\s*=\s*["']([^"']+)["'][^>]*/?>"#
    ))
    .ok()?;
    regex
        .captures(block)
        .and_then(|caps| caps.get(1))
        .map(|value| value.as_str().trim().to_string())
        .filter(|value| !value.is_empty())
}

fn count_feed_fields(block: &str) -> usize {
    [
        "title",
        "link",
        "description",
        "summary",
        "content",
        "guid",
        "id",
    ]
    .iter()
    .filter(|field| block.to_lowercase().contains(&format!("<{field}")))
    .count()
}

fn sanitize_fragment(value: &str) -> String {
    let tags = Regex::new(r#"(?is)<[^>]+>"#).expect("tag regex");
    let whitespace = Regex::new(r#"\s+"#).expect("whitespace regex");
    let without_tags = tags.replace_all(value, " ");
    let collapsed = whitespace.replace_all(&without_tags, " ");
    decode_html_entities(collapsed.trim())
}

fn decode_html_entities(value: &str) -> String {
    value
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        value.to_string()
    } else {
        value.chars().take(max_chars).collect::<String>() + "..."
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::semantic_render::{
        ExtractionTarget, SecurityFinding, SemanticForm, SemanticHeading, SemanticMetadata,
        SemanticRenderReport, SemanticRenderSummary, SemanticSegment, StructuredDataBlock,
    };

    fn sample_render(url: &str) -> SemanticRenderReport {
        SemanticRenderReport {
            render_id: "render_1".into(),
            render_profile: "assistant_semantic_render_v1".into(),
            url: url.into(),
            normalized_url: url.into(),
            content_type: "text/html".into(),
            content_hash: "hash".into(),
            metadata: SemanticMetadata {
                title: "Example".into(),
                description: None,
                canonical_url: None,
                lang: Some("en".into()),
                meta_tags: BTreeMap::new(),
            },
            headings: vec![SemanticHeading {
                level: 1,
                text: "Example".into(),
            }],
            segments: vec![
                SemanticSegment {
                    kind: "paragraph".into(),
                    text: "This is the first meaningful paragraph in the document.".into(),
                    word_count: 9,
                },
                SemanticSegment {
                    kind: "paragraph".into(),
                    text: "This is the second meaningful paragraph in the document.".into(),
                    word_count: 9,
                },
                SemanticSegment {
                    kind: "paragraph".into(),
                    text: "This is the third meaningful paragraph in the document.".into(),
                    word_count: 9,
                },
            ],
            links: vec![SemanticLink {
                href: "https://example.com/page=2".into(),
                text: "Next".into(),
                rel: Some("next".into()),
                kind: "internal".into(),
            }],
            forms: vec![SemanticForm {
                action: None,
                method: "GET".into(),
                field_count: 1,
                field_types: vec!["text".into()],
                has_password: false,
                has_file_upload: false,
            }],
            structured_data: vec![StructuredDataBlock {
                schema_type: "Article".into(),
                preview: "{\"@type\":\"Article\"}".into(),
                bytes: 22,
            }],
            extraction_targets: vec![ExtractionTarget {
                target_type: "tabular_content".into(),
                confidence: 0.8,
                rationale: "Table detected".into(),
            }],
            security_findings: vec![SecurityFinding {
                code: "sample".into(),
                severity: "low".into(),
                detail: "sample".into(),
            }],
            warnings: vec![],
            summary: SemanticRenderSummary {
                total_text_chars: 150,
                estimated_words: 27,
                estimated_read_time_minutes: 1,
                heading_count: 1,
                segment_count: 3,
                link_count: 1,
                internal_link_count: 1,
                external_link_count: 0,
                form_count: 1,
                structured_data_blocks: 1,
                script_count: 0,
                iframe_count: 0,
                inline_event_handlers: 0,
                access_barrier_signals: 0,
            },
            rendered_at: 1,
        }
    }

    #[test]
    fn adapter_analysis_extracts_tables_and_pagination() {
        let render = sample_render("https://example.com/start");
        let acquisition = SemanticAcquisitionReport {
            acquisition_id: "acq_1".into(),
            status: "acquired".into(),
            provenance: crate::features::semantic_acquisition::SemanticAcquisitionProvenance {
                acquisition_id: "acq_1".into(),
                requested_url: "https://example.com/start".into(),
                final_url: "https://example.com/start".into(),
                normalized_url: "https://example.com/start".into(),
                domain: "example.com".into(),
                scheme: "https".into(),
                status_code: 200,
                content_type: "text/html".into(),
                response_class: "html".into(),
                bytes_received: 120,
                content_hash: "hash".into(),
                attempt_count: 1,
                retried: false,
                duration_ms: 10,
                started_at: 1,
                fetched_at: 2,
                response_headers: BTreeMap::new(),
                egress: crate::network::egress::EgressPolicy::from_env().status(),
                policy: crate::features::semantic_acquisition::SemanticAcquisitionPolicy {
                    allowed_domains: vec!["example.com".into()],
                    respect_robots: true,
                    timeout_ms: 1_000,
                    max_response_bytes: 100_000,
                    max_retries: 0,
                    max_segments: 10,
                    max_links: 10,
                    requests_per_minute: 10,
                    user_agent: "AstraAssistant".into(),
                    persist_report: false,
                },
                robots: crate::features::semantic_acquisition::RobotsPolicyResult {
                    checked: false,
                    robots_url: None,
                    allowed: true,
                    matched_scope: "*".into(),
                    allow_rules: 0,
                    disallow_rules: 0,
                    reason: "disabled".into(),
                    fetch_status: None,
                    warnings: vec![],
                },
                warnings: vec![],
            },
            render,
            recorded_at: 3,
        };
        let outcome = SemanticAcquisitionOutcome {
            acquisition_id: "acq_1".into(),
            requested_url: "https://example.com/start".into(),
            final_url: "https://example.com/start".into(),
            normalized_url: "https://example.com/start".into(),
            domain: "example.com".into(),
            scheme: "https".into(),
            status_code: 200,
            content_type: "text/html".into(),
            response_class: "html".into(),
            bytes_received: 120,
            content_hash: "hash".into(),
            attempt_count: 1,
            duration_ms: 10,
            fetched_at: 2,
            response_headers: BTreeMap::new(),
            robots: crate::features::semantic_acquisition::RobotsPolicyResult {
                checked: false,
                robots_url: None,
                allowed: true,
                matched_scope: "*".into(),
                allow_rules: 0,
                disallow_rules: 0,
                reason: "disabled".into(),
                fetch_status: None,
                warnings: vec![],
            },
            warnings: vec![],
            source_body: "<table><tr><th>Name</th><th>Score</th></tr><tr><td>Ada</td><td>99</td></tr></table>".into(),
            semantic_html: "<html><body><table><tr><th>Name</th><th>Score</th></tr><tr><td>Ada</td><td>99</td></tr></table><a rel=\"next\" href=\"https://example.com/page=2\">Next</a></body></html>".into(),
        };

        let report = analyze_semantic_adapters(&acquisition, &outcome);

        assert_eq!(report.recommended_adapter, "tabular_html");
        assert_eq!(report.tables.len(), 1);
        assert_eq!(report.tables[0].headers, vec!["Name", "Score"]);
        assert_eq!(report.pagination.len(), 1);
        assert!(report
            .adapter_kinds
            .contains(&"paginated_collection".to_string()));
    }

    #[test]
    fn json_collection_adapter_extracts_items() {
        let render = sample_render("https://example.com/data.json");
        let acquisition = SemanticAcquisitionReport {
            acquisition_id: "acq_json".into(),
            status: "acquired".into(),
            provenance: crate::features::semantic_acquisition::SemanticAcquisitionProvenance {
                acquisition_id: "acq_json".into(),
                requested_url: "https://example.com/data.json".into(),
                final_url: "https://example.com/data.json".into(),
                normalized_url: "https://example.com/data.json".into(),
                domain: "example.com".into(),
                scheme: "https".into(),
                status_code: 200,
                content_type: "application/json".into(),
                response_class: "json".into(),
                bytes_received: 120,
                content_hash: "hash".into(),
                attempt_count: 1,
                retried: false,
                duration_ms: 10,
                started_at: 1,
                fetched_at: 2,
                response_headers: BTreeMap::new(),
                egress: crate::network::egress::EgressPolicy::from_env().status(),
                policy: crate::features::semantic_acquisition::SemanticAcquisitionPolicy {
                    allowed_domains: vec!["example.com".into()],
                    respect_robots: true,
                    timeout_ms: 1_000,
                    max_response_bytes: 100_000,
                    max_retries: 0,
                    max_segments: 10,
                    max_links: 10,
                    requests_per_minute: 10,
                    user_agent: "AstraAssistant".into(),
                    persist_report: false,
                },
                robots: crate::features::semantic_acquisition::RobotsPolicyResult {
                    checked: false,
                    robots_url: None,
                    allowed: true,
                    matched_scope: "*".into(),
                    allow_rules: 0,
                    disallow_rules: 0,
                    reason: "disabled".into(),
                    fetch_status: None,
                    warnings: vec![],
                },
                warnings: vec![],
            },
            render,
            recorded_at: 3,
        };
        let outcome = SemanticAcquisitionOutcome {
            acquisition_id: "acq_json".into(),
            requested_url: "https://example.com/data.json".into(),
            final_url: "https://example.com/data.json".into(),
            normalized_url: "https://example.com/data.json".into(),
            domain: "example.com".into(),
            scheme: "https".into(),
            status_code: 200,
            content_type: "application/json".into(),
            response_class: "json".into(),
            bytes_received: 120,
            content_hash: "hash".into(),
            attempt_count: 1,
            duration_ms: 10,
            fetched_at: 2,
            response_headers: BTreeMap::new(),
            robots: crate::features::semantic_acquisition::RobotsPolicyResult {
                checked: false,
                robots_url: None,
                allowed: true,
                matched_scope: "*".into(),
                allow_rules: 0,
                disallow_rules: 0,
                reason: "disabled".into(),
                fetch_status: None,
                warnings: vec![],
            },
            warnings: vec![],
            source_body: r#"{"items":[{"title":"Alpha","url":"https://example.com/a","summary":"First item"},{"title":"Beta","url":"https://example.com/b","summary":"Second item"}]}"#.into(),
            semantic_html: String::new(),
        };

        let report = analyze_semantic_adapters(&acquisition, &outcome);

        assert_eq!(report.recommended_adapter, "json_collection");
        assert_eq!(report.collection_items.len(), 2);
        assert_eq!(report.collection_items[0].title.as_deref(), Some("Alpha"));
    }
}
