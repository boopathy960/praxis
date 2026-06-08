// ─────────────────────────────────────────────────────────────
// Threat Destroy — Threat Neutralization Tool
// ─────────────────────────────────────────────────────────────
// Port of backend/agents/tools/threat_destroyer.py + threat_guard.py

use std::collections::HashMap;
use std::time::Instant;

#[derive(Debug, Clone, PartialEq)]
pub enum ThreatType {
    Malware,
    Phishing,
    Injection,
    DataExfiltration,
    RansomWare,
    Rootkit,
    Spyware,
    DDoS,
    ManInTheMiddle,
    ZeroDay,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ThreatSeverity {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone)]
pub struct ThreatDetection {
    pub id: String,
    pub threat_type: ThreatType,
    pub severity: ThreatSeverity,
    pub description: String,
    pub source: String,
    pub confidence: f64,
    pub indicators: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct NeutralizationResult {
    pub threat_id: String,
    pub action_taken: NeutralizationAction,
    pub success: bool,
    pub details: String,
    pub duration_ms: f64,
}

#[derive(Debug, Clone)]
pub enum NeutralizationAction {
    Quarantine,
    Block,
    Terminate,
    Patch,
    Isolate,
    Report,
    Ignore,
}

/// Threat neutralization engine — detects, classifies, and eliminates threats.
#[allow(dead_code)]
pub struct ThreatDestroyer {
    threat_signatures: HashMap<String, ThreatType>,
    quarantine: Vec<ThreatDetection>,
    total_threats: u64,
    total_neutralized: u64,
    auto_neutralize_threshold: ThreatSeverity,
}

impl ThreatDestroyer {
    pub fn new() -> Self {
        let mut sigs = HashMap::new();
        sigs.insert("eval(".into(), ThreatType::Injection);
        sigs.insert("document.cookie".into(), ThreatType::DataExfiltration);
        sigs.insert("<script>".into(), ThreatType::Injection);
        sigs.insert("base64_decode".into(), ThreatType::Malware);
        sigs.insert("powershell -enc".into(), ThreatType::Malware);
        sigs.insert(".onion".into(), ThreatType::Phishing);
        sigs.insert("DROP TABLE".into(), ThreatType::Injection);
        sigs.insert("UNION SELECT".into(), ThreatType::Injection);

        Self {
            threat_signatures: sigs,
            quarantine: Vec::new(),
            total_threats: 0,
            total_neutralized: 0,
            auto_neutralize_threshold: ThreatSeverity::High,
        }
    }

    /// Scan content for threats.
    pub fn scan(&mut self, content: &str, source: &str) -> Vec<ThreatDetection> {
        let mut detections = Vec::new();
        let content_lower = content.to_lowercase();

        for (signature, threat_type) in &self.threat_signatures {
            if content_lower.contains(&signature.to_lowercase()) {
                self.total_threats += 1;
                detections.push(ThreatDetection {
                    id: format!("THR-{:06}", self.total_threats),
                    threat_type: threat_type.clone(),
                    severity: self.classify_severity(threat_type),
                    description: format!("Detected {:?} indicator: {}", threat_type, signature),
                    source: source.to_string(),
                    confidence: 0.85,
                    indicators: vec![signature.clone()],
                });
            }
        }
        detections
    }

    /// Neutralize a detected threat.
    pub fn neutralize(&mut self, threat: &ThreatDetection) -> NeutralizationResult {
        let start = Instant::now();

        let action = match &threat.severity {
            ThreatSeverity::Critical => NeutralizationAction::Terminate,
            ThreatSeverity::High => NeutralizationAction::Quarantine,
            ThreatSeverity::Medium => NeutralizationAction::Block,
            ThreatSeverity::Low => NeutralizationAction::Report,
        };

        self.quarantine.push(threat.clone());
        self.total_neutralized += 1;

        NeutralizationResult {
            threat_id: threat.id.clone(),
            action_taken: action,
            success: true,
            details: format!("Threat {} neutralized: {}", threat.id, threat.description),
            duration_ms: start.elapsed().as_secs_f64() * 1000.0,
        }
    }

    fn classify_severity(&self, threat_type: &ThreatType) -> ThreatSeverity {
        match threat_type {
            ThreatType::RansomWare | ThreatType::Rootkit | ThreatType::ZeroDay => {
                ThreatSeverity::Critical
            }
            ThreatType::Malware | ThreatType::DataExfiltration => ThreatSeverity::High,
            ThreatType::Injection | ThreatType::Phishing => ThreatSeverity::Medium,
            _ => ThreatSeverity::Low,
        }
    }

    pub fn total_threats(&self) -> u64 {
        self.total_threats
    }
    pub fn total_neutralized(&self) -> u64 {
        self.total_neutralized
    }
    pub fn quarantine_count(&self) -> usize {
        self.quarantine.len()
    }
}

impl Default for ThreatDestroyer {
    fn default() -> Self {
        Self::new()
    }
}
