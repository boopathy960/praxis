//! Deep Research — decompose a question, spawn sub-agents, scrape exact data.
//!
//! A single [agentic loop](crate::agentic_loop) is one agent doing one task. Real
//! research is wider: a question fans into several precise sub-questions, each
//! needs a *primary source* scraped for an *exact* value, and the values must be
//! cross-checked and woven back together. This module is that orchestrator.
//!
//! One [`investigate`](DeepResearch::investigate) call:
//!   1. **Decomposes** the question into focused sub-questions, each targeting ONE
//!      exact, verifiable datum (a number, price, date, name).
//!   2. **Fans out a sub-agent per sub-question** — each is a full agentic-loop run
//!      with the research toolset ([`http_get`](crate::device_agent), deep-crawl,
//!      web search, shell) that fetches a primary source and extracts the precise
//!      value *with its unit*, citing the URL it used.
//!   3. Optionally **cross-checks** each datum with a second, independent sub-agent
//!      told to use a different source — agreement is consensus, disagreement is
//!      flagged rather than hidden.
//!   4. **Synthesizes** the verified sub-findings into one cited answer.
//!
//! The "exact data" discipline is in the sub-agent prompt: it must identify the
//! *correct field/unit* (the value labelled `USD`, not another currency or token;
//! the figure for the right date) — the fix for a model grabbing the first field
//! it sees. Each sub-agent still runs through the loop's verification + the
//! supervised device layer, so every fetch is real and audited.
//!
//! Honest boundary: sub-agents run **sequentially** (one shared remote model, so a
//! fan-out is paced, not truly parallel), and "verified" means each sub-agent's
//! action provably ran and its postcondition held — not that the model's *reading*
//! of the data is correct. Cross-check + synthesis reduce that gap; they do not
//! erase it.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::agentic_loop::{AgenticLoop, LoopRequest, LoopStep};
use crate::asc2::ReasoningExecutor;
use crate::common::{AppError, now_ms};

/// The toolset every research sub-agent is granted: read-only fetching + scraping
/// (no mutation). `http_get` is the clean, escaping-free way to pull an exact value.
const RESEARCH_TOOLS: &[&str] = &[
    "http_get",
    "deep_crawl",
    "web_search",
    "shell_exec",
    "fs_read",
];
/// Iteration budget per sub-agent — enough to try a couple of sources and conclude.
const SUBAGENT_ITERATIONS: usize = 7;

#[derive(Debug, Clone, Deserialize)]
pub struct ResearchRequest {
    pub question: String,
    /// How many sub-questions to split into (clamped 1..=8, default 3).
    #[serde(default)]
    pub max_subquestions: Option<usize>,
    /// Run a second, independent sub-agent per datum and require agreement.
    /// Doubles the model calls — off by default to respect rate limits.
    #[serde(default)]
    pub cross_check: Option<bool>,
}

/// One sub-question's result — a precise datum, its sources, and how it was checked.
#[derive(Debug, Clone, Serialize)]
pub struct SubFinding {
    pub question: String,
    pub answer: String,
    pub sources: Vec<String>,
    /// The sub-agent's action ran and its postcondition held.
    pub verified: bool,
    /// When cross-checked: did the two independent sub-agents agree on the value?
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agreement: Option<bool>,
    pub sub_agent_runs: usize,
}

/// The synthesized result of a deep-research investigation.
#[derive(Debug, Clone, Serialize)]
pub struct ResearchReport {
    pub question: String,
    pub subquestions: Vec<String>,
    pub findings: Vec<SubFinding>,
    /// The synthesized, cited answer to the main question.
    pub answer: String,
    /// `0.0..=1.0` — share of findings verified, lifted by cross-check agreement.
    pub confidence: f64,
    /// Every distinct source URL the sub-agents actually fetched.
    pub sources: Vec<String>,
    /// Total sub-agent loop runs spawned (≈ sub-questions × sources-per-datum).
    pub sub_agents: usize,
    pub created_at_ms: i64,
}

/// Internal raw output of one sub-agent run, before cross-check folding.
struct SubAgentRun {
    answer: String,
    sources: Vec<String>,
    verified: bool,
}

/// The deep-research orchestrator. Shares the one reasoner (to decompose and
/// synthesize) and the one [agentic loop](crate::agentic_loop) (each sub-agent is
/// a fresh run of it).
#[derive(Clone)]
pub struct DeepResearch {
    reasoner: Arc<dyn ReasoningExecutor>,
    loop_engine: AgenticLoop,
}

