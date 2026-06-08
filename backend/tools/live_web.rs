use std::time::Duration;

use regex::Regex;
use url::Url;

use super::web_search::SearchSourceClass;

const DEFAULT_USER_AGENT: &str =
    "AstraCoreEngine/2.0 (+https://www.bing.com/search; compatible assistant retrieval)";

#[derive(Debug, Clone)]
pub(crate) struct LiveSearchHit {
    pub title: String,
    pub url: String,
    pub domain: String,
    pub snippet: String,
}

#[derive(Debug, Clone)]
pub(crate) struct PageExtraction {
    pub title: String,
    pub url: String,
    pub domain: String,
    pub summary: String,
}

pub(crate) fn bing_search(
    query: &str,
    class: &SearchSourceClass,
    max_results: usize,
) -> Vec<LiveSearchHit> {
    if cfg!(test) || max_results == 0 {
        return Vec::new();
    }

    let rewritten_query = rewrite_query_for_class(query, class);
    let search_url = match Url::parse_with_params(
        "https://www.bing.com/search",
        &[
            ("format", "rss"),
            ("setlang", "en-us"),
            ("cc", "us"),
            ("q", rewritten_query.as_str()),
        ],
    ) {
        Ok(url) => url,
        Err(_) => return Vec::new(),
    };
    let body = match get_text(search_url.as_str()) {
        Some(body) => body,
        None => return Vec::new(),
    };

    let mut hits = parse_bing_rss_items(&body);
    hits.sort_by(|left, right| {
        query_overlap_score(right, query)
            .cmp(&query_overlap_score(left, query))
            .then_with(|| {
                class_alignment_score(right, class).cmp(&class_alignment_score(left, class))
            })
            .then_with(|| left.domain.cmp(&right.domain))
    });
    let has_query_overlap = hits.iter().any(|hit| query_overlap_score(hit, query) > 0);
    let has_aligned_hits = hits.iter().any(|hit| class_alignment_score(hit, class) > 0);
    let has_strong_class_hits = hits
        .iter()
        .any(|hit| class_alignment_score(hit, class) >= 2);

    hits.into_iter()
        .filter(|hit| !has_query_overlap || query_overlap_score(hit, query) > 0)
        .filter(|hit| !has_aligned_hits || class_alignment_score(hit, class) > 0)
        .filter(|hit| !has_strong_class_hits || class_alignment_score(hit, class) >= 2)
        .filter(|hit| !hit.url.is_empty() && !hit.domain.is_empty())
        .take(max_results)
        .collect()
}

pub(crate) fn fetch_page_extract(url: &str, max_chars: usize) -> Option<PageExtraction> {
    if cfg!(test) || url.trim().is_empty() {
        return None;
    }

    let body = get_text(url)?;
    let domain = domain_for_url(url)?;
    let title = extract_title(&body).unwrap_or_else(|| domain.clone());
    let summary = extract_summary(&body, max_chars)?;

    Some(PageExtraction {
        title,
        url: url.to_string(),
        domain,
        summary,
    })
}

pub(crate) fn domain_for_url(url: &str) -> Option<String> {
    Url::parse(url)
        .ok()
        .and_then(|parsed| parsed.domain().map(|domain| domain.to_string()))
}

pub(crate) fn credibility_for_domain(domain: &str, class: &SearchSourceClass) -> f64 {
    let lower = domain.to_ascii_lowercase();
    let mut score: f64 = match class {
        SearchSourceClass::Docs => 0.83,
        SearchSourceClass::Papers => 0.86,
        SearchSourceClass::Books => 0.78,
        SearchSourceClass::News => 0.72,
        SearchSourceClass::Web => 0.66,
        SearchSourceClass::Social => 0.48,
    };

    if lower.ends_with(".gov") || lower.ends_with(".edu") {
        score += 0.12;
    }
    if lower.ends_with(".org") {
        score += 0.06;
    }
    if lower.contains("docs.") || lower.contains("developer.") || lower.contains("learn.") {
        score += 0.08;
    }
    if lower.contains("arxiv.org")
        || lower.contains("acm.org")
        || lower.contains("ieee.org")
        || lower.contains("springer.com")
    {
        score += 0.1;
    }
    if lower.contains("reddit.com") || lower.contains("quora.com") || lower.contains("medium.com") {
        score -= 0.1;
    }

    score.clamp(0.2, 0.99)
}

