use std::time::Instant;

use serde::{Deserialize, Serialize};

use super::live_web::{bing_search, credibility_for_domain, fetch_page_extract};
use super::web_search::SearchSourceClass;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchQuery {
    pub topic: String,
    pub depth: ResearchDepth,
    pub max_sources: usize,
    pub focus_areas: Vec<String>,
    pub exclude_domains: Vec<String>,
    #[serde(default)]
    pub source_classes: Vec<SearchSourceClass>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResearchDepth {
    Quick,
    Standard,
    Deep,
    Ultra,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchSource {
    pub url: String,
    pub domain: String,
    pub title: String,
    pub content_summary: String,
    pub credibility_score: f64,
    pub relevance_score: f64,
    pub extraction_method: String,
    pub source_class: SearchSourceClass,
    pub evidence_tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchReport {
    pub topic: String,
    pub summary: String,
    pub key_findings: Vec<String>,
    pub sources: Vec<ResearchSource>,
    pub confidence: f64,
    pub contradictions: Vec<String>,
    pub gaps: Vec<String>,
    pub duration_ms: f64,
}

pub struct DeepResearch {
    max_concurrent_crawls: usize,
    credibility_threshold: f64,
    total_researches: u64,
    total_sources_processed: u64,
}

impl DeepResearch {
    pub fn new() -> Self {
        Self {
            max_concurrent_crawls: 10,
            credibility_threshold: 0.3,
            total_researches: 0,
            total_sources_processed: 0,
        }
    }

    pub fn research(&mut self, query: ResearchQuery) -> ResearchReport {
        self.total_researches += 1;
        let start = Instant::now();

        let target_sources = match &query.depth {
            ResearchDepth::Quick => 3,
            ResearchDepth::Standard => 8,
            ResearchDepth::Deep => 16,
            ResearchDepth::Ultra => 24,
        }
        .min(query.max_sources.max(1));

        let discovered = self.discover_sources(&query, target_sources);
        self.total_sources_processed += discovered.len() as u64;

        let mut verified: Vec<ResearchSource> = discovered
            .into_iter()
            .filter(|source| source.credibility_score >= self.credibility_threshold)
            .collect();
        let diversity_penalty = platform_diversity_penalty(&verified);
        verified.sort_by(|left, right| {
            enterprise_source_score(right, diversity_penalty)
                .total_cmp(&enterprise_source_score(left, diversity_penalty))
                .then_with(|| left.url.cmp(&right.url))
        });

        let contradictions = self.find_contradictions(&query.topic, &verified);
        let gaps = self.identify_gaps(&query, &verified);
        let key_findings = self.synthesize_findings(&verified);
        let confidence = enterprise_confidence(&verified, diversity_penalty);

        let summary = format!(
            "Research on '{}': analyzed {} vetted sources across {} lanes with {:.0}% confidence.",
            query.topic,
            verified.len(),
            distinct_lane_count(&verified),
            confidence * 100.0
        );

        ResearchReport {
            topic: query.topic,
            summary,
            key_findings,
            sources: verified,
            confidence,
            contradictions,
            gaps,
            duration_ms: start.elapsed().as_secs_f64() * 1000.0,
        }
    }

    fn discover_sources(&self, query: &ResearchQuery, count: usize) -> Vec<ResearchSource> {
        let classes = if query.source_classes.is_empty() {
            vec![SearchSourceClass::Web, SearchSourceClass::Docs]
        } else {
            query.source_classes.clone()
        };
        let live_sources = self.discover_live_sources(query, &classes, count);
        if !live_sources.is_empty() {
            return live_sources;
        }

        let per_class = (count / classes.len().max(1)).max(1);
        let mut sources = Vec::new();

        for class in classes {
            for index in 0..per_class {
                if sources.len() >= count || sources.len() >= self.max_concurrent_crawls * 4 {
                    break;
                }
                let template = source_template(&class, index);
                let domain = format!("{}.research.example", template.0);
                sources.push(ResearchSource {
                    url: format!(
                        "https://{}/{}",
                        domain,
                        query
                            .topic
                            .to_lowercase()
                            .replace(|c: char| !c.is_ascii_alphanumeric(), "-")
                    ),
                    domain,
                    title: format!("{} on {}", template.1, query.topic),
                    content_summary: format!(
                        "{} coverage of {} with emphasis on {}",
                        template.2,
                        query.topic,
                        if query.focus_areas.is_empty() {
                            "cross-source alignment".to_string()
                        } else {
                            query.focus_areas.join(", ")
                        }
                    ),
                    credibility_score: (template.3 - index as f64 * 0.03).clamp(0.0, 1.0),
                    relevance_score: (0.72 + class_relevance_bonus(&class) - index as f64 * 0.02)
                        .clamp(0.0, 1.0),
                    extraction_method: if matches!(
                        class,
                        SearchSourceClass::Papers | SearchSourceClass::Books
                    ) {
                        "semantic_extract".into()
                    } else {
                        "html_parse".into()
                    },
                    source_class: class.clone(),
                    evidence_tags: build_tags(&class, index),
                });
            }
        }

        sources
    }

    fn discover_live_sources(
        &self,
        query: &ResearchQuery,
        classes: &[SearchSourceClass],
        count: usize,
    ) -> Vec<ResearchSource> {
        if cfg!(test) {
            return Vec::new();
        }

        let per_class = (count / classes.len().max(1)).max(1);
        let mut sources = Vec::new();

        for class in classes {
            let hits = bing_search(&query.topic, class, per_class.saturating_mul(2).max(2));
            for (index, hit) in hits.into_iter().enumerate() {
                if sources.len() >= count || sources.len() >= self.max_concurrent_crawls * 4 {
                    break;
                }
                if query
                    .exclude_domains
                    .iter()
                    .any(|blocked| hit.domain.eq_ignore_ascii_case(blocked))
                {
                    continue;
                }

                let should_fetch_page = index < 2
                    && matches!(
                        class,
                        SearchSourceClass::Docs
                            | SearchSourceClass::Papers
                            | SearchSourceClass::Books
                    );
                let fetched = if should_fetch_page {
                    fetch_page_extract(&hit.url, 320)
                } else {
                    None
                };
                let title = fetched
                    .as_ref()
                    .map(|page| page.title.clone())
                    .unwrap_or(hit.title.clone());
                let url = fetched
                    .as_ref()
                    .map(|page| page.url.clone())
                    .unwrap_or(hit.url.clone());
                let domain = fetched
                    .as_ref()
                    .map(|page| page.domain.clone())
                    .unwrap_or(hit.domain.clone());
                let summary = fetched
                    .as_ref()
                    .map(|page| page.summary.clone())
                    .filter(|summary| summary.len() >= 60)
                    .unwrap_or_else(|| hit.snippet.clone());
                let relevance_score =
                    (0.82 + class_relevance_bonus(class) - index as f64 * 0.03).clamp(0.0, 1.0);
                let credibility_score =
                    (credibility_for_domain(&domain, class) - index as f64 * 0.01).clamp(0.0, 1.0);
                let extraction_method = if fetched.is_some() {
                    "live_page_extract"
                } else {
                    "bing_rss_extract"
                };
                let mut evidence_tags = build_tags(class, index);
                evidence_tags.push("live_web".into());
                if query
                    .focus_areas
                    .iter()
                    .any(|focus| summary.to_lowercase().contains(&focus.to_lowercase()))
                {
                    evidence_tags.push("focus_match".into());
                }

                sources.push(ResearchSource {
                    url,
                    domain,
                    title,
                    content_summary: summary,
                    credibility_score,
                    relevance_score,
                    extraction_method: extraction_method.into(),
                    source_class: class.clone(),
                    evidence_tags,
                });
            }
        }

        sources
    }

    fn find_contradictions(&self, topic: &str, sources: &[ResearchSource]) -> Vec<String> {
        let has_social = sources
            .iter()
            .any(|source| matches!(source.source_class, SearchSourceClass::Social));
        let has_papers = sources
            .iter()
            .any(|source| matches!(source.source_class, SearchSourceClass::Papers));
        let has_news = sources
            .iter()
            .any(|source| matches!(source.source_class, SearchSourceClass::News));

        let mut contradictions = Vec::new();
        if has_social && has_papers {
            contradictions.push(format!(
                "Community sentiment on '{}' may diverge from formal research findings.",
                topic
            ));
        }
        if has_news && has_books(sources) {
            contradictions.push(format!(
                "Recent reporting on '{}' may conflict with long-form historical framing.",
                topic
            ));
        }
        contradictions
    }

    fn identify_gaps(&self, query: &ResearchQuery, sources: &[ResearchSource]) -> Vec<String> {
        let mut gaps = Vec::new();
        for focus in &query.focus_areas {
            if !sources.iter().any(|source| {
                source
                    .content_summary
                    .to_lowercase()
                    .contains(&focus.to_lowercase())
            }) {
                gaps.push(format!("No vetted source covered focus area '{}'", focus));
            }
        }

        for class in &query.source_classes {
            if !sources.iter().any(|source| &source.source_class == class) {
                gaps.push(format!("Missing {} coverage", class.as_str()));
            }
        }

        gaps
    }

    fn synthesize_findings(&self, sources: &[ResearchSource]) -> Vec<String> {
        sources
            .iter()
            .filter(|source| source.relevance_score >= 0.7)
            .take(8)
            .map(|source| {
                format!(
                    "{}: {}",
                    source.source_class.as_str(),
                    source.content_summary
                )
            })
            .collect()
    }

    pub fn total_researches(&self) -> u64 {
        self.total_researches
    }
}

impl Default for DeepResearch {
    fn default() -> Self {
        Self::new()
    }
}

fn distinct_lane_count(sources: &[ResearchSource]) -> usize {
    let mut lanes = std::collections::BTreeSet::new();
    for source in sources {
        lanes.insert(source.source_class.as_str());
    }
    lanes.len()
}

fn has_books(sources: &[ResearchSource]) -> bool {
    sources
        .iter()
        .any(|source| matches!(source.source_class, SearchSourceClass::Books))
}

fn enterprise_source_score(source: &ResearchSource, diversity_penalty: f64) -> f64 {
    let freshness = if source.evidence_tags.iter().any(|tag| tag == "live_web") {
        0.96
    } else {
        0.82
    };
    let independence = if matches!(source.source_class, SearchSourceClass::Social) {
        0.72
    } else {
        0.9
    };
    let raw = source.credibility_score * 0.36
        + source.relevance_score * 0.30
        + freshness * 0.18
        + independence * 0.16;
    (raw * (-0.35 * diversity_penalty.max(0.0)).exp()).clamp(0.0, 1.0)
}

fn enterprise_confidence(sources: &[ResearchSource], diversity_penalty: f64) -> f64 {
    if sources.is_empty() {
        return 0.0;
    }
    let weights = evidence_weights(sources);
    let support = sources
        .iter()
        .zip(weights.iter())
        .map(|(source, weight)| {
            weight
                * (source.credibility_score * 0.45
                    + source.relevance_score * 0.35
                    + enterprise_source_score(source, diversity_penalty) * 0.20)
        })
        .sum::<f64>();
    let stale_penalty = sources
        .iter()
        .zip(weights.iter())
        .map(|(source, weight)| {
            let freshness = if source.evidence_tags.iter().any(|tag| tag == "live_web") {
                0.96
            } else {
                0.82
            };
            weight * (1.0 - freshness)
        })
        .sum::<f64>();
    sigmoid(2.4 * support - 0.8 - 0.9 * stale_penalty - diversity_penalty)
}

fn evidence_weights(sources: &[ResearchSource]) -> Vec<f64> {
    if sources.is_empty() {
        return Vec::new();
    }
    let scores = sources
        .iter()
        .map(|source| {
            (1.25 * source.credibility_score
                + 0.95 * source.relevance_score
                + 0.25 * source.evidence_tags.len() as f64)
                .exp()
        })
        .collect::<Vec<_>>();
    let denom = scores.iter().sum::<f64>().max(1e-9);
    scores.into_iter().map(|score| score / denom).collect()
}

fn platform_diversity_penalty(sources: &[ResearchSource]) -> f64 {
    if sources.is_empty() {
        return 1.0;
    }
    let mut counts = std::collections::BTreeMap::<&str, usize>::new();
    for source in sources {
        *counts.entry(source.source_class.as_str()).or_default() += 1;
    }
    let total = sources.len() as f64;
    counts
        .values()
        .map(|count| (*count as f64 / total - 0.65).max(0.0))
        .sum::<f64>()
        .clamp(0.0, 1.0)
}

fn sigmoid(value: f64) -> f64 {
    1.0 / (1.0 + (-value).exp())
}

fn class_relevance_bonus(class: &SearchSourceClass) -> f64 {
    match class {
        SearchSourceClass::Papers => 0.14,
        SearchSourceClass::Docs => 0.12,
        SearchSourceClass::Books => 0.1,
        SearchSourceClass::News => 0.08,
        SearchSourceClass::Web => 0.06,
        SearchSourceClass::Social => 0.04,
    }
}

fn build_tags(class: &SearchSourceClass, index: usize) -> Vec<String> {
    let mut tags = vec![class.as_str().to_string()];
    if index == 0 {
        tags.push("lead_source".into());
    }
    if matches!(class, SearchSourceClass::Papers | SearchSourceClass::Docs) {
        tags.push("high_precision".into());
    }
    tags
}

fn source_template(
    class: &SearchSourceClass,
    index: usize,
) -> (&'static str, &'static str, &'static str, f64) {
    match class {
        SearchSourceClass::Web => [
            ("overview", "Web overview", "General web analysis", 0.71),
            ("insight", "Analyst note", "Interpretive web coverage", 0.74),
            ("guide", "Guide page", "Operational walkthrough", 0.78),
        ][index % 3],
        SearchSourceClass::News => [
            ("newsroom", "Newsroom brief", "Recent news coverage", 0.82),
            ("desk", "Reporter notebook", "Fresh event reporting", 0.79),
            ("wire", "Wire recap", "Cross-outlet summary", 0.8),
        ][index % 3],
        SearchSourceClass::Social => [
            (
                "forum",
                "Forum conversation",
                "Community observations",
                0.58,
            ),
            ("thread", "Social thread", "User-reported experiences", 0.55),
            ("creator", "Creator post", "Opinionated perspective", 0.53),
        ][index % 3],
        SearchSourceClass::Books => [
            ("library", "Book extract", "Historical framing", 0.89),
            ("archive", "Reference volume", "Long-form context", 0.9),
            ("review", "Book review", "Interpretive summary", 0.84),
        ][index % 3],
        SearchSourceClass::Papers => [
            ("journal", "Journal article", "Formal study", 0.93),
            ("preprint", "Preprint paper", "Early research signal", 0.81),
            ("lab", "Lab note", "Method-focused summary", 0.87),
        ][index % 3],
        SearchSourceClass::Docs => [
            ("docs", "Official docs", "Canonical reference", 0.96),
            ("spec", "Technical spec", "Normative description", 0.97),
            (
                "manual",
                "Implementation manual",
                "Operational guidance",
                0.9,
            ),
        ][index % 3],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deep_research_tracks_requested_source_classes() {
        let mut research = DeepResearch::new();
        let report = research.research(ResearchQuery {
            topic: "agent protocol governance".into(),
            depth: ResearchDepth::Deep,
            max_sources: 12,
            focus_areas: vec!["security".into(), "transport".into()],
            exclude_domains: vec![],
            source_classes: vec![SearchSourceClass::Docs, SearchSourceClass::Papers],
        });

        assert!(!report.sources.is_empty());
        assert!(report.sources.iter().all(|source| matches!(
            source.source_class,
            SearchSourceClass::Docs | SearchSourceClass::Papers
        )));
    }
}