impl DeepResearch {
    #[must_use]
    pub fn new(reasoner: Arc<dyn ReasoningExecutor>, loop_engine: AgenticLoop) -> Self {
        Self {
            reasoner,
            loop_engine,
        }
    }

    /// Run the full investigation: decompose → fan out sub-agents → cross-check →
    /// synthesize. Sub-agents run sequentially (one shared model).
    pub async fn investigate(&self, request: ResearchRequest) -> Result<ResearchReport, AppError> {
        let question = request.question.trim().to_string();
        if question.is_empty() {
            return Err(AppError::Validation("research question is empty".into()));
        }
        let k = request.max_subquestions.unwrap_or(3).clamp(1, 8);
        let cross_check = request.cross_check.unwrap_or(false);

        // 1. Decompose into exact-datum sub-questions (fallback: the question itself).
        let subquestions = self.decompose(&question, k).await;

        // 2. Fan out a sub-agent per sub-question (cross-checked if requested).
        let mut findings = Vec::new();
        let mut sub_agents = 0usize;
        for sq in &subquestions {
            let primary = self.run_sub_agent(sq, PRIMARY_HINT).await;
            sub_agents += 1;
            let finding = if cross_check {
                let second = self.run_sub_agent(sq, CROSS_CHECK_HINT).await;
                sub_agents += 1;
                let agreement = answers_agree(&primary.answer, &second.answer);
                // Prefer the verified answer; if both verified and they agree, even better.
                let (answer, verified) = if primary.verified || !second.verified {
                    (primary.answer.clone(), primary.verified)
                } else {
                    (second.answer.clone(), second.verified)
                };
                let mut sources = primary.sources.clone();
                merge_unique(&mut sources, &second.sources);
                SubFinding {
                    question: sq.clone(),
                    answer,
                    sources,
                    verified: verified && agreement,
                    agreement: Some(agreement),
                    sub_agent_runs: 2,
                }
            } else {
                SubFinding {
                    question: sq.clone(),
                    answer: primary.answer,
                    sources: primary.sources,
                    verified: primary.verified,
                    agreement: None,
                    sub_agent_runs: 1,
                }
            };
            findings.push(finding);
        }

        // 3. Synthesize the verified sub-findings into one cited answer.
        let answer = self.synthesize(&question, &findings).await;
        let confidence = confidence_of(&findings);
        let mut sources = Vec::new();
        for finding in &findings {
            merge_unique(&mut sources, &finding.sources);
        }

        Ok(ResearchReport {
            question,
            subquestions,
            findings,
            answer,
            confidence,
            sources,
            sub_agents,
            created_at_ms: now_ms(),
        })
    }

    /// Split the question into focused, single-datum sub-questions. Falls back to
    /// the whole question when no model is configured or the reply is unparseable.
    async fn decompose(&self, question: &str, k: usize) -> Vec<String> {
        let system = format!(
            "You are a research planner. Break the question into at most {k} focused SUB-QUESTIONS, \
             each targeting ONE exact, verifiable datum (a specific number, price, date, name, or \
             fact) that can be scraped from a primary web source. If the question is already a \
             single datum, return just it. Reply with ONLY one JSON object: \
             {{\"subquestions\":[\"...\"]}}."
        );
        let raw = self
            .reasoner
            .complete(system, format!("QUESTION: {question}"))
            .await
            .unwrap_or_default();
        match parse_subquestions(&raw) {
            Some(list) if !list.is_empty() => list.into_iter().take(k).collect(),
            _ => vec![question.to_string()],
        }
    }

    /// Spawn one sub-agent: a full agentic-loop run that fetches a primary source
    /// and extracts the EXACT value (right field/unit) with its source URL.
    async fn run_sub_agent(&self, subquestion: &str, mode_hint: &str) -> SubAgentRun {
        let objective = format!(
            "Research and report the EXACT answer to: {subquestion}\n\
             {mode_hint}\n\
             Prefer the http_get tool to fetch a PRIMARY source (an authoritative API or site). \
             CRITICAL — pick the CORRECT field and unit: choose the value explicitly labelled for \
             what was asked (e.g. the 'USD' figure, NOT another currency or token; the number for \
             the right date/row). Do not grab the first field you see. \
             When you have the exact value, finish immediately with done=true and \
             final_answer = the precise value WITH its unit, followed by '(source: <the URL you used>)'."
        );
        let result = self
            .loop_engine
            .run(LoopRequest {
                objective,
                allow_tools: RESEARCH_TOOLS.iter().map(|t| (*t).to_string()).collect(),
                max_iterations: SUBAGENT_ITERATIONS,
            })
            .await;
        match result {
            Ok(report) => SubAgentRun {
                answer: report.final_answer,
                sources: sources_from_steps(&report.steps),
                verified: report.all_verified
                    && matches!(report.status.as_str(), "completed" | "recalled"),
            },
            Err(error) => SubAgentRun {
                answer: format!("(sub-agent failed: {error})"),
                sources: Vec::new(),
                verified: false,
            },
        }
    }

