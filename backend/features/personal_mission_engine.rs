use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde::{Deserialize, Serialize};

use crate::crypto::hash::sha3_256_hex;
use crate::features::abyss_crawler_features::AbyssMissionRequest;

const DEFAULT_PERSONAL_MAX_PAGES: usize = 4;
const DEFAULT_TIMEOUT_MS: u64 = 8_000;
const DEFAULT_MAX_RESPONSE_BYTES: usize = 512 * 1024;
const DEFAULT_MAX_RETRIES: u8 = 1;
const DEFAULT_SEGMENT_LIMIT: usize = 24;
const DEFAULT_LINK_LIMIT: usize = 24;
const DEFAULT_REQUESTS_PER_MINUTE: u32 = 18;
const MAX_RECENT_MISSIONS: usize = 24;
const MAX_RECENT_IMPROVEMENTS: usize = 32;

#[derive(Debug, Clone, Deserialize)]
pub struct PersonalMissionRequest {
    pub objective: String,
    #[serde(default)]
    pub seed_urls: Vec<String>,
    #[serde(default)]
    pub allowed_domains: Vec<String>,
    #[serde(default)]
    pub current_focus: Option<String>,
    #[serde(default)]
    pub open_files: Vec<String>,
    #[serde(default)]
    pub local_focus_tags: Vec<String>,
    #[serde(default)]
    pub local_assets: Vec<String>,
    #[serde(default)]
    pub threat_signals: Vec<String>,
    #[serde(default)]
    pub owned_assets: Vec<String>,
    #[serde(default)]
    pub service_offers: Vec<String>,
    #[serde(default)]
    pub budget_receiver: Option<String>,
    #[serde(default)]
    pub budget_amount: Option<f64>,
    #[serde(default)]
    pub budget_currency: Option<String>,
    #[serde(default = "default_true")]
    pub private_mode: bool,
    #[serde(default = "default_personal_max_pages")]
    pub max_pages: usize,
    #[serde(default = "default_true")]
    pub continue_on_error: bool,
    #[serde(default = "default_true")]
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
    #[serde(default = "default_true")]
    pub persist_report: bool,
    #[serde(default = "default_user_agent")]
    pub user_agent: String,
    #[serde(default)]
    pub notarize_to_chain: bool,
    #[serde(default)]
    pub attestor: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LearnedDomainSignal {
    pub domain: String,
    pub missions: u64,
    pub avg_readiness: f64,
    pub avg_fitness: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PersonalImprovementRecord {
    pub component: String,
    pub recommendation: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PersonalMissionDigest {
    pub mission_id: String,
    pub objective: String,
    pub readiness_score: f64,
    pub fitness: f64,
    pub pages_crawled: usize,
    pub domains: Vec<String>,
    pub next_actions: Vec<String>,
    pub completed_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PersonalMissionStatus {
    pub total_missions: u64,
    pub successful_missions: u64,
    pub llm_free: bool,
    pub gpu_free: bool,
    pub last_mission_at: i64,
    pub learned_domains: Vec<LearnedDomainSignal>,
    pub recent_missions: Vec<PersonalMissionDigest>,
    pub recent_improvements: Vec<PersonalImprovementRecord>,
    pub guardrails: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct PersonalMissionRecordInput {
    pub mission_id: String,
    pub objective: String,
    pub domains: Vec<String>,
    pub readiness_score: f64,
    pub fitness: f64,
    pub pages_crawled: usize,
    pub next_actions: Vec<String>,
    pub weak_components: Vec<String>,
    pub improvement_recommendations: Vec<String>,
    pub completed_at: i64,
}

#[derive(Debug, Clone, Default)]
pub struct PersonalMissionEngine {
    total_missions: u64,
    successful_missions: u64,
    last_mission_at: i64,
    domain_signals: BTreeMap<String, (u64, f64, f64)>,
    recent_missions: VecDeque<PersonalMissionDigest>,
    recent_improvements: VecDeque<PersonalImprovementRecord>,
}

impl PersonalMissionRequest {
    #[must_use]
    pub fn abyss_request(&self) -> Option<AbyssMissionRequest> {
        (!self.seed_urls.is_empty()).then(|| AbyssMissionRequest {
            objective: self.objective.clone(),
            seed_urls: self.seed_urls.clone(),
            allowed_domains: self.allowed_domains.clone(),
            follow_pagination: true,
            max_pages: self.max_pages.max(1),
            continue_on_error: self.continue_on_error,
            respect_robots: self.respect_robots,
            timeout_ms: self.timeout_ms,
            max_response_bytes: self.max_response_bytes,
            max_retries: self.max_retries,
            max_segments: self.max_segments,
            max_links: self.max_links,
            requests_per_minute: self.requests_per_minute,
            persist_report: self.persist_report,
            user_agent: self.user_agent.clone(),
            local_focus_tags: self.local_focus_tags.clone(),
            enable_truth_triangulation: true,
            enable_archive_guidance: true,
            notarize_to_chain: self.notarize_to_chain,
            attestor: self.attestor.clone(),
        })
    }
}

impl PersonalMissionEngine {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_mission(&mut self, input: PersonalMissionRecordInput) -> PersonalMissionDigest {
        self.total_missions = self.total_missions.saturating_add(1);
        self.successful_missions = self.successful_missions.saturating_add(1);
        self.last_mission_at = input.completed_at;

        let domains = normalize_domains(&input.domains);
        for domain in &domains {
            let entry = self
                .domain_signals
                .entry(domain.clone())
                .or_insert((0, 0.0, 0.0));
            entry.0 = entry.0.saturating_add(1);
            entry.1 += input.readiness_score;
            entry.2 += input.fitness;
        }

        let digest = PersonalMissionDigest {
            mission_id: input.mission_id,
            objective: input.objective,
            readiness_score: input.readiness_score,
            fitness: input.fitness,
            pages_crawled: input.pages_crawled,
            domains: domains.clone(),
            next_actions: input.next_actions,
            completed_at: input.completed_at,
        };
        self.recent_missions.push_front(digest.clone());
        while self.recent_missions.len() > MAX_RECENT_MISSIONS {
            self.recent_missions.pop_back();
        }

        let recommendations = if input.improvement_recommendations.is_empty() {
            input
                .weak_components
                .into_iter()
                .map(|component| format!("Stabilize {component} before the next autonomous cycle"))
                .collect::<Vec<_>>()
        } else {
            input.improvement_recommendations
        };
        for recommendation in recommendations {
            let component = recommendation
                .split_whitespace()
                .next()
                .unwrap_or("runtime")
                .trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
                .to_ascii_lowercase();
            self.recent_improvements
                .push_front(PersonalImprovementRecord {
                    component,
                    recommendation,
                    created_at: input.completed_at,
                });
        }
        while self.recent_improvements.len() > MAX_RECENT_IMPROVEMENTS {
            self.recent_improvements.pop_back();
        }

        digest
    }

    #[must_use]
    pub fn status(&self) -> PersonalMissionStatus {
        let mut learned_domains = self
            .domain_signals
            .iter()
            .map(
                |(domain, (missions, readiness_total, fitness_total))| LearnedDomainSignal {
                    domain: domain.clone(),
                    missions: *missions,
                    avg_readiness: if *missions == 0 {
                        0.0
                    } else {
                        readiness_total / *missions as f64
                    },
                    avg_fitness: if *missions == 0 {
                        0.0
                    } else {
                        fitness_total / *missions as f64
                    },
                },
            )
            .collect::<Vec<_>>();
        learned_domains.sort_by(|left, right| {
            right
                .avg_fitness
                .total_cmp(&left.avg_fitness)
                .then_with(|| left.domain.cmp(&right.domain))
        });

        PersonalMissionStatus {
            total_missions: self.total_missions,
            successful_missions: self.successful_missions,
            llm_free: true,
            gpu_free: true,
            last_mission_at: self.last_mission_at,
            learned_domains,
            recent_missions: self.recent_missions.iter().cloned().collect(),
            recent_improvements: self.recent_improvements.iter().cloned().collect(),
            guardrails: vec![
                "The personal mission engine is local, CPU-only, and heuristic-driven.".into(),
                "It optimizes for the owner’s workflows, not multi-user delegation.".into(),
                "Unsafe intrusion, evasion, and covert identity features remain disabled.".into(),
                "Weak components are surfaced as reviewable self-improvement proposals.".into(),
            ],
        }
    }
}

fn normalize_domains(domains: &[String]) -> Vec<String> {
    domains
        .iter()
        .flat_map(|domain| {
            domain
                .split(',')
                .map(str::trim)
                .filter(|domain| !domain.is_empty())
                .map(|domain| domain.to_ascii_lowercase())
                .collect::<Vec<_>>()
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn default_true() -> bool {
    true
}

fn default_personal_max_pages() -> usize {
    DEFAULT_PERSONAL_MAX_PAGES
}

fn default_timeout_ms() -> u64 {
    DEFAULT_TIMEOUT_MS
}

fn default_max_response_bytes() -> usize {
    DEFAULT_MAX_RESPONSE_BYTES
}

fn default_max_retries() -> u8 {
    DEFAULT_MAX_RETRIES
}

fn default_segment_limit() -> usize {
    DEFAULT_SEGMENT_LIMIT
}

fn default_link_limit() -> usize {
    DEFAULT_LINK_LIMIT
}

fn default_requests_per_minute() -> u32 {
    DEFAULT_REQUESTS_PER_MINUTE
}

fn default_user_agent() -> String {
    "AstraPersonal/1.0".into()
}

#[allow(dead_code)]
fn mission_id(objective: &str, completed_at: i64) -> String {
    sha3_256_hex(format!("{objective}:{completed_at}").as_bytes())[..24].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn personal_engine_records_domain_learning() {
        let mut engine = PersonalMissionEngine::new();
        let digest = engine.record_mission(PersonalMissionRecordInput {
            mission_id: "mission-1".into(),
            objective: "research vendor risk".into(),
            domains: vec!["example.com".into(), "mirror.example.com".into()],
            readiness_score: 0.81,
            fitness: 0.78,
            pages_crawled: 3,
            next_actions: vec!["review contradictions".into()],
            weak_components: vec!["reasoning".into()],
            improvement_recommendations: vec!["reasoning needs a deeper replay buffer".into()],
            completed_at: Utc::now().timestamp_millis(),
        });

        assert_eq!(digest.pages_crawled, 3);
        assert_eq!(engine.status().total_missions, 1);
        assert_eq!(engine.status().learned_domains.len(), 2);
        assert_eq!(engine.status().recent_improvements.len(), 1);
    }
}
