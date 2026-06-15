//! The Proof Economy — a proof economy instead of a task economy.
//!
//! The unit of output here is not a *completed action you must trust*, it is a
//! **machine-checkable claim**. A claim arrives with a `Check` — a deterministic
//! re-runnable proof — and the arbiter of truth is that check executed for real
//! through the [device layer](crate::device_agent::DeviceCapabilities), **never
//! an LLM's opinion**. Reality pushes back on every claim.
//!
//! Around that sits an adversarial economy with skin in the game:
//!   * **Propose** stakes reputation on a claim. If the claim's own check fails
//!     on arrival, the claim is born `Refuted` and the stake is burned.
//!   * **Attack** stakes reputation on *breaking* a claim. The house re-runs the
//!     claim's check (the world may have changed). If it no longer holds, the
//!     claim is `Refuted` and the proposer's stake transfers to the attacker. If
//!     it still holds, the attack failed and the attacker is slashed.
//!   * **Mint.** A claim that survives `MINT_SURVIVED_ATTACKS` independent
//!     attacks while its check keeps passing is **minted** into the permanent
//!     ledger — verified knowledge that compounds and is never re-litigated.
//!     Anyone can re-verify it cheaply by re-running its `Check`.
//!
//! Over time, agents whose confidence does not track reality go bankrupt
//! (reputation ≤ 0 ⇒ they can no longer propose or attack), so calibration
//! becomes survival. Impossibility claims (`CommandFails`) are first-class — a
//! minted "this cannot be done" prunes an entire continent of dead-end work.
//!
//! Honest boundary: an "experiment" here is a checkable computation / command /
//! crawl, not a robotic-lab measurement — the grounding is real execution, not
//! physical apparatus.

use std::path::Path;
use std::sync::Arc;

use parking_lot::Mutex;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use crate::common::{AppError, new_id, now_ms};
use crate::device_agent::DeviceCapabilities;
pub use crate::verification::Check;

