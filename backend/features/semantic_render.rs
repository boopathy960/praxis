use std::collections::{BTreeMap, BTreeSet};

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;

use crate::crypto::hash::sha3_256_hex;
use crate::error::{AstraError, AstraResult};

const MAX_RENDER_BYTES: usize = 2 * 1024 * 1024;
const DEFAULT_SEGMENT_LIMIT: usize = 24;
const DEFAULT_LINK_LIMIT: usize = 24;

#[derive(Debug, Clone, Deserialize)]
pub struct SemanticRenderRequest {
    pub url: String,
    pub html: String,
    #[serde(default = "default_content_type")]
    pub content_type: String,
    #[serde(default)]
    pub attestor: Option<String>,
    #[serde(default)]
    pub notarize_to_chain: bool,
    #[serde(default = "default_segment_limit")]
    pub max_segments: usize,
    #[serde(default = "default_link_limit")]
    pub max_links: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct SemanticRenderReport {
    pub render_id: String,
    pub render_profile: String,
    pub url: String,
    pub normalized_url: String,
    pub content_type: String,
    pub content_hash: String,
    pub metadata: SemanticMetadata,
    pub headings: Vec<SemanticHeading>,
    pub segments: Vec<SemanticSegment>,
    pub links: Vec<SemanticLink>,
    pub forms: Vec<SemanticForm>,
    pub structured_data: Vec<StructuredDataBlock>,
    pub extraction_targets: Vec<ExtractionTarget>,
    pub security_findings: Vec<SecurityFinding>,
    pub warnings: Vec<String>,
    pub summary: SemanticRenderSummary,
    pub rendered_at: i64,
}

impl SemanticRenderReport {
    #[must_use]
    pub fn combined_text(&self) -> String {
        self.segments
            .iter()
            .map(|segment| segment.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SemanticMetadata {
    pub title: String,
    pub description: Option<String>,
    pub canonical_url: Option<String>,
    pub lang: Option<String>,
    pub meta_tags: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SemanticHeading {
    pub level: u8,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SemanticSegment {
    pub kind: String,
    pub text: String,
    pub word_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct SemanticLink {
    pub href: String,
    pub text: String,
    pub rel: Option<String>,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SemanticForm {
    pub action: Option<String>,
    pub method: String,
    pub field_count: usize,
    pub field_types: Vec<String>,
    pub has_password: bool,
    pub has_file_upload: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct StructuredDataBlock {
    pub schema_type: String,
    pub preview: String,
    pub bytes: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExtractionTarget {
    pub target_type: String,
    pub confidence: f64,
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SecurityFinding {
    pub code: String,
    pub severity: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SemanticRenderSummary {
    pub total_text_chars: usize,
    pub estimated_words: usize,
    pub estimated_read_time_minutes: u64,
    pub heading_count: usize,
    pub segment_count: usize,
    pub link_count: usize,
    pub internal_link_count: usize,
    pub external_link_count: usize,
    pub form_count: usize,
    pub structured_data_blocks: usize,
    pub script_count: usize,
    pub iframe_count: usize,
    pub inline_event_handlers: usize,
    pub access_barrier_signals: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct SemanticRenderStats {
    pub total_renders: u64,
    pub total_segments_emitted: u64,
    pub total_links_emitted: u64,
    pub total_forms_observed: u64,
    pub notarized_renders: u64,
    pub last_rendered_at: i64,
}

pub struct SemanticRenderEngine {
    total_renders: u64,
    total_segments_emitted: u64,
    total_links_emitted: u64,
    total_forms_observed: u64,
    notarized_renders: u64,
    last_rendered_at: i64,
}

impl SemanticRenderEngine {
    #[must_use]
    pub fn new() -> Self {
        Self {
            total_renders: 0,
            total_segments_emitted: 0,
            total_links_emitted: 0,
            total_forms_observed: 0,
            notarized_renders: 0,
            last_rendered_at: 0,
        }
    }

    pub fn render(&mut self, request: SemanticRenderRequest) -> AstraResult<SemanticRenderReport> {
        if request.url.trim().is_empty() {
            return Err(AstraError::ControlPlaneRejected(
                "semantic render requires a URL".into(),
            ));
        }
        if request.html.trim().is_empty() {
            return Err(AstraError::RenderFailed(
                "semantic render requires non-empty HTML".into(),
            ));
        }
        if request.html.len() > MAX_RENDER_BYTES {
            return Err(AstraError::RenderFailed(format!(
                "semantic render input exceeds {} bytes",
                MAX_RENDER_BYTES
            )));
        }

        let normalized_url = Url::parse(&request.url)
            .map_err(|error| AstraError::RenderFailed(format!("invalid render URL: {error}")))?;
        let rendered_at = chrono::Utc::now().timestamp_millis();
        let content_hash = sha3_256_hex(request.html.as_bytes());
        let render_id = sha3_256_hex(
            format!(
                "{}:{}:{}",
                normalized_url.as_str(),
                content_hash,
                rendered_at
            )
            .as_bytes(),
        )[..24]
            .to_string();

        let metadata = extract_metadata(&request.html, &normalized_url);
        let headings = extract_headings(&request.html);
        let segments = extract_segments(&request.html, request.max_segments.max(1));
        let links = extract_links(&request.html, &normalized_url, request.max_links.max(1));
        let forms = extract_forms(&request.html, &normalized_url);
        let structured_data = extract_structured_data(&request.html);
        let security_findings =
            analyze_security_surface(&normalized_url, &request.html, &links, &forms, &segments);
        let summary = build_summary(
            &segments,
            &headings,
            &links,
            &forms,
            &structured_data,
            &request.html,
        );
        let extraction_targets =
            derive_extraction_targets(&segments, &links, &forms, &structured_data, &request.html);
        let warnings = derive_warnings(&summary, &security_findings, &metadata, &segments);

        self.total_renders += 1;
        self.total_segments_emitted += segments.len() as u64;
        self.total_links_emitted += links.len() as u64;
        self.total_forms_observed += forms.len() as u64;
        self.last_rendered_at = rendered_at;

        Ok(SemanticRenderReport {
            render_id,
            render_profile: "assistant_semantic_render_v1".into(),
            url: request.url,
            normalized_url: normalized_url.to_string(),
            content_type: request.content_type,
            content_hash,
            metadata,
            headings,
            segments,
            links,
            forms,
            structured_data,
            extraction_targets,
            security_findings,
            warnings,
            summary,
            rendered_at,
        })
    }

    pub fn record_chain_commit(&mut self) {
        self.notarized_renders += 1;
    }

    #[must_use]
    pub fn stats(&self) -> SemanticRenderStats {
        SemanticRenderStats {
            total_renders: self.total_renders,
            total_segments_emitted: self.total_segments_emitted,
            total_links_emitted: self.total_links_emitted,
            total_forms_observed: self.total_forms_observed,
            notarized_renders: self.notarized_renders,
            last_rendered_at: self.last_rendered_at,
        }
    }
}

fn default_content_type() -> String {
    "text/html".into()
}

fn default_segment_limit() -> usize {
    DEFAULT_SEGMENT_LIMIT
}

fn default_link_limit() -> usize {
    DEFAULT_LINK_LIMIT
}

fn extract_metadata(html: &str, url: &Url) -> SemanticMetadata {
    let mut meta_tags = BTreeMap::new();
    let html_tag_re = Regex::new(r#"(?is)<html\b([^>]*)>"#).expect("html regex should compile");
    let title_re =
        Regex::new(r#"(?is)<title\b[^>]*>(.*?)</title>"#).expect("title regex should compile");
    let meta_re = Regex::new(r#"(?is)<meta\b([^>]*)>"#).expect("meta regex should compile");
    let link_re = Regex::new(r#"(?is)<link\b([^>]*)>"#).expect("link regex should compile");

    let lang = html_tag_re
        .captures(html)
        .and_then(|caps| caps.get(1))
        .map(|m| parse_attributes(m.as_str()))
        .and_then(|attrs| attrs.get("lang").cloned());

    for caps in meta_re.captures_iter(html) {
        let attrs = parse_attributes(caps.get(1).map_or("", |m| m.as_str()));
        let key = attrs
            .get("name")
            .or_else(|| attrs.get("property"))
            .or_else(|| attrs.get("http-equiv"));
        let value = attrs.get("content");
        if let (Some(key), Some(value)) = (key, value) {
            meta_tags.insert(key.to_lowercase(), decode_html_entities(value));
        }
    }

    let title = title_re
        .captures(html)
        .and_then(|caps| caps.get(1))
        .map_or_else(
            || {
                meta_tags
                    .get("og:title")
                    .cloned()
                    .unwrap_or_else(|| url.host_str().unwrap_or("untitled").to_string())
            },
            |capture| {
                let title = sanitize_text(capture.as_str());
                if title.is_empty() {
                    url.host_str().unwrap_or("untitled").to_string()
                } else {
                    title
                }
            },
        );

    let canonical_url = link_re.captures_iter(html).find_map(|caps| {
        let attrs = parse_attributes(caps.get(1).map_or("", |m| m.as_str()));
        let rel = attrs.get("rel")?;
        if rel.to_lowercase().contains("canonical") {
            attrs
                .get("href")
                .and_then(|href| resolve_url(url, href))
                .map(|resolved| resolved.to_string())
        } else {
            None
        }
    });

    SemanticMetadata {
        title,
        description: meta_tags
            .get("description")
            .cloned()
            .or_else(|| meta_tags.get("og:description").cloned())
            .or_else(|| meta_tags.get("twitter:description").cloned()),
        canonical_url,
        lang,
        meta_tags,
    }
}

fn extract_headings(html: &str) -> Vec<SemanticHeading> {
    let heading_re = Regex::new(r#"(?is)<h([1-6])\b[^>]*>(.*?)</h[1-6]>"#)
        .expect("heading regex should compile");

    heading_re
        .captures_iter(html)
        .filter_map(|caps| {
            let level = caps.get(1)?.as_str().parse::<u8>().ok()?;
            let text = sanitize_text(caps.get(2)?.as_str());
            if text.is_empty() {
                None
            } else {
                Some(SemanticHeading { level, text })
            }
        })
        .collect()
}

fn extract_segments(html: &str, limit: usize) -> Vec<SemanticSegment> {
    let body = extract_body_or_document(html);
    let mut candidates = Vec::new();
    let mut seen = BTreeSet::new();

    for (tag, kind) in [
        ("p", "paragraph"),
        ("li", "list_item"),
        ("blockquote", "blockquote"),
        ("pre", "preformatted"),
        ("code", "code"),
        ("td", "table_cell"),
        ("th", "table_header"),
    ] {
        let pattern = format!(r#"(?is)<{tag}\b[^>]*>(.*?)</{tag}>"#);
        let regex = Regex::new(&pattern).expect("segment regex should compile");
        for caps in regex.captures_iter(body) {
            let Some(full) = caps.get(0) else {
                continue;
            };
            let Some(inner) = caps.get(1) else {
                continue;
            };
            let text = sanitize_text(inner.as_str());
            let words = word_count(&text);
            if text.is_empty() || words < 3 {
                continue;
            }
            let key = text.to_lowercase();
            if !seen.insert(key) {
                continue;
            }
            candidates.push((
                full.start(),
                SemanticSegment {
                    kind: kind.into(),
                    text,
                    word_count: words,
                },
            ));
        }
    }

    candidates.sort_by_key(|(start, _)| *start);
    let mut segments = candidates
        .into_iter()
        .map(|(_, segment)| segment)
        .take(limit)
        .collect::<Vec<_>>();

    if segments.is_empty() {
        let fallback = sanitize_text(body);
        if !fallback.is_empty() {
            for chunk in split_words(&fallback, 80).into_iter().take(limit) {
                let words = word_count(&chunk);
                if words >= 3 {
                    segments.push(SemanticSegment {
                        kind: "body_fallback".into(),
                        text: chunk,
                        word_count: words,
                    });
                }
            }
        }
    }

    segments
}

fn extract_links(html: &str, base_url: &Url, limit: usize) -> Vec<SemanticLink> {
    let anchor_re =
        Regex::new(r#"(?is)<a\b([^>]*)>(.*?)</a>"#).expect("anchor regex should compile");
    let mut links = Vec::new();
    let mut seen = BTreeSet::new();

    for caps in anchor_re.captures_iter(html) {
        let attrs = parse_attributes(caps.get(1).map_or("", |m| m.as_str()));
        let Some(raw_href) = attrs.get("href") else {
            continue;
        };
        let text = sanitize_text(caps.get(2).map_or("", |m| m.as_str()));
        let resolved = resolve_url(base_url, raw_href).map(|value| value.to_string());
        let href = resolved.unwrap_or_else(|| raw_href.to_string());
        if !seen.insert(href.clone()) {
            continue;
        }

        links.push(SemanticLink {
            kind: classify_link_kind(base_url, &href),
            href,
            text,
            rel: attrs.get("rel").cloned(),
        });

        if links.len() >= limit {
            break;
        }
    }

    links
}

fn extract_forms(html: &str, base_url: &Url) -> Vec<SemanticForm> {
    let form_re =
        Regex::new(r#"(?is)<form\b([^>]*)>(.*?)</form>"#).expect("form regex should compile");
    let input_re = Regex::new(r#"(?is)<input\b([^>]*)>"#).expect("input regex should compile");
    let textarea_re =
        Regex::new(r#"(?is)<textarea\b([^>]*)>"#).expect("textarea regex should compile");
    let select_re = Regex::new(r#"(?is)<select\b([^>]*)>"#).expect("select regex should compile");

    let mut forms = Vec::new();
    for caps in form_re.captures_iter(html) {
        let attrs = parse_attributes(caps.get(1).map_or("", |m| m.as_str()));
        let inner = caps.get(2).map_or("", |m| m.as_str());
        let action = attrs.get("action").and_then(|value| {
            resolve_url(base_url, value)
                .map(|url| url.to_string())
                .or_else(|| Some(value.clone()))
        });
        let method = attrs
            .get("method")
            .map(|value| value.to_uppercase())
            .unwrap_or_else(|| "GET".into());

        let mut field_types = Vec::new();
        for input in input_re.captures_iter(inner) {
            let input_attrs = parse_attributes(input.get(1).map_or("", |m| m.as_str()));
            field_types.push(
                input_attrs
                    .get("type")
                    .cloned()
                    .unwrap_or_else(|| "text".into())
                    .to_lowercase(),
            );
        }
        let textarea_count = textarea_re.captures_iter(inner).count();
        let select_count = select_re.captures_iter(inner).count();
        field_types.extend((0..textarea_count).map(|_| "textarea".to_string()));
        field_types.extend((0..select_count).map(|_| "select".to_string()));

        forms.push(SemanticForm {
            action,
            method,
            field_count: field_types.len(),
            has_password: field_types.iter().any(|field| field == "password"),
            has_file_upload: field_types.iter().any(|field| field == "file"),
            field_types,
        });
    }

    forms
}

fn extract_structured_data(html: &str) -> Vec<StructuredDataBlock> {
    let script_re =
        Regex::new(r#"(?is)<script\b([^>]*)>(.*?)</script>"#).expect("script regex should compile");
    let mut blocks = Vec::new();

    for caps in script_re.captures_iter(html) {
        let attrs = parse_attributes(caps.get(1).map_or("", |m| m.as_str()));
        let Some(script_type) = attrs.get("type") else {
            continue;
        };
        if script_type.to_lowercase() != "application/ld+json" {
            continue;
        }
        let raw = caps.get(2).map_or("", |m| m.as_str()).trim();
        if raw.is_empty() {
            continue;
        }
        blocks.push(StructuredDataBlock {
            schema_type: classify_schema_type(raw),
            preview: truncate_chars(&sanitize_text(raw), 180),
            bytes: raw.len(),
        });
    }

    blocks
}

fn analyze_security_surface(
    base_url: &Url,
    html: &str,
    links: &[SemanticLink],
    forms: &[SemanticForm],
    segments: &[SemanticSegment],
) -> Vec<SecurityFinding> {
    let script_count = count_matches(r#"(?is)<script\b"#, html);
    let iframe_count = count_matches(r#"(?is)<iframe\b"#, html);
    let inline_event_handlers = count_matches(r#"(?i)\son[a-z]+\s*="#, html);
    let total_words = segments
        .iter()
        .map(|segment| segment.word_count)
        .sum::<usize>();
    let mut findings = Vec::new();

    if script_count >= 20 {
        findings.push(SecurityFinding {
            code: "script_heavy_surface".into(),
            severity: "medium".into(),
            detail: format!(
                "Document exposes a high script surface with {script_count} script tags."
            ),
        });
    }
    if iframe_count > 0 {
        findings.push(SecurityFinding {
            code: "embedded_frame_surface".into(),
            severity: "medium".into(),
            detail: format!("Document embeds {iframe_count} iframe elements."),
        });
    }
    if inline_event_handlers > 0 {
        findings.push(SecurityFinding {
            code: "inline_event_handlers".into(),
            severity: "high".into(),
            detail: format!("Document contains {inline_event_handlers} inline DOM event handlers."),
        });
    }
    if total_words < 120 && script_count > 12 {
        findings.push(SecurityFinding {
            code: "thin_content_with_heavy_script".into(),
            severity: "medium".into(),
            detail: "Document is script-heavy relative to extracted readable content.".into(),
        });
    }

    let base_host = base_url.host_str().unwrap_or_default().to_lowercase();
    for form in forms {
        if form.has_password && base_url.scheme() != "https" {
            findings.push(SecurityFinding {
                code: "credential_form_over_non_https".into(),
                severity: "high".into(),
                detail: "Password capture is present on a non-HTTPS origin.".into(),
            });
        }
        if let Some(action) = &form.action {
            if let Ok(action_url) = Url::parse(action) {
                let action_host = action_url.host_str().unwrap_or_default().to_lowercase();
                if !base_host.is_empty() && action_host != base_host {
                    findings.push(SecurityFinding {
                        code: "cross_origin_form_post".into(),
                        severity: "medium".into(),
                        detail: format!("Form posts to a different origin: {action}."),
                    });
                }
            }
        }
    }

    if links.iter().filter(|link| link.kind == "external").count() >= 12 {
        findings.push(SecurityFinding {
            code: "high_external_link_density".into(),
            severity: "low".into(),
            detail:
                "Document exposes a dense external link graph that may require reputation review."
                    .into(),
        });
    }

    findings
}

fn build_summary(
    segments: &[SemanticSegment],
    headings: &[SemanticHeading],
    links: &[SemanticLink],
    forms: &[SemanticForm],
    structured_data: &[StructuredDataBlock],
    html: &str,
) -> SemanticRenderSummary {
    let total_text_chars = segments
        .iter()
        .map(|segment| segment.text.len())
        .sum::<usize>();
    let estimated_words = segments
        .iter()
        .map(|segment| segment.word_count)
        .sum::<usize>();
    let internal_link_count = links.iter().filter(|link| link.kind == "internal").count();
    let external_link_count = links.iter().filter(|link| link.kind == "external").count();

    SemanticRenderSummary {
        total_text_chars,
        estimated_words,
        estimated_read_time_minutes: ((estimated_words as f64) / 220.0).ceil() as u64,
        heading_count: headings.len(),
        segment_count: segments.len(),
        link_count: links.len(),
        internal_link_count,
        external_link_count,
        form_count: forms.len(),
        structured_data_blocks: structured_data.len(),
        script_count: count_matches(r#"(?is)<script\b"#, html),
        iframe_count: count_matches(r#"(?is)<iframe\b"#, html),
        inline_event_handlers: count_matches(r#"(?i)\son[a-z]+\s*="#, html),
        access_barrier_signals: count_access_barrier_signals(html),
    }
}

fn derive_extraction_targets(
    segments: &[SemanticSegment],
    links: &[SemanticLink],
    forms: &[SemanticForm],
    structured_data: &[StructuredDataBlock],
    html: &str,
) -> Vec<ExtractionTarget> {
    let mut targets = Vec::new();

    if segments.len() >= 3 {
        targets.push(ExtractionTarget {
            target_type: "article_content".into(),
            confidence: 0.94,
            rationale: "Readable paragraph and list segments were extracted successfully.".into(),
        });
    }
    if links.iter().any(|link| link.kind == "external") {
        targets.push(ExtractionTarget {
            target_type: "citation_graph".into(),
            confidence: 0.82,
            rationale: "Document exposes external references that can be crawled as evidence edges."
                .into(),
        });
    }
    if !forms.is_empty() {
        targets.push(ExtractionTarget {
            target_type: "workflow_surface".into(),
            confidence: 0.78,
            rationale: "Interactive form surfaces were detected and normalized.".into(),
        });
    }
    if !structured_data.is_empty() {
        targets.push(ExtractionTarget {
            target_type: "structured_schema".into(),
            confidence: 0.97,
            rationale: "JSON-LD schema blocks are available for deterministic extraction.".into(),
        });
    }
    if html.to_lowercase().contains("<table") {
        targets.push(ExtractionTarget {
            target_type: "tabular_content".into(),
            confidence: 0.75,
            rationale: "Table markup is present and can be harvested into rows and cells.".into(),
        });
    }

    targets
}

fn derive_warnings(
    summary: &SemanticRenderSummary,
    security_findings: &[SecurityFinding],
    metadata: &SemanticMetadata,
    segments: &[SemanticSegment],
) -> Vec<String> {
    let mut warnings = Vec::new();

    if segments.is_empty() {
        warnings
            .push("No meaningful semantic text blocks were extracted from the document.".into());
    }
    if metadata.description.is_none() {
        warnings.push("Document is missing a durable description meta field.".into());
    }
    if summary.access_barrier_signals > 0 {
        warnings.push(
            "Potential access barriers were detected; downstream scraping may require review."
                .into(),
        );
    }
    if summary.script_count > 0 && summary.estimated_words < 80 {
        warnings.push("Readable content is sparse relative to the active script surface.".into());
    }
    if security_findings
        .iter()
        .any(|finding| finding.severity == "high")
    {
        warnings.push(
            "High-severity render findings were detected and should be reviewed before automation."
                .into(),
        );
    }

    warnings
}

fn classify_schema_type(raw: &str) -> String {
    serde_json::from_str::<Value>(raw)
        .ok()
        .and_then(|value| {
            value
                .get("@type")
                .and_then(Value::as_str)
                .map(ToString::to_string)
                .or_else(|| {
                    value
                        .get("@graph")
                        .and_then(Value::as_array)
                        .and_then(|graph| graph.first())
                        .and_then(|node| node.get("@type"))
                        .and_then(Value::as_str)
                        .map(ToString::to_string)
                })
        })
        .unwrap_or_else(|| "json_ld".into())
}

fn classify_link_kind(base_url: &Url, href: &str) -> String {
    if href.starts_with('#') {
        return "fragment".into();
    }
    if href.starts_with("mailto:") {
        return "mailto".into();
    }
    if let Ok(url) = Url::parse(href) {
        let base_host = base_url.host_str().unwrap_or_default();
        let href_host = url.host_str().unwrap_or_default();
        if !base_host.is_empty() && base_host.eq_ignore_ascii_case(href_host) {
            "internal".into()
        } else {
            "external".into()
        }
    } else {
        "internal".into()
    }
}

fn resolve_url(base_url: &Url, raw: &str) -> Option<Url> {
    Url::parse(raw).ok().or_else(|| base_url.join(raw).ok())
}

fn extract_body_or_document(html: &str) -> &str {
    let body_re =
        Regex::new(r#"(?is)<body\b[^>]*>(.*?)</body>"#).expect("body regex should compile");
    body_re
        .captures(html)
        .and_then(|caps| caps.get(1))
        .map_or(html, |m| m.as_str())
}

fn parse_attributes(raw: &str) -> BTreeMap<String, String> {
    let attr_re =
        Regex::new(r#"([a-zA-Z_:][-a-zA-Z0-9_:.]*)\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+))"#)
            .expect("attribute regex should compile");
    let mut attrs = BTreeMap::new();
    for caps in attr_re.captures_iter(raw) {
        let Some(name) = caps.get(1) else {
            continue;
        };
        let value = caps
            .get(2)
            .or_else(|| caps.get(3))
            .or_else(|| caps.get(4))
            .map_or("", |m| m.as_str());
        attrs.insert(name.as_str().to_lowercase(), decode_html_entities(value));
    }
    attrs
}

fn sanitize_text(raw: &str) -> String {
    let without_comments = Regex::new(r#"(?is)<!--.*?-->"#)
        .expect("comment regex should compile")
        .replace_all(raw, " ");
    let without_tags = Regex::new(r#"(?is)<[^>]+>"#)
        .expect("tag regex should compile")
        .replace_all(&without_comments, " ");
    normalize_whitespace(&decode_html_entities(&without_tags))
}

fn normalize_whitespace(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn decode_html_entities(raw: &str) -> String {
    let mut decoded = raw
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'");

    let numeric_re =
        Regex::new(r#"&#x([0-9a-fA-F]+);|&#([0-9]+);"#).expect("entity regex should compile");
    loop {
        let Some(caps) = numeric_re.captures(&decoded) else {
            break;
        };
        let Some(full) = caps.get(0) else {
            break;
        };
        let replacement = if let Some(hex) = caps.get(1) {
            u32::from_str_radix(hex.as_str(), 16)
                .ok()
                .and_then(char::from_u32)
                .map_or_else(String::new, |ch| ch.to_string())
        } else if let Some(dec) = caps.get(2) {
            dec.as_str()
                .parse::<u32>()
                .ok()
                .and_then(char::from_u32)
                .map_or_else(String::new, |ch| ch.to_string())
        } else {
            String::new()
        };
        decoded.replace_range(full.range(), &replacement);
    }
    decoded
}

fn word_count(text: &str) -> usize {
    text.split_whitespace().count()
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        text.to_string()
    } else {
        let truncated = text.chars().take(max_chars).collect::<String>();
        format!("{truncated}...")
    }
}

fn split_words(text: &str, chunk_words: usize) -> Vec<String> {
    let words = text.split_whitespace().collect::<Vec<_>>();
    words
        .chunks(chunk_words.max(1))
        .map(|chunk| chunk.join(" "))
        .collect()
}

fn count_access_barrier_signals(html: &str) -> usize {
    let lowered = html.to_lowercase();
    [
        "sign in to continue",
        "create an account",
        "register to continue",
        "members only",
        "subscribe to read",
        "premium content",
        "log in to view",
        "unlock this article",
    ]
    .iter()
    .filter(|marker| lowered.contains(**marker))
    .count()
}

fn count_matches(pattern: &str, text: &str) -> usize {
    Regex::new(pattern)
        .expect("count regex should compile")
        .find_iter(text)
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_html() -> String {
        r#"
        <html lang="en">
          <head>
            <title>Astra Research Pipeline</title>
            <meta name="description" content="Semantic rendering for assistant-grade scraping." />
            <meta property="og:title" content="Astra Research Pipeline" />
            <link rel="canonical" href="https://example.com/research/pipeline" />
            <script type="application/ld+json">
              {"@context":"https://schema.org","@type":"NewsArticle","headline":"Astra Research Pipeline"}
            </script>
          </head>
          <body>
            <article>
              <h1>Astra Research Pipeline</h1>
              <p>The assistant render engine extracts normalized evidence for indexing and chain proofs.</p>
              <p>It keeps content semantic, bounded, and ready for downstream verification workflows.</p>
              <ul>
                <li>Evidence capture for downstream audit trails</li>
                <li>Link normalization for recursive acquisition</li>
              </ul>
              <a href="/docs/start">Docs</a>
              <a rel="nofollow" href="https://outside.example/report">External report</a>
              <form action="/login" method="post">
                <input type="email" />
                <input type="password" />
              </form>
            </article>
          </body>
        </html>
        "#
        .into()
    }

    #[test]
    fn semantic_render_extracts_metadata_and_segments() {
        let mut engine = SemanticRenderEngine::new();
        let report = engine
            .render(SemanticRenderRequest {
                url: "https://example.com/research/pipeline".into(),
                html: sample_html(),
                content_type: "text/html".into(),
                attestor: None,
                notarize_to_chain: false,
                max_segments: 12,
                max_links: 8,
            })
            .expect("semantic render should succeed");

        assert_eq!(report.metadata.title, "Astra Research Pipeline");
        assert_eq!(
            report.metadata.canonical_url.as_deref(),
            Some("https://example.com/research/pipeline")
        );
        assert_eq!(report.metadata.lang.as_deref(), Some("en"));
        assert!(report.segments.len() >= 3);
        assert_eq!(report.links.len(), 2);
        assert_eq!(report.forms.len(), 1);
        assert_eq!(report.structured_data.len(), 1);
        assert!(report
            .extraction_targets
            .iter()
            .any(|target| target.target_type == "article_content"));
        assert!(report
            .extraction_targets
            .iter()
            .any(|target| target.target_type == "structured_schema"));
    }

    #[test]
    fn semantic_render_flags_security_and_access_barriers() {
        let mut engine = SemanticRenderEngine::new();
        let html = format!(
            r#"<html><body>
                <div>Sign in to continue</div>
                <form action="http://collector.example/login"><input type="password" /></form>
                <button onclick="steal()">Continue</button>
                {}
              </body></html>"#,
            "<script>void 0;</script>".repeat(24)
        );

        let report = engine
            .render(SemanticRenderRequest {
                url: "http://example.com/login".into(),
                html,
                content_type: "text/html".into(),
                attestor: None,
                notarize_to_chain: false,
                max_segments: 8,
                max_links: 8,
            })
            .expect("semantic render should succeed");

        assert!(report.summary.access_barrier_signals > 0);
        assert!(report
            .security_findings
            .iter()
            .any(|finding| finding.code == "credential_form_over_non_https"));
        assert!(report
            .security_findings
            .iter()
            .any(|finding| finding.code == "inline_event_handlers"));
        assert!(report
            .warnings
            .iter()
            .any(|warning| warning.contains("access barriers")));
    }

    #[test]
    fn semantic_render_stats_track_notarized_documents() {
        let mut engine = SemanticRenderEngine::new();
        engine
            .render(SemanticRenderRequest {
                url: "https://example.com".into(),
                html: "<html><body><p>Hello semantic world with enough words for extraction.</p></body></html>".into(),
                content_type: "text/html".into(),
                attestor: None,
                notarize_to_chain: false,
                max_segments: 8,
                max_links: 8,
            })
            .expect("semantic render should succeed");
        engine.record_chain_commit();

        let stats = engine.stats();
        assert_eq!(stats.total_renders, 1);
        assert_eq!(stats.notarized_renders, 1);
        assert!(stats.total_segments_emitted >= 1);
    }
}
