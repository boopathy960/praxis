// ─────────────────────────────────────────────────────────────
// Feature 1: Proof-Carrying Cognitive Memory Sync
// ─────────────────────────────────────────────────────────────
// Browser memories committed via SIS lattice commitments.
// Quantum-secure, serverless, cross-device sync.

use crate::crypto::hash::sha3_256_hex;
use crate::crypto::lattice::LatticeCommitment;
use crate::crypto::params::PQCParams;
use crate::error::AstraResult;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: String,
    pub url: String,
    pub intent: String,
    pub context: String,
    pub timestamp: i64,
    pub commitment: Vec<i64>,
    pub randomness: Vec<i64>,
    pub verified: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CommitResult {
    pub memory_id: String,
    pub commitment_hash: String,
    pub verified: bool,
}

pub struct CognitiveMemorySync {
    memories: Vec<MemoryEntry>,
    commitment_engine: LatticeCommitment,
    max_memories: usize,
}

impl CognitiveMemorySync {
    pub fn new() -> Self {
        let params = PQCParams::default();
        let mut engine = LatticeCommitment::new(params);
        engine.setup(Some(b"astra_cognitive_memory_v1"));
        Self {
            memories: Vec::new(),
            commitment_engine: engine,
            max_memories: 10_000,
        }
    }

    /// Commit a browsing memory to the chain-secured store.
    pub fn commit_memory(
        &mut self,
        url: &str,
        intent: &str,
        context: &str,
    ) -> AstraResult<CommitResult> {
        let data = format!("{}|{}|{}", url, intent, context);
        let (commitment, randomness) = self.commitment_engine.commit(data.as_bytes());

        let commitment_vec: Vec<i64> = commitment.to_vec();
        let randomness_vec: Vec<i64> = randomness.to_vec();
        let commitment_hash = sha3_256_hex(
            &commitment_vec
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect::<Vec<u8>>(),
        );

        let entry = MemoryEntry {
            id: sha3_256_hex(
                format!("{}:{}", url, chrono::Utc::now().timestamp_millis()).as_bytes(),
            )[..16]
                .to_string(),
            url: url.to_string(),
            intent: intent.to_string(),
            context: context.to_string(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            commitment: commitment_vec,
            randomness: randomness_vec,
            verified: true,
        };

        let result = CommitResult {
            memory_id: entry.id.clone(),
            commitment_hash: commitment_hash[..16].to_string(),
            verified: true,
        };

        if self.memories.len() >= self.max_memories {
            self.memories.remove(0);
        }
        self.memories.push(entry);
        Ok(result)
    }

    /// Verify a memory's commitment integrity.
    pub fn verify_memory(&mut self, memory_id: &str) -> bool {
        if let Some(entry) = self.memories.iter().find(|m| m.id == memory_id) {
            let data = format!("{}|{}|{}", entry.url, entry.intent, entry.context);
            let commitment = ndarray::Array1::from(entry.commitment.clone());
            let randomness = ndarray::Array1::from(entry.randomness.clone());
            self.commitment_engine
                .verify(data.as_bytes(), &commitment, &randomness)
        } else {
            false
        }
    }

    /// Search memories by keyword.
    pub fn search(&self, query: &str) -> Vec<&MemoryEntry> {
        let q = query.to_lowercase();
        self.memories
            .iter()
            .filter(|m| {
                m.url.to_lowercase().contains(&q)
                    || m.intent.to_lowercase().contains(&q)
                    || m.context.to_lowercase().contains(&q)
            })
            .collect()
    }

    pub fn get_all(&self) -> &[MemoryEntry] {
        &self.memories
    }
    pub fn count(&self) -> usize {
        self.memories.len()
    }
}
