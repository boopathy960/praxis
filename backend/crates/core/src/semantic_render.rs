use regex::Regex;
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use url::Url;

use crate::common::{AppError, now_ms, sha3_hex};

const MAX_RENDER_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone, Deserialize)]
pub struct SemanticRenderRequest {
    pub url: String,
    pub html: String,
    #[serde(default = "default_content_type")]
    pub content_type: String,
    #[serde(default)]
    pub notarize_to_chain: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SemanticRenderReport {
    pub render_id: String,
    pub render_profile: String,
    pub proof_hash: String,
    pub url: String,
    pub normalized_url: String,
    pub content_type: String,
    pub content_hash: String,
    pub title: Option<String>,
    pub headings: Vec<String>,
    pub links: Vec<String>,
    pub forms: usize,
    pub script_count: usize,
    pub iframe_count: usize,
    pub inline_event_handlers: usize,
    pub warnings: Vec<String>,
    pub rendered_at_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SemanticRenderStats {
    pub total_renders: u64,
    pub notarized_renders: u64,
    pub last_rendered_at_ms: i64,
}

#[derive(Debug, Default, Clone)]
pub struct SemanticRenderEngine {
    total_renders: u64,
    notarized_renders: u64,
    last_rendered_at_ms: i64,
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
        let rendered_at_ms = now_ms();
        let content_hash = sha3_hex(request.html.as_bytes());
        let render_id = sha3_hex(
            format!(
                "{}:{content_hash}:{rendered_at_ms}",
                normalized_url.as_str()
            )
            .as_bytes(),
        )[..24]
            .to_string();
        let script_count = count_matches(r#"(?is)<script\b"#, &request.html);
        let iframe_count = count_matches(r#"(?is)<iframe\b"#, &request.html);
        let inline_event_handlers = count_matches(r#"(?i)\son[a-z]+\s*="#, &request.html);

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
        let title = extract_title(&request.html);
        let headings = extract_headings(&request.html);
        let links = extract_links(&request.html, &normalized_url);
        let proof_hash = sha3_hex(
            serde_json::json!({
                "render_profile": "enterprise_agent_semantic_render_v1",
                "normalized_url": normalized_url.to_string(),
                "content_type": request.content_type.clone(),
                "content_hash": content_hash,
                "title": title,
                "headings": headings,
                "links": links,
                "forms": count_matches(r#"(?is)<form\b"#, &request.html),
                "script_count": script_count,
                "iframe_count": iframe_count,
                "inline_event_handlers": inline_event_handlers,
                "warnings": warnings,
            })
            .to_string()
            .as_bytes(),
        );

        self.total_renders += 1;
        self.last_rendered_at_ms = rendered_at_ms;

        Ok(SemanticRenderReport {
            render_id,
            render_profile: "enterprise_agent_semantic_render_v1".into(),
            proof_hash,
            url: request.url,
            normalized_url: normalized_url.to_string(),
            content_type: request.content_type,
            content_hash,
            title,
            headings,
            links,
            forms: count_matches(r#"(?is)<form\b"#, &request.html),
            script_count,
            iframe_count,
            inline_event_handlers,
            warnings,
            rendered_at_ms,
        })
    }

    pub fn record_chain_commit(&mut self) {
        self.notarized_renders += 1;
    }

    #[must_use]
    pub fn stats(&self) -> SemanticRenderStats {
        SemanticRenderStats {
            total_renders: self.total_renders,
            notarized_renders: self.notarized_renders,
            last_rendered_at_ms: self.last_rendered_at_ms,
        }
    }
}

fn default_content_type() -> String {
    "text/html".into()
}

fn count_matches(pattern: &str, html: &str) -> usize {
    Regex::new(pattern)
        .expect("static regex should compile")
        .find_iter(html)
        .count()
}

fn extract_title(html: &str) -> Option<String> {
    Regex::new(r#"(?is)<title[^>]*>(.*?)</title>"#)
        .expect("static regex should compile")
        .captures(html)
        .and_then(|captures| captures.get(1))
        .map(|value| strip_tags(value.as_str()))
        .filter(|value| !value.is_empty())
}

fn extract_headings(html: &str) -> Vec<String> {
    Regex::new(r#"(?is)<h[1-6][^>]*>(.*?)</h[1-6]>"#)
        .expect("static regex should compile")
        .captures_iter(html)
        .filter_map(|captures| captures.get(1))
        .map(|value| strip_tags(value.as_str()))
        .filter(|value| !value.is_empty())
        .take(24)
        .collect()
}

fn extract_links(html: &str, base_url: &Url) -> Vec<String> {
    Regex::new(r#"(?is)<a\b[^>]*href\s*=\s*["']([^"']+)["'][^>]*>"#)
        .expect("static regex should compile")
        .captures_iter(html)
        .filter_map(|captures| captures.get(1))
        .filter_map(|value| base_url.join(value.as_str()).ok())
        .map(|url| url.to_string())
        .take(48)
        .collect()
}

fn strip_tags(raw: &str) -> String {
    let without_tags = Regex::new(r#"(?is)<[^>]+>"#)
        .expect("static regex should compile")
        .replace_all(raw, " ");
    without_tags
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_private_host(host: &str) -> bool {
    if matches!(host, "localhost" | "127.0.0.1" | "::1") {
        return true;
    }
    host.parse::<IpAddr>().ok().is_some_and(|ip| match ip {
        IpAddr::V4(addr) => {
            let octets = addr.octets();
            octets[0] == 10
                || (octets[0] == 172 && (16..=31).contains(&octets[1]))
                || (octets[0] == 192 && octets[1] == 168)
                || octets[0] == 127
        }
        IpAddr::V6(addr) => addr.is_loopback() || addr.is_unique_local(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