    /// Weave the verified sub-findings into one direct, cited answer.
    async fn synthesize(&self, question: &str, findings: &[SubFinding]) -> String {
        let dossier = findings
            .iter()
            .map(|f| {
                format!(
                    "- {}\n    value: {}\n    verified: {}\n    sources: {}",
                    f.question,
                    f.answer.trim(),
                    f.verified,
                    if f.sources.is_empty() {
                        "(none)".into()
                    } else {
                        f.sources.join(", ")
                    }
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let system = "You are synthesizing a deep-research answer. Using ONLY the sub-findings \
             below, write a precise, direct answer to the MAIN QUESTION. State exact values WITH \
             their units and cite the sources. If a finding is unverified or the values conflict, \
             say so honestly. NEVER invent data that is not in the findings."
            .to_string();
        let user =
            format!("MAIN QUESTION: {question}\n\nSUB-FINDINGS:\n{dossier}\n\nFinal answer:");
        let synthesized = self
            .reasoner
            .complete(system, user)
            .await
            .unwrap_or_default();
        let synthesized = synthesized.trim();
        if synthesized.is_empty() {
            // Honest fallback: stitch the raw findings together.
            findings
                .iter()
                .map(|f| format!("{}: {}", f.question, f.answer.trim()))
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            synthesized.to_string()
        }
    }
}

const PRIMARY_HINT: &str = "Find it from the most authoritative primary source you can reach.";
const CROSS_CHECK_HINT: &str = "This is an INDEPENDENT cross-check — use a DIFFERENT source than the most obvious one, so the \
     value can be corroborated.";

/// Pull the source URLs a sub-agent actually fetched, from its loop steps.
fn sources_from_steps(steps: &[LoopStep]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for step in steps {
        if !matches!(step.tool.as_str(), "http_get" | "deep_crawl") {
            continue;
        }
        let url = step
            .input
            .get("url")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .or_else(|| {
                step.input
                    .get("urls")
                    .and_then(|v| v.as_array())
                    .and_then(|a| a.first())
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
            });
        if let Some(url) = url {
            if !out.contains(&url) {
                out.push(url);
            }
        }
    }
    out
}

fn merge_unique(into: &mut Vec<String>, more: &[String]) {
    for item in more {
        if !into.contains(item) {
            into.push(item.clone());
        }
    }
}

/// Overall confidence: the share of findings that verified, lifted when a
/// cross-check agreed and dampened when one disagreed.
fn confidence_of(findings: &[SubFinding]) -> f64 {
    if findings.is_empty() {
        return 0.0;
    }
    let mut total: f64 = 0.0;
    for finding in findings {
        let mut score: f64 = if finding.verified { 0.75 } else { 0.4 };
        match finding.agreement {
            Some(true) => score = (score + 0.2).min(0.97),
            Some(false) => score *= 0.6,
            None => {}
        }
        total += score;
    }
    (total / findings.len() as f64).clamp(0.0, 0.97)
}

/// Do two independent sub-agent answers agree? Numeric answers agree within 1%;
/// otherwise fall back to lexical similarity.
fn answers_agree(a: &str, b: &str) -> bool {
    if let (Some(x), Some(y)) = (first_number(a), first_number(b)) {
        let scale = x.abs().max(y.abs()).max(1e-9);
        return (x - y).abs() / scale <= 0.01;
    }
    crate::text_match::similarity(a, b) >= 0.5
}

/// Extract the first numeric value from a string (ignoring thousands separators
/// and a leading currency symbol), for comparing scraped figures.
fn first_number(text: &str) -> Option<f64> {
    let mut buf = String::new();
    let mut seen_digit = false;
    for ch in text.chars() {
        if ch.is_ascii_digit() {
            buf.push(ch);
            seen_digit = true;
        } else if ch == '.' && seen_digit && !buf.contains('.') {
            buf.push(ch);
        } else if ch == ',' && seen_digit {
            // thousands separator — skip
        } else if seen_digit {
            break;
        }
    }
    buf.parse::<f64>().ok()
}

fn parse_subquestions(raw: &str) -> Option<Vec<String>> {
    #[derive(Deserialize)]
    struct Decomp {
        #[serde(default)]
        subquestions: Vec<String>,
    }
    let start = raw.find('{')?;
    let end = raw.rfind('}')?;
    if end < start {
        return None;
    }
    let parsed: Decomp = serde_json::from_str(&raw[start..=end]).ok()?;
    let cleaned: Vec<String> = parsed
        .subquestions
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asc2::{BoxFuture, ReasoningProposal, ReasoningRequest};
    use crate::device_agent::{DeviceCapabilities, DevicePolicy};
    use parking_lot::Mutex;
    use std::collections::VecDeque;

    struct ScriptedExecutor {
        replies: Mutex<VecDeque<String>>,
    }
    impl ScriptedExecutor {
        fn new(replies: Vec<String>) -> Arc<Self> {
            Arc::new(Self {
                replies: Mutex::new(replies.into_iter().collect()),
            })
        }
    }
    impl ReasoningExecutor for ScriptedExecutor {
        fn name(&self) -> &str {
            "scripted"
        }
        fn execute(
            &self,
            _request: ReasoningRequest,
        ) -> BoxFuture<Result<ReasoningProposal, AppError>> {
            Box::pin(async { Err(AppError::Internal("n/a".into())) })
        }
        fn complete(&self, _system: String, _user: String) -> BoxFuture<Result<String, AppError>> {
            let next = self
                .replies
                .lock()
                .pop_front()
                .unwrap_or_else(|| "(no more replies)".to_string());
            Box::pin(async move { Ok(next) })
        }
    }

    #[test]
    fn agreement_and_number_extraction() {
        assert_eq!(first_number("ETH is 1,729.51 USD"), Some(1729.51));
        assert_eq!(first_number("about $487,460.93 (00)"), Some(487460.93));
        assert!(answers_agree("1729.51 USD", "$1,729.50"));
        assert!(!answers_agree("1729 USD", "487460 00"));
    }

    #[actix_web::test]
    async fn investigate_fans_out_subagents_and_synthesizes() {
        let device = Arc::new(DeviceCapabilities::new(DevicePolicy::permissive()));
        // decompose -> one sub-question; sub-agent finishes immediately; synthesize.
        let replies = vec![
            r#"{"subquestions":["What is the current price of Ethereum in USD?"]}"#.to_string(),
            r#"{"done":true,"final_answer":"1729.51 USD (source: https://api.coinbase.com)","reasoning":"read the USD field"}"#.to_string(),
            "Ethereum is currently 1729.51 USD (source: Coinbase).".to_string(),
        ];
        let scripted = ScriptedExecutor::new(replies);
        let loop_engine = AgenticLoop::new(scripted.clone(), device);
        let dr = DeepResearch::new(scripted, loop_engine);
        let report = dr
            .investigate(ResearchRequest {
                question: "What is the current price of Ethereum?".into(),
                max_subquestions: Some(3),
                cross_check: Some(false),
            })
            .await
            .expect("investigate");
        assert_eq!(report.subquestions.len(), 1);
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.sub_agents, 1);
        assert!(report.answer.contains("1729.51"));
        assert!(report.findings[0].answer.contains("1729.51"));
    }

    #[actix_web::test]
    async fn decompose_falls_back_to_the_whole_question() {
        let device = Arc::new(DeviceCapabilities::new(DevicePolicy::permissive()));
        // Unparseable decomposition -> fall back to the question; sub-agent + synth.
        let replies = vec![
            "I cannot do that".to_string(),
            r#"{"done":true,"final_answer":"42","reasoning":"done"}"#.to_string(),
            "The answer is 42.".to_string(),
        ];
        let scripted = ScriptedExecutor::new(replies);
        let loop_engine = AgenticLoop::new(scripted.clone(), device);
        let dr = DeepResearch::new(scripted, loop_engine);
        let report = dr
            .investigate(ResearchRequest {
                question: "what is the meaning".into(),
                max_subquestions: None,
                cross_check: None,
            })
            .await
            .expect("investigate");
        assert_eq!(report.subquestions, vec!["what is the meaning".to_string()]);
        assert_eq!(report.findings.len(), 1);
    }
}