const BASE_REPUTATION: f64 = 100.0;
const PROPOSE_STAKE: f64 = 10.0;
const ATTACK_STAKE: f64 = 6.0;
const MINT_REWARD: f64 = 12.0;
const MINT_SURVIVED_ATTACKS: u32 = 2;
/// Verifications that mutate the world cost more to stake — closing off futures
/// (irreversibility) is a cost the economy prices in (option-preserving).
const IRREVERSIBLE_STAKE_MULT: f64 = 2.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimKind {
    /// "X is true."
    #[default]
    Assertion,
    /// "A prior claim / approach is false."
    Refutation,
    /// "X cannot be done" — backed by a check that the operation fails.
    Impossibility,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimStatus {
    Proposed,
    Minted,
    Refuted,
    Disputed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attack {
    pub attacker: String,
    pub succeeded: bool,
    pub note: String,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claim {
    pub claim_id: String,
    pub statement: String,
    pub kind: ClaimKind,
    pub proposer: String,
    pub verification: Check,
    pub reversible: bool,
    pub evidence: Vec<String>,
    /// Reputation the proposer has staked (held until mint, burned on refute).
    pub stake: f64,
    pub status: ClaimStatus,
    pub survived_attacks: u32,
    pub confidence: f64,
    /// Minted claims this one builds on — verified once, never re-litigated.
    pub depends_on: Vec<String>,
    pub attacks: Vec<Attack>,
    pub last_verification: String,
    pub created_at_ms: i64,
    pub minted_at_ms: Option<i64>,
}

/// An agent's standing in the economy — its "skin in the game". Reputation ≤ 0
/// is bankruptcy: it can no longer propose or attack.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub agent_id: String,
    pub reputation: f64,
    pub claims_minted: u32,
    pub claims_refuted: u32,
    pub attacks_won: u32,
    pub attacks_lost: u32,
}

impl Account {
    fn fresh(agent_id: &str) -> Self {
        Self {
            agent_id: agent_id.to_string(),
            reputation: BASE_REPUTATION,
            claims_minted: 0,
            claims_refuted: 0,
            attacks_won: 0,
            attacks_lost: 0,
        }
    }
    #[must_use]
    pub fn bankrupt(&self) -> bool {
        self.reputation <= 0.0
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProposeRequest {
    pub statement: String,
    #[serde(default = "default_kind")]
    pub kind: ClaimKind,
    pub proposer: String,
    pub verification: Check,
    #[serde(default)]
    pub evidence: Vec<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
}

fn default_kind() -> ClaimKind {
    ClaimKind::Assertion
}

#[derive(Debug, Clone, Deserialize)]
pub struct AttackRequest {
    pub attacker: String,
    #[serde(default)]
    pub note: String,
    /// An optional counter-check: if it passes while the claim's check also
    /// passes, the two contradict and the claim is frozen as `Disputed`.
    #[serde(default)]
    pub counter: Option<Check>,
}

#[derive(Debug, Clone, Serialize)]
pub struct VerifyReport {
    pub claim_id: String,
    pub statement: String,
    pub status: ClaimStatus,
    pub pass: bool,
    pub detail: String,
    pub reverifiable: bool,
}

/// The proof economy. Holds the durable ledger and the device layer that runs
/// every check for real.
#[derive(Clone)]
pub struct ProofEconomy {
    store: Arc<Mutex<Connection>>,
    device: Arc<DeviceCapabilities>,
}

impl ProofEconomy {
    pub fn new(data_dir: impl AsRef<Path>, device: Arc<DeviceCapabilities>) -> Result<Self, AppError> {
        let path = data_dir.as_ref().join("proof_economy.sqlite");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                AppError::Internal(format!("failed to create proof-economy dir: {e}"))
            })?;
        }
        let connection = Connection::open(&path)
            .map_err(|e| AppError::Internal(format!("failed to open proof ledger: {e}")))?;
        connection
            .execute_batch(
                "PRAGMA journal_mode=WAL;
                 CREATE TABLE IF NOT EXISTS proof_claims (
                    claim_id TEXT PRIMARY KEY, payload TEXT NOT NULL,
                    status TEXT NOT NULL, created_at_ms INTEGER NOT NULL);
                 CREATE TABLE IF NOT EXISTS proof_accounts (
                    agent_id TEXT PRIMARY KEY, payload TEXT NOT NULL);",
            )
            .map_err(sql_err)?;
        Ok(Self {
            store: Arc::new(Mutex::new(connection)),
            device,
        })
    }

    /// Test/in-memory variant.
    pub fn in_memory(device: Arc<DeviceCapabilities>) -> Result<Self, AppError> {
        let connection = Connection::open_in_memory().map_err(sql_err)?;
        connection
            .execute_batch(
                "CREATE TABLE proof_claims (claim_id TEXT PRIMARY KEY, payload TEXT NOT NULL, status TEXT NOT NULL, created_at_ms INTEGER NOT NULL);
                 CREATE TABLE proof_accounts (agent_id TEXT PRIMARY KEY, payload TEXT NOT NULL);",
            )
            .map_err(sql_err)?;
        Ok(Self {
            store: Arc::new(Mutex::new(connection)),
            device,
        })
    }

    /// Stake reputation on a new claim. Its own check is run immediately: a
    /// claim that does not hold on arrival is born `Refuted` and the stake is
    /// burned — you cannot cheaply assert a falsehood.
    pub fn propose(&self, request: ProposeRequest) -> Result<Claim, AppError> {
        let statement = request.statement.trim().to_string();
        if statement.is_empty() {
            return Err(AppError::Validation("claim statement is empty".into()));
        }
        let reversible = request.verification.is_reversible();
        let stake = PROPOSE_STAKE * if reversible { 1.0 } else { IRREVERSIBLE_STAKE_MULT };

        let mut proposer = self.account_or_fresh(&request.proposer);
        if proposer.bankrupt() {
            return Err(AppError::Forbidden(format!(
                "{} is bankrupt and cannot propose",
                request.proposer
            )));
        }
        if proposer.reputation < stake {
            return Err(AppError::Forbidden(format!(
                "{} has {:.1} reputation, needs {:.1} to stake this claim",
                request.proposer, proposer.reputation, stake
            )));
        }
        proposer.reputation -= stake;

        let (holds, detail) = self.run_check(&request.verification);
        let now = now_ms();
        let mut claim = Claim {
            claim_id: new_id("claim"),
            statement,
            kind: request.kind,
            proposer: request.proposer.clone(),
            verification: request.verification,
            reversible,
            evidence: request.evidence,
            stake,
            status: ClaimStatus::Proposed,
            survived_attacks: 0,
            confidence: 0.6,
            depends_on: request.depends_on,
            attacks: Vec::new(),
            last_verification: detail,
            created_at_ms: now,
            minted_at_ms: None,
        };
        if !holds {
            claim.status = ClaimStatus::Refuted; // stake already burned
            claim.confidence = 0.0;
            proposer.claims_refuted += 1;
        }
        self.save_account(&proposer)?;
        self.save_claim(&claim)?;
        Ok(claim)
    }

    /// Stake reputation on breaking a claim. The house re-runs the claim's check
    /// (reality may have moved). Successful refutation transfers the proposer's
    /// stake to the attacker; a failed attack is slashed. Surviving enough
    /// attacks mints the claim.
    pub fn attack(&self, claim_id: &str, request: AttackRequest) -> Result<Claim, AppError> {
        let mut claim = self.load_claim(claim_id)?;
        if claim.status != ClaimStatus::Proposed {
            return Err(AppError::Validation(format!(
                "claim is {:?}; only Proposed claims can be attacked",
                claim.status
            )));
        }
        let mut attacker = self.account_or_fresh(&request.attacker);
        if attacker.bankrupt() {
            return Err(AppError::Forbidden(format!(
                "{} is bankrupt and cannot attack",
                request.attacker
            )));
        }
        if attacker.reputation < ATTACK_STAKE {
            return Err(AppError::Forbidden(format!(
                "{} has {:.1} reputation, needs {:.1} to attack",
                request.attacker, attacker.reputation, ATTACK_STAKE
            )));
        }
        attacker.reputation -= ATTACK_STAKE;

        let mut proposer = self.account_or_fresh(&claim.proposer);
        let (still_holds, detail) = self.run_check(&claim.verification);

        if !still_holds {
            // The claim no longer holds — refuted. Proposer's stake → attacker.
            claim.status = ClaimStatus::Refuted;
            claim.last_verification = format!("refuted on attack: {detail}");
            claim.confidence = 0.0;
            proposer.claims_refuted += 1;
            attacker.attacks_won += 1;
            attacker.reputation += ATTACK_STAKE + claim.stake;
            claim.attacks.push(Attack {
                attacker: request.attacker,
                succeeded: true,
                note: request.note,
                created_at_ms: now_ms(),
            });
        } else if let Some(counter) = request.counter.as_ref().filter(|_| true) {
            let (counter_holds, cdetail) = self.run_check(counter);
            if counter_holds {
                // Both the claim and a contradicting check pass — freeze it.
                claim.status = ClaimStatus::Disputed;
                claim.last_verification = format!("disputed: counter-check also passes ({cdetail})");
                attacker.reputation += ATTACK_STAKE; // genuine dispute is refunded
                claim.attacks.push(Attack {
                    attacker: request.attacker,
                    succeeded: false,
                    note: format!("contradiction: {}", request.note),
                    created_at_ms: now_ms(),
                });
            } else {
                self.failed_attack(&mut claim, &mut attacker, &mut proposer, &request);
            }
        } else {
            self.failed_attack(&mut claim, &mut attacker, &mut proposer, &request);
        }

        self.save_account(&attacker)?;
        self.save_account(&proposer)?;
        self.save_claim(&claim)?;
        Ok(claim)
    }

    fn failed_attack(
        &self,
        claim: &mut Claim,
        attacker: &mut Account,
        proposer: &mut Account,
        request: &AttackRequest,
    ) {
        // Attacker's stake is burned; the claim withstood scrutiny.
        attacker.attacks_lost += 1;
        claim.survived_attacks += 1;
        claim.confidence = (claim.confidence + 0.15).min(0.99);
        claim.attacks.push(Attack {
            attacker: request.attacker.clone(),
            succeeded: false,
            note: request.note.clone(),
            created_at_ms: now_ms(),
        });
        if claim.survived_attacks >= MINT_SURVIVED_ATTACKS {
            claim.status = ClaimStatus::Minted;
            claim.minted_at_ms = Some(now_ms());
            claim.confidence = (claim.confidence + 0.1).min(1.0);
            claim.last_verification = "minted: survived adversarial review".into();
            proposer.reputation += claim.stake + MINT_REWARD; // stake returned + reward
            proposer.claims_minted += 1;
        }
    }

    /// The cheap public proof: re-run a claim's check. No reputation changes —
    /// anyone can call this to verify a minted result without re-litigating it.
    pub fn verify(&self, claim_id: &str) -> Result<VerifyReport, AppError> {
        let claim = self.load_claim(claim_id)?;
        let (pass, detail) = self.run_check(&claim.verification);
        Ok(VerifyReport {
            claim_id: claim.claim_id,
            statement: claim.statement,
            status: claim.status,
            pass,
            detail,
            reverifiable: true,
        })
    }

    /// The minted ledger — verified knowledge that compounds and never rots.
    pub fn ledger(&self) -> Result<Vec<Claim>, AppError> {
        self.claims_by_status("minted")
    }

    /// Open claims awaiting adversarial review.
    pub fn open_claims(&self) -> Result<Vec<Claim>, AppError> {
        self.claims_by_status("proposed")
    }

    /// Value-of-information experiment selection: the open claim where resolving
    /// uncertainty is worth the most per unit of verification cost.
    pub fn next_experiment(&self) -> Result<Option<Claim>, AppError> {
        let open = self.open_claims()?;
        Ok(open.into_iter().max_by(|a, b| {
            voi(a).partial_cmp(&voi(b)).unwrap_or(std::cmp::Ordering::Equal)
        }))
    }

    pub fn get_claim(&self, claim_id: &str) -> Result<Claim, AppError> {
        self.load_claim(claim_id)
    }

    pub fn account(&self, agent_id: &str) -> Result<Account, AppError> {
        Ok(self.account_or_fresh(agent_id))
    }

    // ── check execution (the arbiter of truth) ──────────────────────────

    fn run_check(&self, check: &Check) -> (bool, String) {
        let outcome = crate::verification::run(check, &self.device, None);
        (outcome.pass, outcome.detail)
    }

    /// The newest minted claim whose statement matches `statement` exactly —
    /// the agentic loop's memory lookup so it never re-litigates solved work.
    pub fn find_minted(&self, statement: &str) -> Result<Option<Claim>, AppError> {
        let want = statement.trim();
        Ok(self
            .ledger()?
            .into_iter()
            .find(|claim| claim.statement.trim() == want))
    }

    /// The minted claim most semantically similar to `statement`, if its
    /// similarity meets `threshold`. Lexical cosine, not deep embeddings — an
    /// exact match scores 1.0; the threshold should be conservative so a loose
    /// match cannot trigger a false recall. Returns `(claim, score)`.
    pub fn find_minted_semantic(
        &self,
        statement: &str,
        threshold: f64,
    ) -> Result<Option<(Claim, f64)>, AppError> {
        let want = statement.trim();
        let mut best: Option<(Claim, f64)> = None;
        for claim in self.ledger()? {
            let score = if claim.statement.trim() == want {
                1.0
            } else {
                crate::text_match::similarity(want, &claim.statement)
            };
            if score >= threshold && best.as_ref().map_or(true, |(_, s)| score > *s) {
                best = Some((claim, score));
            }
        }
        Ok(best)
    }

    // ── persistence ─────────────────────────────────────────────────────

    fn save_claim(&self, claim: &Claim) -> Result<(), AppError> {
        let payload = serde_json::to_string(claim).map_err(ser_err)?;
        let status = serde_json::to_value(claim.status)
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_else(|| "proposed".into());
        self.store
            .lock()
            .execute(
                "INSERT OR REPLACE INTO proof_claims (claim_id, payload, status, created_at_ms) VALUES (?1,?2,?3,?4)",
                params![claim.claim_id, payload, status, claim.created_at_ms],
            )
            .map_err(sql_err)?;
        Ok(())
    }

    fn load_claim(&self, claim_id: &str) -> Result<Claim, AppError> {
        let payload: String = self
            .store
            .lock()
            .query_row(
                "SELECT payload FROM proof_claims WHERE claim_id=?1",
                [claim_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_err)?
            .ok_or_else(|| AppError::NotFound(format!("claim {claim_id}")))?;
        serde_json::from_str(&payload).map_err(de_err)
    }

    fn claims_by_status(&self, status: &str) -> Result<Vec<Claim>, AppError> {
        let store = self.store.lock();
        let mut stmt = store
            .prepare("SELECT payload FROM proof_claims WHERE status=?1 ORDER BY created_at_ms DESC LIMIT 500")
            .map_err(sql_err)?;
        let rows = stmt
            .query_map([status], |row| row.get::<_, String>(0))
            .map_err(sql_err)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(serde_json::from_str(&row.map_err(sql_err)?).map_err(de_err)?);
        }
        Ok(out)
    }

    fn account_or_fresh(&self, agent_id: &str) -> Account {
        let payload: Option<String> = self
            .store
            .lock()
            .query_row(
                "SELECT payload FROM proof_accounts WHERE agent_id=?1",
                [agent_id],
                |row| row.get(0),
            )
            .optional()
            .ok()
            .flatten();
        payload
            .and_then(|p| serde_json::from_str(&p).ok())
            .unwrap_or_else(|| Account::fresh(agent_id))
    }

    fn save_account(&self, account: &Account) -> Result<(), AppError> {
        let payload = serde_json::to_string(account).map_err(ser_err)?;
        self.store
            .lock()
            .execute(
                "INSERT OR REPLACE INTO proof_accounts (agent_id, payload) VALUES (?1,?2)",
                params![account.agent_id, payload],
            )
            .map_err(sql_err)?;
        Ok(())
    }
}

fn voi(claim: &Claim) -> f64 {
    // worth resolving = stake at risk × remaining uncertainty, per unit cost.
    claim.stake * (1.0 - claim.confidence) / claim.verification.cost().max(0.1)
}

fn sql_err(e: rusqlite::Error) -> AppError {
    AppError::Internal(format!("proof-economy sql error: {e}"))
}
fn ser_err(e: serde_json::Error) -> AppError {
    AppError::Internal(format!("proof-economy serialize error: {e}"))
}
fn de_err(e: serde_json::Error) -> AppError {
    AppError::Internal(format!("proof-economy decode error: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device_agent::DevicePolicy;

    fn economy() -> ProofEconomy {
        let device = Arc::new(DeviceCapabilities::new(DevicePolicy::permissive()));
        ProofEconomy::in_memory(device).expect("economy")
    }

    #[test]
    fn true_claim_survives_attacks_and_mints() {
        let econ = economy();
        let claim = econ
            .propose(ProposeRequest {
                statement: "echo prints the answer 42".into(),
                kind: ClaimKind::Assertion,
                proposer: "alice".into(),
                verification: Check::ShellOutputContains {
                    command: "echo the answer is 42".into(),
                    substring: "42".into(),
                },
                evidence: vec![],
                depends_on: vec![],
            })
            .expect("propose");
        assert_eq!(claim.status, ClaimStatus::Proposed);

        // Two failed attacks (the claim is actually true) → mint.
        econ.attack(&claim.claim_id, AttackRequest { attacker: "bob".into(), note: "nope".into(), counter: None }).unwrap();
        let minted = econ
            .attack(&claim.claim_id, AttackRequest { attacker: "carol".into(), note: "nope".into(), counter: None })
            .unwrap();
        assert_eq!(minted.status, ClaimStatus::Minted);
        assert_eq!(minted.survived_attacks, 2);

        // Proposer profited; attackers were slashed.
        assert!(econ.account("alice").unwrap().reputation > BASE_REPUTATION);
        assert!(econ.account("bob").unwrap().reputation < BASE_REPUTATION);
        assert_eq!(econ.account("alice").unwrap().claims_minted, 1);

        // It's in the permanent ledger and re-verifiable cheaply.
        assert_eq!(econ.ledger().unwrap().len(), 1);
        assert!(econ.verify(&claim.claim_id).unwrap().pass);
    }

    #[test]
    fn false_claim_is_refuted_on_arrival_and_burns_stake() {
        let econ = economy();
        let claim = econ
            .propose(ProposeRequest {
                statement: "echo 42 prints 99".into(),
                kind: ClaimKind::Assertion,
                proposer: "liar".into(),
                verification: Check::ShellOutputContains {
                    command: "echo 42".into(),
                    substring: "99".into(),
                },
                evidence: vec![],
                depends_on: vec![],
            })
            .unwrap();
        assert_eq!(claim.status, ClaimStatus::Refuted);
        // Stake was burned.
        assert!(econ.account("liar").unwrap().reputation < BASE_REPUTATION);
        assert_eq!(econ.account("liar").unwrap().claims_refuted, 1);
    }

    #[test]
    fn claim_refuted_when_reality_changes() {
        let econ = economy();
        let dir = std::env::temp_dir().join(format!("astra-proof-{}", now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("fact.txt");
        std::fs::write(&file, "the sky is blue").unwrap();
        let path = file.to_string_lossy().to_string();

        let claim = econ
            .propose(ProposeRequest {
                statement: "fact.txt says the sky is blue".into(),
                kind: ClaimKind::Assertion,
                proposer: "alice".into(),
                verification: Check::FileContains { path: path.clone(), substring: "blue".into() },
                evidence: vec![],
                depends_on: vec![],
            })
            .unwrap();
        assert_eq!(claim.status, ClaimStatus::Proposed); // held on arrival

        // Reality moves: the file changes. Now an attack falsifies the claim.
        std::fs::write(&file, "the sky is green").unwrap();
        let refuted = econ
            .attack(&claim.claim_id, AttackRequest { attacker: "bob".into(), note: "file changed".into(), counter: None })
            .unwrap();
        assert_eq!(refuted.status, ClaimStatus::Refuted);
        // Proposer's stake transferred to the successful attacker.
        assert!(econ.account("bob").unwrap().reputation > BASE_REPUTATION);
        assert_eq!(econ.account("bob").unwrap().attacks_won, 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn impossibility_claim_mints_when_operation_fails() {
        let econ = economy();
        // "This command cannot succeed" — backed by CommandFails.
        let claim = econ
            .propose(ProposeRequest {
                statement: "the command `astra_nonexistent_binary_zzz` cannot run".into(),
                kind: ClaimKind::Impossibility,
                proposer: "prover".into(),
                verification: Check::CommandFails {
                    command: "astra_nonexistent_binary_zzz --do-the-impossible".into(),
                },
                evidence: vec![],
                depends_on: vec![],
            })
            .unwrap();
        assert_eq!(claim.status, ClaimStatus::Proposed);
        econ.attack(&claim.claim_id, AttackRequest { attacker: "skeptic1".into(), note: "".into(), counter: None }).unwrap();
        let minted = econ
            .attack(&claim.claim_id, AttackRequest { attacker: "skeptic2".into(), note: "".into(), counter: None })
            .unwrap();
        assert_eq!(minted.kind, ClaimKind::Impossibility);
        assert_eq!(minted.status, ClaimStatus::Minted);
    }

    #[test]
    fn bankruptcy_blocks_proposing() {
        let econ = economy();
        // Burn reputation with repeated false claims until bankrupt.
        for _ in 0..12 {
            let _ = econ.propose(ProposeRequest {
                statement: "false".into(),
                kind: ClaimKind::Assertion,
                proposer: "reckless".into(),
                verification: Check::Trivial { pass: false },
                evidence: vec![],
                depends_on: vec![],
            });
        }
        let acct = econ.account("reckless").unwrap();
        assert!(acct.bankrupt(), "reputation should be exhausted, got {}", acct.reputation);
        let blocked = econ.propose(ProposeRequest {
            statement: "one more".into(),
            kind: ClaimKind::Assertion,
            proposer: "reckless".into(),
            verification: Check::Trivial { pass: true },
            evidence: vec![],
            depends_on: vec![],
        });
        assert!(blocked.is_err(), "bankrupt agent must be blocked from proposing");
    }
}
