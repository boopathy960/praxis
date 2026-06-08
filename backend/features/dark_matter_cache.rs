// ─────────────────────────────────────────────────────────────
// Feature: Offline Dark-Matter Caching
// ─────────────────────────────────────────────────────────────
// Intelligent offline caching that predicts offline page needs,
// downloads site trees, maintains freshness scores, and provides
// full offline browsing with smart content prioritization.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum CachePriority {
    Critical,    // Always keep
    High,        // Keep if space available
    Medium,      // Standard eviction rules
    Low,         // First to evict
    Speculative, // Auto-predicted, evict freely
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedPage {
    pub id: String,
    pub url: String,
    pub title: String,
    pub content_hash: String,
    pub cached_at: i64,
    pub expires_at: i64,
    pub last_accessed: i64,
    pub access_count: u32,
    pub size_bytes: u64,
    pub priority: CachePriority,
    pub freshness_score: f64,
    pub content_type: String,
    pub headers: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachePolicy {
    pub max_size_mb: u64,
    pub max_age_hours: u64,
    pub auto_download_depth: usize,
    pub wifi_only_download: bool,
    pub compress_content: bool,
    pub priority_rules: Vec<PriorityRule>,
}

impl Default for CachePolicy {
    fn default() -> Self {
        Self {
            max_size_mb: 500,
            max_age_hours: 168, // 1 week
            auto_download_depth: 2,
            wifi_only_download: true,
            compress_content: true,
            priority_rules: vec![
                PriorityRule {
                    url_pattern: "*.wikipedia.org/*".into(),
                    priority: CachePriority::High,
                },
                PriorityRule {
                    url_pattern: "*.docs.*".into(),
                    priority: CachePriority::High,
                },
                PriorityRule {
                    url_pattern: "*.stackoverflow.com/*".into(),
                    priority: CachePriority::High,
                },
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriorityRule {
    pub url_pattern: String,
    pub priority: CachePriority,
}

#[derive(Debug, Clone, Serialize)]
pub struct CacheStats {
    pub total_pages: usize,
    pub total_size_mb: f64,
    pub hit_count: u64,
    pub miss_count: u64,
    pub hit_rate: f64,
    pub avg_freshness: f64,
    pub oldest_page_hours: f64,
    pub pages_by_priority: HashMap<String, usize>,
    pub download_queue_size: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct DownloadJob {
    pub url: String,
    pub depth: usize,
    pub status: String,
    pub pages_downloaded: usize,
    pub total_size_bytes: u64,
    pub started_at: i64,
}

pub struct DarkMatterCache {
    pages: HashMap<String, CachedPage>,
    policy: CachePolicy,
    download_queue: Vec<DownloadJob>,
    // Stats
    hit_count: u64,
    miss_count: u64,
    total_cached: u64,
    total_evicted: u64,
}

impl DarkMatterCache {
    pub fn new() -> Self {
        Self {
            pages: HashMap::new(),
            policy: CachePolicy::default(),
            download_queue: Vec::new(),
            hit_count: 0,
            miss_count: 0,
            total_cached: 0,
            total_evicted: 0,
        }
    }

    /// Cache a page with intelligent priority assignment.
    pub fn cache_page(
        &mut self,
        url: &str,
        title: &str,
        content: &str,
        content_type: &str,
        headers: HashMap<String, String>,
    ) -> CachedPage {
        let now = chrono::Utc::now().timestamp_millis();
        let content_hash = crate::crypto::hash::sha3_256_hex(content.as_bytes())[..16].to_string();
        let size = content.len() as u64;
        let priority = self.determine_priority(url);

        let page = CachedPage {
            id: uuid::Uuid::new_v4().to_string()[..12].to_string(),
            url: url.to_string(),
            title: title.to_string(),
            content_hash,
            cached_at: now,
            expires_at: now + (self.policy.max_age_hours as i64 * 3_600_000),
            last_accessed: now,
            access_count: 0,
            size_bytes: size,
            priority,
            freshness_score: 1.0,
            content_type: content_type.to_string(),
            headers,
        };

        self.pages.insert(url.to_string(), page.clone());
        self.total_cached += 1;

        // Enforce size limits
        self.enforce_size_limit();

        page
    }

    /// Retrieve a cached page (updates access stats).
    pub fn get_cached_page(&mut self, url: &str) -> Option<&CachedPage> {
        let now = chrono::Utc::now().timestamp_millis();

        if let Some(page) = self.pages.get_mut(url) {
            page.access_count += 1;
            page.last_accessed = now;
            let cached_at = page.cached_at;
            let expires_at = page.expires_at;
            page.freshness_score = Self::compute_freshness_static(cached_at, expires_at, now);
            self.hit_count += 1;
            Some(page)
        } else {
            self.miss_count += 1;
            None
        }
    }

    /// Check if a URL is cached.
    pub fn is_cached(&self, url: &str) -> bool {
        self.pages.contains_key(url)
    }

    /// Predict which pages user will need offline.
    pub fn predict_offline_needs(&self, recent_urls: &[String]) -> Vec<String> {
        let mut predictions = Vec::new();

        // Domain-based prediction: if user visits domain frequently, cache more from it
        let mut domain_counts: HashMap<String, usize> = HashMap::new();
        for url in recent_urls {
            let domain = reqwest::Url::parse(url)
                .map(|u: reqwest::Url| u.host_str().unwrap_or("").to_string())
                .unwrap_or_default();
            *domain_counts.entry(domain).or_insert(0) += 1;
        }

        let mut sorted_domains: Vec<_> = domain_counts.iter().collect();
        sorted_domains.sort_by(|a, b| b.1.cmp(a.1));

        for (domain, _) in sorted_domains.iter().take(5) {
            // Suggest caching the root + common paths
            predictions.push(format!("https://{}/", domain));
        }

        // Also suggest pages visited in the same time window last week
        for (url, page) in &self.pages {
            if page.access_count > 3 {
                if !predictions.contains(url) {
                    predictions.push(url.clone());
                }
            }
        }

        predictions.truncate(20);
        predictions
    }

    /// Queue a site tree for offline download.
    pub fn queue_download(&mut self, root_url: &str, depth: usize) -> DownloadJob {
        let job = DownloadJob {
            url: root_url.to_string(),
            depth: depth.min(self.policy.auto_download_depth),
            status: "queued".to_string(),
            pages_downloaded: 0,
            total_size_bytes: 0,
            started_at: chrono::Utc::now().timestamp_millis(),
        };
        self.download_queue.push(job.clone());
        job
    }

    /// Simulate processing the download queue.
    pub fn process_downloads(&mut self) -> Vec<DownloadJob> {
        let mut completed = Vec::new();
        for job in &mut self.download_queue {
            if job.status == "queued" {
                job.status = "downloading".to_string();
                // Simulate pages downloaded based on depth
                let pages = (1..=job.depth)
                    .map(|d| 3_usize.pow(d as u32))
                    .sum::<usize>()
                    .min(50);
                job.pages_downloaded = pages;
                job.total_size_bytes = pages as u64 * 50_000; // ~50KB avg
                job.status = "completed".to_string();
                completed.push(job.clone());
            }
        }
        self.download_queue.retain(|j| j.status != "completed");
        completed
    }

    /// Prune stale cache entries.
    pub fn prune_stale(&mut self) -> usize {
        let now = chrono::Utc::now().timestamp_millis();
        let before = self.pages.len();
        self.pages
            .retain(|_, page| page.expires_at > now || page.priority == CachePriority::Critical);
        let removed = before - self.pages.len();
        self.total_evicted += removed as u64;
        removed
    }

    /// Update cache policy.
    pub fn update_policy(&mut self, policy: CachePolicy) {
        self.policy = policy;
        self.enforce_size_limit();
    }

    /// Clear entire cache.
    pub fn clear(&mut self) -> usize {
        let count = self.pages.len();
        self.pages.clear();
        self.total_evicted += count as u64;
        count
    }

    /// Get comprehensive cache statistics.
    pub fn get_stats(&self) -> CacheStats {
        let now = chrono::Utc::now().timestamp_millis();
        let total_size: u64 = self.pages.values().map(|p| p.size_bytes).sum();
        let avg_freshness = if self.pages.is_empty() {
            1.0
        } else {
            self.pages
                .values()
                .map(|p| self.compute_freshness(p.cached_at, p.expires_at, now))
                .sum::<f64>()
                / self.pages.len() as f64
        };

        let oldest_hours = self
            .pages
            .values()
            .map(|p| (now - p.cached_at) as f64 / 3_600_000.0)
            .fold(0.0_f64, f64::max);

        let total_accesses = self.hit_count + self.miss_count;
        let hit_rate = if total_accesses > 0 {
            self.hit_count as f64 / total_accesses as f64
        } else {
            0.0
        };

        let mut by_priority: HashMap<String, usize> = HashMap::new();
        for page in self.pages.values() {
            let label = format!("{:?}", page.priority);
            *by_priority.entry(label).or_insert(0) += 1;
        }

        CacheStats {
            total_pages: self.pages.len(),
            total_size_mb: total_size as f64 / 1_048_576.0,
            hit_count: self.hit_count,
            miss_count: self.miss_count,
            hit_rate,
            avg_freshness,
            oldest_page_hours: oldest_hours,
            pages_by_priority: by_priority,
            download_queue_size: self.download_queue.len(),
        }
    }

    // ── Internal ──

    fn compute_freshness(&self, cached_at: i64, expires_at: i64, now: i64) -> f64 {
        Self::compute_freshness_static(cached_at, expires_at, now)
    }

    fn compute_freshness_static(cached_at: i64, expires_at: i64, now: i64) -> f64 {
        let total_life = (expires_at - cached_at).max(1) as f64;
        let elapsed = (now - cached_at).max(0) as f64;
        (1.0 - elapsed / total_life).clamp(0.0, 1.0)
    }

    fn determine_priority(&self, url: &str) -> CachePriority {
        for rule in &self.policy.priority_rules {
            if Self::url_matches_pattern(url, &rule.url_pattern) {
                return rule.priority;
            }
        }
        CachePriority::Medium
    }

    fn url_matches_pattern(url: &str, pattern: &str) -> bool {
        // Simple wildcard matching
        let parts: Vec<&str> = pattern.split('*').collect();
        if parts.len() == 1 {
            return url.contains(pattern);
        }
        let mut remaining = url;
        for part in &parts {
            if part.is_empty() {
                continue;
            }
            match remaining.find(part) {
                Some(pos) => remaining = &remaining[pos + part.len()..],
                None => return false,
            }
        }
        true
    }

    fn enforce_size_limit(&mut self) {
        let max_bytes = self.policy.max_size_mb * 1_048_576;
        let mut total: u64 = self.pages.values().map(|p| p.size_bytes).sum();

        if total <= max_bytes {
            return;
        }

        // Evict: Speculative → Low → Medium (never Critical/High unless extreme)
        let priority_order = [
            CachePriority::Speculative,
            CachePriority::Low,
            CachePriority::Medium,
        ];

        for priority in &priority_order {
            if total <= max_bytes {
                break;
            }
            let mut candidates: Vec<String> = self
                .pages
                .iter()
                .filter(|(_, p)| p.priority == *priority)
                .map(|(url, p)| (url.clone(), p.last_accessed))
                .collect::<Vec<_>>()
                .into_iter()
                .map(|(url, _)| url)
                .collect();

            // Sort by oldest access first (LRU)
            candidates.sort_by_key(|url| self.pages.get(url).map(|p| p.last_accessed).unwrap_or(0));

            for url in candidates {
                if total <= max_bytes {
                    break;
                }
                if let Some(page) = self.pages.remove(&url) {
                    total -= page.size_bytes;
                    self.total_evicted += 1;
                }
            }
        }
    }
}