fn get_text(url: &str) -> Option<String> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(3))
        .timeout_read(Duration::from_secs(4))
        .timeout_write(Duration::from_secs(4))
        .build();
    let response = agent
        .get(url)
        .set("User-Agent", DEFAULT_USER_AGENT)
        .set("Accept-Language", "en-US,en;q=0.8")
        .call()
        .ok()?;
    response.into_string().ok()
}

fn rewrite_query_for_class(query: &str, class: &SearchSourceClass) -> String {
    match class {
        SearchSourceClass::Docs => format!("{query} documentation guide reference"),
        SearchSourceClass::Papers => format!("{query} research paper study preprint"),
        SearchSourceClass::News => format!("{query} latest news report"),
        SearchSourceClass::Social => format!("{query} forum discussion community reddit"),
        SearchSourceClass::Books => format!("{query} book chapter reference"),
        SearchSourceClass::Web => query.to_string(),
    }
}

fn parse_bing_rss_items(body: &str) -> Vec<LiveSearchHit> {
    let item_re = Regex::new(r"(?s)<item>(.*?)</item>").expect("valid bing rss item regex");
    item_re
        .captures_iter(body)
        .filter_map(|capture| parse_rss_item(capture.get(1)?.as_str()))
        .collect()
}

fn parse_rss_item(item: &str) -> Option<LiveSearchHit> {
    let title = capture_tag(item, "title")?;
    let url = capture_tag(item, "link")?;
    let domain = domain_for_url(&url)?;
    let snippet = capture_tag(item, "description").unwrap_or_else(|| title.clone());

    Some(LiveSearchHit {
        title,
        url,
        domain,
        snippet,
    })
}

fn class_alignment_score(hit: &LiveSearchHit, class: &SearchSourceClass) -> u8 {
    let haystack = format!(
        "{} {} {}",
        hit.domain.to_ascii_lowercase(),
        hit.title.to_ascii_lowercase(),
        hit.snippet.to_ascii_lowercase()
    );

    let needles: &[&str] = match class {
        SearchSourceClass::Docs => &[
            "docs",
            "documentation",
            "developer",
            "reference",
            "guide",
            "manual",
            "learn",
        ],
        SearchSourceClass::Papers => &[
            "paper", "research", "study", "preprint", "journal", "arxiv", "ieee", "acm",
        ],
        SearchSourceClass::News => &[
            "news", "report", "breaking", "press", "reuters", "times", "post", "wire",
        ],
        SearchSourceClass::Social => &[
            "reddit",
            "forum",
            "community",
            "discussion",
            "stack",
            "quora",
            "thread",
        ],
        SearchSourceClass::Books => &[
            "book",
            "chapter",
            "archive",
            "library",
            "isbn",
            "openlibrary",
            "reader",
        ],
        SearchSourceClass::Web => &["guide", "overview", "tutorial", "how to", "reference"],
    };

    let score = needles
        .iter()
        .copied()
        .filter(|needle| !needle.is_empty() && haystack.contains(*needle))
        .count() as u8;

    score
}

fn query_overlap_score(hit: &LiveSearchHit, query: &str) -> u8 {
    let haystack = format!(
        "{} {} {}",
        hit.domain.to_ascii_lowercase(),
        hit.title.to_ascii_lowercase(),
        hit.snippet.to_ascii_lowercase()
    );
    query
        .split_whitespace()
        .map(|token| token.trim_matches(|c: char| !c.is_ascii_alphanumeric()))
        .filter(|token| token.len() >= 4)
        .filter(|token| haystack.contains(&token.to_ascii_lowercase()))
        .count()
        .min(u8::MAX as usize) as u8
}

