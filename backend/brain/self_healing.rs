// ─────────────────────────────────────────────────────────────
// Self-Healing Engine — Autonomous Fault Recovery
// ─────────────────────────────────────────────────────────────
// Monitors all subsystems, detects anomalies, and triggers
// automatic recovery actions. No human intervention needed.

use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Critical,
    Dead,
    Recovering,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum HealingAction {
    Restart,
    ClearCache,
    ResetState,
    ReduceLoad,
    IncreaseTimeout,
    FallbackMode,
    Quarantine,
    Escalate,
}

#[derive(Debug, Clone, Serialize)]
pub struct SubsystemHealth {
    pub name: String,
    pub status: HealthStatus,
    pub uptime_secs: u64,
    pub error_count: u64,
    pub last_error: Option<String>,
    pub last_healthy: i64,
    pub latency_p50_ms: f64,
    pub latency_p99_ms: f64,
    pub memory_bytes: u64,
    pub recovery_count: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct HealingEvent {
    pub subsystem: String,
    pub action: HealingAction,
    pub reason: String,
    pub timestamp: i64,
    pub success: bool,
    pub recovery_time_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AnomalyDetection {
    pub subsystem: String,
    pub metric: String,
    pub current_value: f64,
    pub expected_value: f64,
    pub deviation: f64,
    pub severity: f64,
}

pub struct SelfHealingEngine {
    subsystems: HashMap<String, SubsystemHealth>,
    events: Vec<HealingEvent>,
    anomalies: Vec<AnomalyDetection>,
    metric_baselines: HashMap<String, MetricBaseline>,
    healing_policies: Vec<HealingPolicy>,
    total_recoveries: u64,
    total_anomalies: u64,
}

#[derive(Debug, Clone)]
struct MetricBaseline {
    mean: f64,
    std_dev: f64,
    samples: Vec<f64>,
}

impl MetricBaseline {
    fn new() -> Self {
        Self {
            mean: 0.0,
            std_dev: 1.0,
            samples: Vec::new(),
        }
    }

    fn update(&mut self, value: f64) {
        self.samples.push(value);
        if self.samples.len() > 200 {
            let excess = self.samples.len() - 200;
            self.samples.drain(..excess);
        }

        let n = self.samples.len() as f64;
        self.mean = self.samples.iter().sum::<f64>() / n;
        let variance: f64 = self
            .samples
            .iter()
            .map(|x| (x - self.mean).powi(2))
            .sum::<f64>()
            / n;
        self.std_dev = variance.sqrt().max(0.001);
    }

    fn z_score(&self, value: f64) -> f64 {
        (value - self.mean) / self.std_dev
    }
}

#[derive(Debug, Clone)]
struct HealingPolicy {
    condition: PolicyCondition,
    action: HealingAction,
    cooldown_ms: u64,
    last_triggered: i64,
}

#[derive(Debug, Clone)]
enum PolicyCondition {
    ErrorRateAbove(f64),
    LatencyAbove(f64),
    MemoryAbove(u64),
    StatusIs(HealthStatus),
    ConsecutiveErrors(u64),
}

impl SelfHealingEngine {
    pub fn new() -> Self {
        let policies = vec![
            HealingPolicy {
                condition: PolicyCondition::ErrorRateAbove(0.1),
                action: HealingAction::ReduceLoad,
                cooldown_ms: 30_000,
                last_triggered: 0,
            },
            HealingPolicy {
                condition: PolicyCondition::ErrorRateAbove(0.3),
                action: HealingAction::Restart,
                cooldown_ms: 60_000,
                last_triggered: 0,
            },
            HealingPolicy {
                condition: PolicyCondition::LatencyAbove(5000.0),
                action: HealingAction::ClearCache,
                cooldown_ms: 15_000,
                last_triggered: 0,
            },
            HealingPolicy {
                condition: PolicyCondition::LatencyAbove(15000.0),
                action: HealingAction::IncreaseTimeout,
                cooldown_ms: 30_000,
                last_triggered: 0,
            },
            HealingPolicy {
                condition: PolicyCondition::MemoryAbove(500_000_000),
                action: HealingAction::ClearCache,
                cooldown_ms: 60_000,
                last_triggered: 0,
            },
            HealingPolicy {
                condition: PolicyCondition::StatusIs(HealthStatus::Critical),
                action: HealingAction::FallbackMode,
                cooldown_ms: 10_000,
                last_triggered: 0,
            },
            HealingPolicy {
                condition: PolicyCondition::StatusIs(HealthStatus::Dead),
                action: HealingAction::Restart,
                cooldown_ms: 5_000,
                last_triggered: 0,
            },
            HealingPolicy {
                condition: PolicyCondition::ConsecutiveErrors(10),
                action: HealingAction::Quarantine,
                cooldown_ms: 120_000,
                last_triggered: 0,
            },
        ];

        Self {
            subsystems: HashMap::new(),
            events: Vec::new(),
            anomalies: Vec::new(),
            metric_baselines: HashMap::new(),
            healing_policies: policies,
            total_recoveries: 0,
            total_anomalies: 0,
        }
    }

    /// Register a subsystem for health monitoring.
    pub fn register_subsystem(&mut self, name: &str) {
        self.subsystems.insert(
            name.to_string(),
            SubsystemHealth {
                name: name.to_string(),
                status: HealthStatus::Healthy,
                uptime_secs: 0,
                error_count: 0,
                last_error: None,
                last_healthy: chrono::Utc::now().timestamp_millis(),
                latency_p50_ms: 0.0,
                latency_p99_ms: 0.0,
                memory_bytes: 0,
                recovery_count: 0,
            },
        );
    }

    /// Report health metrics for a subsystem.
    pub fn report_health(
        &mut self,
        name: &str,
        latency_ms: f64,
        error: Option<&str>,
        memory_bytes: u64,
    ) -> Vec<HealingAction> {
        let now = chrono::Utc::now().timestamp_millis();

        // Update subsystem health
        if let Some(sub) = self.subsystems.get_mut(name) {
            sub.latency_p50_ms = sub.latency_p50_ms * 0.9 + latency_ms * 0.1;
            sub.latency_p99_ms = sub.latency_p99_ms.max(latency_ms);
            sub.memory_bytes = memory_bytes;

            if let Some(err) = error {
                sub.error_count += 1;
                sub.last_error = Some(err.to_string());
            } else {
                sub.last_healthy = now;
            }

            // Compute status
            let error_rate = if sub.uptime_secs > 0 {
                sub.error_count as f64 / sub.uptime_secs as f64
            } else {
                0.0
            };

            sub.status = if error_rate > 0.5 {
                HealthStatus::Dead
            } else if error_rate > 0.2 || latency_ms > 10000.0 {
                HealthStatus::Critical
            } else if error_rate > 0.05 || latency_ms > 3000.0 {
                HealthStatus::Degraded
            } else {
                HealthStatus::Healthy
            };

            sub.uptime_secs += 1;
        }

        // Anomaly detection via z-score
        let baseline_key = format!("{}:latency", name);
        let baseline = self
            .metric_baselines
            .entry(baseline_key.clone())
            .or_insert_with(MetricBaseline::new);
        baseline.update(latency_ms);

        if baseline.samples.len() > 10 {
            let z = baseline.z_score(latency_ms);
            if z.abs() > 3.0 {
                self.total_anomalies += 1;
                self.anomalies.push(AnomalyDetection {
                    subsystem: name.to_string(),
                    metric: "latency_ms".to_string(),
                    current_value: latency_ms,
                    expected_value: baseline.mean,
                    deviation: z,
                    severity: (z.abs() / 3.0).min(1.0),
                });
                if self.anomalies.len() > 500 {
                    self.anomalies.drain(..250);
                }
            }
        }

        // Policy evaluation
        let mut actions = Vec::new();
        let sub = match self.subsystems.get(name) {
            Some(s) => s.clone(),
            None => return actions,
        };

        for policy in self.healing_policies.iter_mut() {
            if now - policy.last_triggered < policy.cooldown_ms as i64 {
                continue;
            }

            let error_rate = if sub.uptime_secs > 0 {
                sub.error_count as f64 / sub.uptime_secs as f64
            } else {
                0.0
            };

            let triggered = match &policy.condition {
                PolicyCondition::ErrorRateAbove(threshold) => error_rate > *threshold,
                PolicyCondition::LatencyAbove(threshold) => sub.latency_p50_ms > *threshold,
                PolicyCondition::MemoryAbove(threshold) => sub.memory_bytes > *threshold,
                PolicyCondition::StatusIs(status) => sub.status == *status,
                PolicyCondition::ConsecutiveErrors(count) => sub.error_count > *count,
            };

            if triggered {
                policy.last_triggered = now;
                actions.push(policy.action.clone());

                self.events.push(HealingEvent {
                    subsystem: name.to_string(),
                    action: policy.action.clone(),
                    reason: format!("Policy triggered: {:?}", policy.condition),
                    timestamp: now,
                    success: true,
                    recovery_time_ms: 0,
                });

                self.total_recoveries += 1;
            }
        }

        // Update recovery count
        if !actions.is_empty() {
            if let Some(sub) = self.subsystems.get_mut(name) {
                sub.recovery_count += 1;
                sub.status = HealthStatus::Recovering;
            }
        }

        // Trim events
        if self.events.len() > 1000 {
            self.events.drain(..500);
        }

        actions
    }

    /// Get overall system health summary.
    pub fn system_health(&self) -> serde_json::Value {
        let statuses: HashMap<String, String> = self
            .subsystems
            .iter()
            .map(|(k, v)| (k.clone(), format!("{:?}", v.status)))
            .collect();

        let critical: Vec<&str> = self
            .subsystems
            .values()
            .filter(|s| matches!(s.status, HealthStatus::Critical | HealthStatus::Dead))
            .map(|s| s.name.as_str())
            .collect();

        serde_json::json!({
            "overall": if critical.is_empty() { "healthy" } else { "degraded" },
            "subsystems": statuses,
            "critical_subsystems": critical,
            "total_recoveries": self.total_recoveries,
            "total_anomalies": self.total_anomalies,
            "recent_events": self.events.iter().rev().take(10).collect::<Vec<_>>(),
        })
    }

    pub fn get_stats(&self) -> serde_json::Value {
        self.system_health()
    }
}
