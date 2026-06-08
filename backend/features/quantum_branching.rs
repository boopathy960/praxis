// ─────────────────────────────────────────────────────────────
// Feature: Quantum Tab Execution (Schizophrenic Branching)
// ─────────────────────────────────────────────────────────────
// Fork any tab into parallel quantum branches exploring different
// paths simultaneously. Each branch maintains independent state,
// can be collapsed or merged, and tracks divergence.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum BranchState {
    Active,
    Suspended,
    Collapsed,
    Merged,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantumBranch {
    pub id: String,
    pub parent_branch_id: String,
    pub source_tab_id: String,
    pub url: String,
    pub title: String,
    pub state: BranchState,
    pub divergence_score: f64,
    pub created_at: i64,
    pub last_active_at: i64,
    pub navigation_history: Vec<String>,
    pub depth: usize,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BranchMergeResult {
    pub merged_branch_id: String,
    pub source_branch_ids: Vec<String>,
    pub merged_url: String,
    pub merged_title: String,
    pub total_navigations: usize,
    pub consensus_score: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct BranchTreeNode {
    pub branch: QuantumBranch,
    pub children: Vec<BranchTreeNode>,
    pub total_descendants: usize,
}

pub struct QuantumBranchManager {
    branches: Vec<QuantumBranch>,
    max_branches_per_tab: usize,
    max_total_branches: usize,
    total_forks: u64,
    total_merges: u64,
    total_collapses: u64,
}

impl QuantumBranchManager {
    pub fn new() -> Self {
        Self {
            branches: Vec::new(),
            max_branches_per_tab: 16,
            max_total_branches: 200,
            total_forks: 0,
            total_merges: 0,
            total_collapses: 0,
        }
    }

    /// Fork a new quantum branch from a tab or existing branch.
    pub fn fork(
        &mut self,
        source_tab_id: &str,
        parent_branch_id: &str,
        current_url: &str,
        current_title: &str,
    ) -> QuantumBranch {
        let parent_depth = self
            .branches
            .iter()
            .find(|b| b.id == parent_branch_id)
            .map(|b| b.depth)
            .unwrap_or(0);

        let now = chrono::Utc::now().timestamp_millis();
        let id = uuid::Uuid::new_v4().to_string()[..12].to_string();

        let branch = QuantumBranch {
            id: id.clone(),
            parent_branch_id: parent_branch_id.to_string(),
            source_tab_id: source_tab_id.to_string(),
            url: current_url.to_string(),
            title: format!("⚛ {}", current_title),
            state: BranchState::Active,
            divergence_score: 0.0,
            created_at: now,
            last_active_at: now,
            navigation_history: vec![current_url.to_string()],
            depth: parent_depth + 1,
            metadata: HashMap::new(),
        };

        self.branches.push(branch.clone());
        self.total_forks += 1;

        // Enforce per-tab limits
        let tab_count = self
            .branches
            .iter()
            .filter(|b| b.source_tab_id == source_tab_id && b.state == BranchState::Active)
            .count();
        if tab_count > self.max_branches_per_tab {
            self.prune_tab_branches(source_tab_id);
        }
        if self.branches.len() > self.max_total_branches {
            self.prune_global();
        }

        branch
    }

    /// Navigate within a branch, updating divergence tracking.
    pub fn navigate_branch(
        &mut self,
        branch_id: &str,
        new_url: &str,
        new_title: &str,
    ) -> Option<f64> {
        let parent_history: Option<Vec<String>> = {
            let branch = self.branches.iter().find(|b| b.id == branch_id)?;
            let parent_id = branch.parent_branch_id.clone();
            self.branches
                .iter()
                .find(|b| b.id == parent_id)
                .map(|p| p.navigation_history.clone())
        };

        let branch = self.branches.iter_mut().find(|b| b.id == branch_id)?;
        branch.navigation_history.push(new_url.to_string());
        branch.url = new_url.to_string();
        branch.title = new_title.to_string();
        branch.last_active_at = chrono::Utc::now().timestamp_millis();

        // Compute Jaccard divergence from parent
        if let Some(parent_hist) = parent_history {
            branch.divergence_score =
                Self::jaccard_divergence(&branch.navigation_history, &parent_hist);
        } else {
            branch.divergence_score = (branch.navigation_history.len() as f64 / 10.0).min(1.0);
        }

        Some(branch.divergence_score)
    }

    /// Collapse a branch and all its children.
    pub fn collapse_branch(&mut self, branch_id: &str) -> usize {
        let mut collapsed = 0;
        let children: Vec<String> = self.get_descendant_ids(branch_id);

        for id in std::iter::once(branch_id.to_string()).chain(children) {
            if let Some(b) = self.branches.iter_mut().find(|b| b.id == id) {
                if b.state == BranchState::Active || b.state == BranchState::Suspended {
                    b.state = BranchState::Collapsed;
                    collapsed += 1;
                }
            }
        }
        self.total_collapses += collapsed as u64;
        collapsed
    }

    /// Suspend a branch (pause but retain state).
    pub fn suspend_branch(&mut self, branch_id: &str) -> bool {
        if let Some(b) = self.branches.iter_mut().find(|b| b.id == branch_id) {
            if b.state == BranchState::Active {
                b.state = BranchState::Suspended;
                return true;
            }
        }
        false
    }

    /// Resume a suspended branch.
    pub fn resume_branch(&mut self, branch_id: &str) -> bool {
        if let Some(b) = self.branches.iter_mut().find(|b| b.id == branch_id) {
            if b.state == BranchState::Suspended {
                b.state = BranchState::Active;
                b.last_active_at = chrono::Utc::now().timestamp_millis();
                return true;
            }
        }
        false
    }

    /// Merge multiple branches — picks richest exploration path.
    pub fn merge_branches(&mut self, branch_ids: &[String]) -> BranchMergeResult {
        let mut to_merge: Vec<&QuantumBranch> = self
            .branches
            .iter()
            .filter(|b| branch_ids.contains(&b.id))
            .collect();

        if to_merge.is_empty() {
            return BranchMergeResult {
                merged_branch_id: String::new(),
                source_branch_ids: branch_ids.to_vec(),
                merged_url: String::new(),
                merged_title: "Empty Merge".into(),
                total_navigations: 0,
                consensus_score: 0.0,
            };
        }

        to_merge.sort_by(|a, b| b.navigation_history.len().cmp(&a.navigation_history.len()));
        let winner_id = to_merge[0].id.clone();
        let winner_url = to_merge[0].url.clone();
        let winner_title = to_merge[0].title.clone();

        // Compute consensus: how many branches ended on similar domains
        let mut domain_counts: HashMap<String, usize> = HashMap::new();
        for b in &to_merge {
            let domain = reqwest::Url::parse(&b.url)
                .map(|u: reqwest::Url| u.host_str().unwrap_or("").to_string())
                .unwrap_or_default();
            *domain_counts.entry(domain).or_insert(0) += 1;
        }
        let max_agreement = *domain_counts.values().max().unwrap_or(&0);
        let consensus = max_agreement as f64 / to_merge.len() as f64;

        let total_nav: usize = to_merge.iter().map(|b| b.navigation_history.len()).sum();

        // Mark all as merged
        for id in branch_ids {
            if let Some(b) = self.branches.iter_mut().find(|b| &b.id == id) {
                b.state = BranchState::Merged;
            }
        }
        self.total_merges += 1;

        BranchMergeResult {
            merged_branch_id: winner_id,
            source_branch_ids: branch_ids.to_vec(),
            merged_url: winner_url,
            merged_title: winner_title,
            total_navigations: total_nav,
            consensus_score: consensus,
        }
    }

    /// Get all branches for a specific tab.
    pub fn get_branches_for_tab(&self, tab_id: &str) -> Vec<&QuantumBranch> {
        self.branches
            .iter()
            .filter(|b| b.source_tab_id == tab_id)
            .collect()
    }

    /// Build the full branch tree for a tab.
    pub fn get_branch_tree(&self, tab_id: &str) -> Vec<BranchTreeNode> {
        let tab_branches: Vec<&QuantumBranch> = self
            .branches
            .iter()
            .filter(|b| b.source_tab_id == tab_id)
            .collect();

        let roots: Vec<&QuantumBranch> = tab_branches
            .iter()
            .filter(|b| b.depth == 1)
            .copied()
            .collect();

        roots
            .iter()
            .map(|r| self.build_tree_node(r, &tab_branches))
            .collect()
    }

    /// Get divergence between two specific branches.
    pub fn get_divergence(&self, branch_a: &str, branch_b: &str) -> f64 {
        let a = self.branches.iter().find(|b| b.id == branch_a);
        let b = self.branches.iter().find(|b| b.id == branch_b);
        match (a, b) {
            (Some(a), Some(b)) => {
                Self::jaccard_divergence(&a.navigation_history, &b.navigation_history)
            }
            _ => 1.0,
        }
    }

    pub fn get_stats(&self) -> serde_json::Value {
        let active = self
            .branches
            .iter()
            .filter(|b| b.state == BranchState::Active)
            .count();
        let suspended = self
            .branches
            .iter()
            .filter(|b| b.state == BranchState::Suspended)
            .count();
        serde_json::json!({
            "total_branches": self.branches.len(),
            "active": active,
            "suspended": suspended,
            "total_forks": self.total_forks,
            "total_merges": self.total_merges,
            "total_collapses": self.total_collapses,
        })
    }

    // ── Internal ──

    fn jaccard_divergence(a: &[String], b: &[String]) -> f64 {
        use std::collections::HashSet;
        let set_a: HashSet<&String> = a.iter().collect();
        let set_b: HashSet<&String> = b.iter().collect();
        if set_a.is_empty() && set_b.is_empty() {
            return 0.0;
        }
        let intersection = set_a.intersection(&set_b).count();
        let union = set_a.union(&set_b).count();
        if union == 0 {
            return 0.0;
        }
        1.0 - (intersection as f64 / union as f64)
    }

    fn get_descendant_ids(&self, parent_id: &str) -> Vec<String> {
        let mut result = Vec::new();
        let direct: Vec<String> = self
            .branches
            .iter()
            .filter(|b| b.parent_branch_id == parent_id)
            .map(|b| b.id.clone())
            .collect();
        for child_id in &direct {
            result.push(child_id.clone());
            result.extend(self.get_descendant_ids(child_id));
        }
        result
    }

    fn build_tree_node(&self, branch: &QuantumBranch, all: &[&QuantumBranch]) -> BranchTreeNode {
        let children: Vec<BranchTreeNode> = all
            .iter()
            .filter(|b| b.parent_branch_id == branch.id)
            .map(|child| self.build_tree_node(child, all))
            .collect();
        let desc: usize = children.iter().map(|c| 1 + c.total_descendants).sum();
        BranchTreeNode {
            branch: branch.clone(),
            children,
            total_descendants: desc,
        }
    }

    fn prune_tab_branches(&mut self, tab_id: &str) {
        let mut tab_branches: Vec<(usize, i64)> = self
            .branches
            .iter()
            .enumerate()
            .filter(|(_, b)| b.source_tab_id == tab_id && b.state == BranchState::Suspended)
            .map(|(i, b)| (i, b.last_active_at))
            .collect();
        tab_branches.sort_by_key(|(_, t)| *t);
        for (idx, _) in tab_branches.iter().take(2) {
            if let Some(b) = self.branches.get_mut(*idx) {
                b.state = BranchState::Collapsed;
            }
        }
    }

    fn prune_global(&mut self) {
        self.branches.retain(|b| b.state != BranchState::Collapsed);
        if self.branches.len() > self.max_total_branches {
            self.branches.sort_by_key(|b| b.last_active_at);
            let remove_count = self.branches.len() - self.max_total_branches;
            self.branches.drain(..remove_count);
        }
    }
}