fn capture_tag(input: &str, tag: &str) -> Option<String> {
    let pattern = format!(r"(?s)<{tag}>(.*?)</{tag}>");
    let re = Regex::new(&pattern).ok()?;
    let raw = re.captures(input)?.get(1)?.as_str();
    let cleaned = decode_html_entities(raw)
        .replace("<![CDATA[", "")
        .replace("]]>", "");
    let normalized = collapse_ws(&strip_tags(&cleaned));
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn extract_title(body: &str) -> Option<String> {
    extract_meta_content(body, "og:title")
        .filter(|value| !value.is_empty())
        .or_else(|| {
            let re = Regex::new(r"(?is)<title[^>]*>(.*?)</title>").ok()?;
            let raw = re.captures(body)?.get(1)?.as_str();
            let title = collapse_ws(&decode_html_entities(&strip_tags(raw)));
            if title.is_empty() {
                None
            } else {
                Some(title)
            }
        })
}

fn extract_summary(body: &str, max_chars: usize) -> Option<String> {
    let preferred = extract_meta_name(body, "description")
        .or_else(|| extract_meta_content(body, "og:description"))
        .filter(|value| value.len() >= 80);
    let summary = preferred.unwrap_or_else(|| {
        let cleaned = cleanup_html(body);
        let excerpt = take_sentence_window(&cleaned, max_chars.max(200));
        if excerpt.is_empty() {
            cleaned.chars().take(max_chars.max(200)).collect()
        } else {
            excerpt
        }
    });

    let bounded = summary.chars().take(max_chars.max(200)).collect::<String>();
    let normalized = collapse_ws(&bounded);
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn extract_meta_name(body: &str, name: &str) -> Option<String> {
    let pattern = format!(
        r#"(?is)<meta[^>]+name=["']{}["'][^>]+content=["']([^"']+)["'][^>]*>"#,
        regex::escape(name)
    );
    let re = Regex::new(&pattern).ok()?;
    let content = re.captures(body)?.get(1)?.as_str();
    let normalized = collapse_ws(&decode_html_entities(content));
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn extract_meta_content(body: &str, property: &str) -> Option<String> {
    let pattern = format!(
        r#"(?is)<meta[^>]+property=["']{}["'][^>]+content=["']([^"']+)["'][^>]*>"#,
        regex::escape(property)
    );
    let re = Regex::new(&pattern).ok()?;
    let content = re.captures(body)?.get(1)?.as_str();
    let normalized = collapse_ws(&decode_html_entities(content));
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn cleanup_html(body: &str) -> String {
    let script_re = Regex::new(r"(?is)<script[^>]*>.*?</script>").expect("valid script regex");
    let style_re = Regex::new(r"(?is)<style[^>]*>.*?</style>").expect("valid style regex");
    let comment_re = Regex::new(r"(?is)<!--.*?-->").expect("valid comment regex");

    let without_scripts = script_re.replace_all(body, " ");
    let without_styles = style_re.replace_all(&without_scripts, " ");
    let without_comments = comment_re.replace_all(&without_styles, " ");
    collapse_ws(&decode_html_entities(&strip_tags(&without_comments)))
}

fn strip_tags(input: &str) -> String {
    let tag_re = Regex::new(r"(?is)<[^>]+>").expect("valid tag regex");
    tag_re.replace_all(input, " ").into_owned()
}

fn take_sentence_window(text: &str, max_chars: usize) -> String {
    let mut summary = String::new();
    for sentence in text.split_terminator(['.', '!', '?']) {
        let sentence = sentence.trim();
        if sentence.len() < 30 {
            continue;
        }
        if !summary.is_empty() {
            summary.push_str(". ");
        }
        summary.push_str(sentence);
        if summary.len() >= max_chars {
            break;
        }
    }
    if !summary.is_empty() && !summary.ends_with('.') {
        summary.push('.');
    }
    summary
}

fn decode_html_entities(input: &str) -> String {
    input
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

fn collapse_ws(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}
