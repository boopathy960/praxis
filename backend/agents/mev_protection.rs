// ─────────────────────────────────────────────────────────────
// MEV Protection Engine
// ─────────────────────────────────────────────────────────────
use crate::chain::transaction::Transaction;
use crate::crypto::hash::sha3_256_hex;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize)]
pub enum MEVType {
    FrontRunning,
    Sandwich,
    BackRunning,
    Arbitrage,
}

#[derive(Debug, Clone, Serialize)]
pub struct MEVAlert {
    pub mev_type: MEVType,
    pub severity: f64,
    pub suspect_tx: String,
    pub victim_tx: String,
    pub estimated_profit: f64,
    pub description: String,
}

pub struct CommitRevealTx {
    pub commit_hash: String,
    pub sender: String,
    pub committed_at: f64,
    pub revealed: bool,
    pub tx_data: Option<serde_json::Value>,
}

pub struct MEVProtectionEngine {
    pending_commits: HashMap<String, CommitRevealTx>,
    alerts: Vec<MEVAlert>,
    protected_count: usize,
    volume_detected: f64,
    tx_arrivals: Vec<(f64, String)>,
}

impl MEVProtectionEngine {
    pub fn new() -> Self {
        Self {
            pending_commits: HashMap::new(),
            alerts: Vec::new(),
            protected_count: 0,
            volume_detected: 0.0,
            tx_arrivals: Vec::new(),
        }
    }

    /// Commit a hidden transaction (phase 1).
    pub fn commit_transaction(&mut self, sender: &str, tx_data: serde_json::Value) -> String {
        let nonce: [u8; 16] = rand::random();
        let data = format!("{}{}", tx_data, crate::crypto::hash::hex_encode(&nonce));
        let hash = sha3_256_hex(data.as_bytes());

        self.pending_commits.insert(
            hash.clone(),
            CommitRevealTx {
                commit_hash: hash.clone(),
                sender: sender.to_string(),
                committed_at: chrono::Utc::now().timestamp_millis() as f64 / 1000.0,
                revealed: false,
                tx_data: Some(tx_data),
            },
        );
        hash
    }

    /// Reveal a committed transaction (phase 2).
    pub fn reveal_transaction(&mut self, hash: &str) -> Option<serde_json::Value> {
        let commit = self.pending_commits.get_mut(hash)?;
        if commit.revealed {
            return None;
        }
        commit.revealed = true;
        self.protected_count += 1;
        commit.tx_data.clone()
    }

    /// Scan mempool for MEV patterns.
    pub fn scan_mempool(&mut self, txs: &[Transaction]) -> Vec<MEVAlert> {
        let mut alerts = Vec::new();
        let now = chrono::Utc::now().timestamp_millis() as f64 / 1000.0;
        for tx in txs {
            self.tx_arrivals.push((now, tx.tx_hash.clone()));
        }
        if self.tx_arrivals.len() > 1000 {
            self.tx_arrivals.drain(..self.tx_arrivals.len() - 1000);
        }

        // Detect front-running: same sender-receiver, short gap
        let mut pairs: HashMap<String, Vec<&Transaction>> = HashMap::new();
        for tx in txs {
            pairs
                .entry(format!("{}-{}", tx.sender, tx.receiver))
                .or_default()
                .push(tx);
        }
        for (_, group) in &pairs {
            if group.len() >= 2 {
                for w in group.windows(2) {
                    let gap = (w[1].timestamp - w[0].timestamp).abs();
                    if gap < 2.0 {
                        let ratio = (w[1].amount - w[0].amount).abs() / w[0].amount.max(0.001);
                        if ratio > 0.1 {
                            let alert = MEVAlert {
                                mev_type: MEVType::FrontRunning,
                                severity: ratio.min(1.0),
                                suspect_tx: w[1].tx_hash.clone(),
                                victim_tx: w[0].tx_hash.clone(),
                                estimated_profit: (w[1].amount - w[0].amount).abs(),
                                description: format!(
                                    "Front-run: {:.2}s gap, {:.1}% change",
                                    gap,
                                    ratio * 100.0
                                ),
                            };
                            self.volume_detected += alert.estimated_profit;
                            alerts.push(alert);
                        }
                    }
                }
            }
        }
        self.alerts.extend(alerts.clone());
        alerts
    }

    /// Enforce fair FIFO ordering.
    pub fn enforce_fair_ordering(&self, mut txs: Vec<Transaction>) -> Vec<Transaction> {
        let arrivals: HashMap<String, f64> = self
            .tx_arrivals
            .iter()
            .cloned()
            .map(|(t, h)| (h, t))
            .collect();
        txs.sort_by(|a, b| {
            let ta = arrivals.get(&a.tx_hash).copied().unwrap_or(a.timestamp);
            let tb = arrivals.get(&b.tx_hash).copied().unwrap_or(b.timestamp);
            ta.partial_cmp(&tb).unwrap_or(std::cmp::Ordering::Equal)
        });
        txs
    }

    pub fn get_stats(&self) -> MEVStats {
        MEVStats {
            total_alerts: self.alerts.len(),
            volume_detected: self.volume_detected,
            protected_transactions: self.protected_count,
            pending_commits: self
                .pending_commits
                .values()
                .filter(|c| !c.revealed)
                .count(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct MEVStats {
    pub total_alerts: usize,
    pub volume_detected: f64,
    pub protected_transactions: usize,
    pub pending_commits: usize,
}
