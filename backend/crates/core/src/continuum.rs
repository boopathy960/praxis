//! The Continuum — six real-world verification domains over one proof substrate.
//!
//! Each of the six "impossible" products is, underneath, the **same machine**
//! pointed at a different domain:
//!
//!   1. **Reproducibility** — a scientific finding becomes a claim bundled with a
//!      re-runnable `Check` ("this dataset + this script yields the reported
//!      effect"). The [Sentinel](crate::sentinel) keeps re-executing it and the
//!      drift ledger records the exact moment reproducibility breaks.
//!   2. **Supply chain** — behavioral guarantees *and* impossibilities about a
//!      dependency ("imported in this sandbox it makes no outbound call"). The
//!      impossibility holds until a new version adds the path — then it drifts and
//!      you are alerted the instant a backdoor's *behavior* appears.
//!   3. **Compliance** — each control is a continuously-attacked `Check` against
//!      software-accessible state ("no S3 bucket is publicly readable"). Not "we
//!      attest", but "this held under an adversary, and here is the second it
//!      drifted".
//!   4. **Spec mining** — run the proposer/refuter round *backwards* at an opaque
//!      system to discover its true, executable spec; survivors are minted and
//!      watched.
//!   5. **Impossibility registry** — the negative-knowledge layer: a queryable
//!      ledger of what provably *cannot* be done here, so nobody re-explores a
//!      proven dead end.
//!   6. **Guardrails** — a vibe-coder's plain-language intent ("never expose user
//!      emails") held as a continuously-attacked impossibility, and recalled in
//!      plain language by lexical similarity.
//!
//! Every domain reduces to: compile an artifact into one or more
//! [`ProposeRequest`](crate::proof_economy::ProposeRequest)s, stake them in the
//! [proof economy](crate::proof_economy) (the check runs for real on arrival),
//! and place each surviving claim under continuous re-verification in the
//! Sentinel. The economy supplies adversarial minting; the Sentinel supplies the
//! continuous "when did it break?" that a one-time badge cannot.
//!
//! Honest boundary, inherited from the substrate: only properties that reduce to
//! a runnable `Check` through the [device layer](crate::device_agent) qualify —
//! no wet-lab, no instruments, no JS-heavy UIs. Impossibility is always "under
//! this sandbox/config", never metaphysical. Plain-language recall is lexical
//! ([`text_match`](crate::text_match)), so it can miss paraphrases that share no
//! vocabulary.

use serde::{Deserialize, Serialize};

use crate::common::AppError;
use crate::proof_economy::{Check, Claim, ClaimKind, ClaimStatus, ProofEconomy, ProposeRequest};
use crate::proof_round::{ProofConductor, RoundConfig, RoundReport};
use crate::sentinel::{Domain, DriftEvent, RegisterWatch, Sentinel, SweepReport, Watch};

/// One claim after enrollment: the staked claim, the watch now re-verifying it,
/// and whether its check failed on arrival (the property is *already* violated).
#[derive(Debug, Clone, Serialize)]
pub struct EnrolledClaim {
    pub claim: Claim,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub watch_id: Option<String>,
    /// The claim's check failed the instant it was staked — e.g. the prohibition
    /// is already breached, the control already out of bounds, the finding does
    /// not reproduce even once. A born-violated claim is *not* watched (there is
    /// no held baseline to drift from); the violation is the headline.
    pub born_violated: bool,
}

/// The result of enrolling a domain artifact — one or more staked, watched claims.
#[derive(Debug, Clone, Serialize)]
pub struct EnrollReport {
    pub domain: Domain,
    pub summary: String,
    pub claims: Vec<EnrolledClaim>,
}

/// The Continuum service — compiles domain artifacts into staked, continuously
/// re-verified claims. Shares the one economy and one Sentinel so every domain's
/// proofs and drift land in the same ledgers.
#[derive(Clone)]
pub struct ContinuumService {
    economy: ProofEconomy,
    sentinel: Sentinel,
    /// Spec mining needs the LLM proposer/refuter populations; attached when a
    /// remote reasoner is configured.
    conductor: Option<ProofConductor>,
}

impl ContinuumService {
    #[must_use]
    pub fn new(economy: ProofEconomy, sentinel: Sentinel) -> Self {
        Self {
            economy,
            sentinel,
            conductor: None,
        }
    }

    /// Attach the proof conductor so spec mining can run.
    #[must_use]
    pub fn with_conductor(mut self, conductor: ProofConductor) -> Self {
        self.conductor = Some(conductor);
        self
    }

    /// The underlying Sentinel — handed to the background sweeper at startup.
    #[must_use]
    pub fn sentinel(&self) -> Sentinel {
        self.sentinel.clone()
    }

    // ── 1. Reproducibility ──────────────────────────────────────────────

    /// Turn a computational finding into a living, re-runnable certificate of
    /// reproducibility: "re-running this yields the reported effect". An optional
    /// dataset-presence guard rots the moment the data version is lost.
    pub fn enroll_reproducibility(&self, req: ReproRequest) -> Result<EnrollReport, AppError> {
        let finding = require(&req.finding, "finding")?;
        let command = require(&req.command, "command")?;
        let expect = require(&req.expect_output, "expect_output")?;
        let proposer = req.proposer.unwrap_or_else(|| "repro".into());
        let run_command = match req
            .work_dir
            .as_deref()
            .map(str::trim)
            .filter(|d| !d.is_empty())
        {
            Some(dir) => format!("cd \"{dir}\" && {command}"),
            None => command,
        };

        let mut claims = Vec::new();
        if let Some(dataset) = req
            .dataset_path
            .as_deref()
            .map(str::trim)
            .filter(|d| !d.is_empty())
        {
            claims.push(self.enroll_one(
                format!("{proposer}_data"),
                format!("dataset present for: {finding}"),
                ClaimKind::Assertion,
                Check::FileExists {
                    path: dataset.to_string(),
                },
                Domain::Reproducibility,
                format!("dataset: {}", truncate(&finding, 48)),
                req.evidence.clone(),
            )?);
        }
        claims.push(self.enroll_one(
            proposer,
            format!("reproducible: {finding}"),
            ClaimKind::Assertion,
            Check::ShellOutputContains {
                command: run_command,
                substring: expect,
            },
            Domain::Reproducibility,
            format!("repro: {}", truncate(&finding, 54)),
            req.evidence,
        )?);
        Ok(EnrollReport {
            domain: Domain::Reproducibility,
            summary: format!("enrolled reproducibility certificate for: {finding}"),
            claims,
        })
    }

    // ── 2. Supply-chain behavioral contracts ────────────────────────────

    /// Maintain an adversarial ledger of what a dependency provably can and
    /// cannot do. Positive guarantees re-verify on every version bump; negative
    /// **impossibility** claims fire the instant a quiet exfiltration path appears.
    pub fn enroll_dependency_contract(
        &self,
        req: DependencyContractRequest,
    ) -> Result<EnrollReport, AppError> {
        let package = require(&req.package, "package")?;
        if req.guarantees.is_empty() && req.prohibitions.is_empty() {
            return Err(AppError::Validation(
                "a contract needs at least one guarantee or prohibition".into(),
            ));
        }
        let proposer = req
            .proposer
            .unwrap_or_else(|| format!("supply_{}", sanitize(&package)));
        let mut claims = Vec::new();

        for guarantee in req.guarantees {
            let desc = require(&guarantee.description, "guarantee.description")?;
            let command = require(&guarantee.command, "guarantee.command")?;
            let check = match guarantee
                .expect
                .as_deref()
                .map(str::trim)
                .filter(|e| !e.is_empty())
            {
                Some(sub) => Check::ShellOutputContains {
                    command,
                    substring: sub.to_string(),
                },
                None => Check::CommandSucceeds { command },
            };
            claims.push(self.enroll_one(
                proposer.clone(),
                format!("{package} guarantees: {desc}"),
                ClaimKind::Assertion,
                check,
                Domain::SupplyChain,
                format!("{package}: {}", truncate(&desc, 44)),
                vec![format!("package:{package}")],
            )?);
        }

        for prohibition in req.prohibitions {
            let desc = require(&prohibition.description, "prohibition.description")?;
            let attempt = require(&prohibition.attempt_command, "prohibition.attempt_command")?;
            // The impossibility: the forbidden behavior cannot be provoked. Holds
            // while the sandbox harness fails to make it happen; a new version
            // that adds the path makes the harness succeed → this stops holding.
            claims.push(self.enroll_one(
                proposer.clone(),
                format!("{package} cannot: {desc}"),
                ClaimKind::Impossibility,
                Check::CommandFails { command: attempt },
                Domain::SupplyChain,
                format!("{package} \u{2298} {}", truncate(&desc, 42)),
                vec![format!("package:{package}")],
            )?);
        }
        Ok(EnrollReport {
            domain: Domain::SupplyChain,
            summary: format!("enrolled behavioral contract for {package}"),
            claims,
        })
    }

    // ── 3. Continuous adversarial compliance ────────────────────────────

    /// Express a control as a `Check` against software-accessible state and place
    /// it under continuous attack. A "must fail" control (the violating op cannot
    /// succeed) is a first-class impossibility; the drift ledger holds the second
    /// it stopped holding.
    pub fn enroll_control(&self, req: ControlRequest) -> Result<EnrollReport, AppError> {
        let framework = require(&req.framework, "framework")?;
        let control_id = require(&req.control_id, "control_id")?;
        let description = require(&req.description, "description")?;
        let proposer = req
            .proposer
            .unwrap_or_else(|| format!("compliance_{}", sanitize(&framework)));
        let (kind, check) = match req.assertion {
            ControlAssertion::MustFail { command } => (
                ClaimKind::Impossibility,
                Check::CommandFails {
                    command: require(&command, "command")?,
                },
            ),
            ControlAssertion::MustSucceed { command } => (
                ClaimKind::Assertion,
                Check::CommandSucceeds {
                    command: require(&command, "command")?,
                },
            ),
            ControlAssertion::OutputMustContain { command, substring } => (
                ClaimKind::Assertion,
                Check::ShellOutputContains {
                    command: require(&command, "command")?,
                    substring: require(&substring, "substring")?,
                },
            ),
        };
        let claim = self.enroll_one(
            proposer,
            format!("[{framework} {control_id}] {description}"),
            kind,
            check,
            Domain::Compliance,
            format!("{framework}/{control_id}"),
            vec![
                format!("framework:{framework}"),
                format!("control:{control_id}"),
            ],
        )?;
        Ok(EnrollReport {
            domain: Domain::Compliance,
            summary: format!("enrolled continuously-attacked control {framework}/{control_id}"),
            claims: vec![claim],
        })
    }

    // ── 4. Verified-spec mining (the round, run backwards) ──────────────

    /// Point the proposer/refuter populations at an opaque system. The proposer
    /// generates behavioral claims (each a runnable check); the refuters attack;
    /// what survives is an adversarially-hardened, executable spec of a system
    /// that had no docs — discovered, not written. Survivors are watched so the
    /// mined spec keeps re-verifying.
    pub async fn mine_spec(&self, req: MineRequest) -> Result<MineReport, AppError> {
        let system = require(&req.system, "system")?;
        let conductor = self.conductor.clone().ok_or_else(|| {
            AppError::Validation(
                "spec mining needs a configured reasoner (no proof conductor attached)".into(),
            )
        })?;
        let topic = format!(
            "the true, observable behavior of this opaque system: {system}. Propose precise \
             behavioral claims, each backed by a check you can actually run on THIS host (a shell \
             command, an HTTP probe via curl, or a file assertion) and that you believe currently \
             passes. Prefer claims that pin down one concrete, surprising behavior each."
        );
        let round = conductor
            .run_round(RoundConfig {
                topic,
                num_proposals: req.probes.unwrap_or(4),
                attack_budget: req.attack_budget.unwrap_or(8),
            })
            .await?;

        let mut spec = Vec::new();
        let mut watch_ids = Vec::new();
        for outcome in round
            .outcomes
            .iter()
            .filter(|o| matches!(o.status, ClaimStatus::Minted))
        {
            if let Ok(claim) = self.economy.get_claim(&outcome.claim_id) {
                let watch = self.sentinel.register(RegisterWatch {
                    claim_id: Some(claim.claim_id.clone()),
                    domain: Domain::SpecMining,
                    label: format!("spec: {}", truncate(&claim.statement, 54)),
                    check: claim.verification.clone(),
                })?;
                watch_ids.push(watch.watch_id);
                spec.push(SpecClause {
                    claim_id: claim.claim_id,
                    statement: claim.statement,
                    check: claim.verification,
                });
            }
        }
        Ok(MineReport {
            system,
            round,
            spec,
            watch_ids,
        })
    }

    // ── 5. The registry of proven impossibilities ───────────────────────

    /// Bank a negative result: "this provably cannot be done here". Backed by a
    /// `CommandFails` check that anyone can re-run; survives only if no one can
    /// make the operation succeed.
    pub fn register_impossibility(
        &self,
        req: ImpossibilityRequest,
    ) -> Result<EnrolledClaim, AppError> {
        let statement = require(&req.statement, "statement")?;
        let attempt = require(&req.attempt_command, "attempt_command")?;
        let proposer = req.proposer.unwrap_or_else(|| "impossibility".into());
        self.enroll_one(
            proposer,
            statement.clone(),
            ClaimKind::Impossibility,
            Check::CommandFails { command: attempt },
            Domain::Impossibility,
            truncate(&statement, 58),
            req.evidence,
        )
    }

    /// The negative-knowledge ledger — minted (banked) and pending impossibilities.
    pub fn impossibility_registry(&self) -> Result<ImpossibilityRegistry, AppError> {
        let minted = self
            .economy
            .ledger()?
            .into_iter()
            .filter(|c| c.kind == ClaimKind::Impossibility)
            .collect();
        let pending = self
            .economy
            .open_claims()?
            .into_iter()
            .filter(|c| c.kind == ClaimKind::Impossibility)
            .collect();
        Ok(ImpossibilityRegistry { minted, pending })
    }

    /// "Has anyone proven this won't work here?" — lexical recall over the minted
    /// impossibility ledger so a team never re-explores a banked dead end.
    pub fn is_known_dead_end(&self, query: DeadEndQuery) -> Result<DeadEndAnswer, AppError> {
        let statement = require(&query.statement, "statement")?;
        let threshold = query.threshold.unwrap_or(0.6).clamp(0.0, 1.0);
        let mut best: Option<(Claim, f64)> = None;
        for claim in self
            .economy
            .ledger()?
            .into_iter()
            .filter(|c| c.kind == ClaimKind::Impossibility)
        {
            let score = if claim.statement.trim() == statement {
                1.0
            } else {
                crate::text_match::similarity(&statement, &claim.statement)
            };
            if score >= threshold && best.as_ref().map_or(true, |(_, s)| score > *s) {
                best = Some((claim, score));
            }
        }
        Ok(match best {
            Some((claim, score)) => DeadEndAnswer {
                known_dead_end: true,
                score,
                note: format!(
                    "proven dead end (similarity {score:.2}) — provably won't work here: {}",
                    claim.statement
                ),
                claim: Some(claim),
            },
            None => DeadEndAnswer {
                known_dead_end: false,
                score: 0.0,
                claim: None,
                note: "no minted impossibility matches — not a known dead end here".into(),
            },
        })
    }

    // ── 6. Vibe-coder guardrails ────────────────────────────────────────

    /// Hold a plain-language safety intent as a continuously-attacked
    /// impossibility: the violation cannot be provoked. The refuter population
    /// hunts for the request that breaks it; the Sentinel catches the instant a
    /// future change makes the violation reproducible — and says so in plain
    /// language, with no code-reading required.
    pub fn enroll_guardrail(&self, req: GuardrailRequest) -> Result<EnrolledClaim, AppError> {
        let intent = require(&req.intent, "intent")?;
        let violation = require(&req.violation_command, "violation_command")?;
        let proposer = req.proposer.unwrap_or_else(|| "guardrail".into());
        self.enroll_one(
            proposer,
            format!("guardrail: {intent}"),
            ClaimKind::Impossibility,
            Check::CommandFails { command: violation },
            Domain::Guardrail,
            truncate(&intent, 58),
            vec![format!("intent:{intent}")],
        )
    }

    /// "Is my app safe re: …?" — recall the relevant guardrails in plain language
    /// and report whether each currently holds. Lexical match, ranked by score.
    pub fn query_guardrails(&self, query: GuardrailQuery) -> Result<Vec<GuardrailMatch>, AppError> {
        let concern = require(&query.concern, "concern")?;
        let mut matches: Vec<GuardrailMatch> = self
            .sentinel
            .watches(Some(Domain::Guardrail))?
            .into_iter()
            .map(|watch| {
                let score = crate::text_match::similarity(&concern, &watch.label);
                GuardrailMatch {
                    watch_id: watch.watch_id,
                    intent: watch.label,
                    holding: watch.holding,
                    score,
                    last_detail: watch.last_detail,
                }
            })
            .filter(|m| m.score > 0.0)
            .collect();
        matches.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(matches)
    }

    // ── Sentinel passthroughs (the continuous layer, surfaced) ──────────

    /// Re-verify every active watch now and record any drift.
    pub fn sweep(&self) -> Result<SweepReport, AppError> {
        self.sentinel.sweep()
    }

    pub fn watches(&self, domain: Option<Domain>) -> Result<Vec<Watch>, AppError> {
        self.sentinel.watches(domain)
    }

    pub fn get_watch(&self, watch_id: &str) -> Result<Watch, AppError> {
        self.sentinel.get_watch(watch_id)
    }

    pub fn sweep_one(&self, watch_id: &str) -> Result<Watch, AppError> {
        self.sentinel.sweep_one(watch_id)
    }

    pub fn drift_log(
        &self,
        watch_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<DriftEvent>, AppError> {
        self.sentinel.drift_log(watch_id, limit)
    }

    // ── 7. The AI-in-production safety stack (red-team + eval + guardrails) ─

    /// Stake a set of AI safety guardrails as continuously-attacked
    /// impossibilities: each red-team probe **cannot** make the assistant violate
    /// the policy (leak a PII field, obey a jailbreak class). The probe command
    /// exits 0 iff the model violated; while it keeps failing the guardrail holds,
    /// and the instant a model or prompt change reopens the hole the Sentinel
    /// refutes it and stamps the moment. [`run_adversary`](Self::run_adversary) is
    /// the continuous, self-calibrating red team.
    pub fn enroll_ai_guardrails(&self, req: AiGuardrailRequest) -> Result<EnrollReport, AppError> {
        let policy = require(&req.policy, "policy")?;
        if req.probes.is_empty() {
            return Err(AppError::Validation(
                "an AI guardrail needs at least one red-team probe".into(),
            ));
        }
        let proposer = req
            .proposer
            .unwrap_or_else(|| format!("aisafety_{}", sanitize(&policy)));
        let mut claims = Vec::new();
        for probe in req.probes {
            let class = require(&probe.jailbreak_class, "probe.jailbreak_class")?;
            let command = require(&probe.probe_command, "probe.probe_command")?;
            claims.push(self.enroll_one(
                proposer.clone(),
                format!("[AI guardrail: {policy}] resists: {class}"),
                ClaimKind::Impossibility,
                Check::CommandFails { command },
                Domain::AiSafety,
                format!("{policy} \u{2298} {}", truncate(&class, 40)),
                vec![format!("policy:{policy}")],
            )?);
        }
        Ok(EnrollReport {
            domain: Domain::AiSafety,
            summary: format!(
                "enrolled {} red-team guardrail(s) for policy {policy}",
                claims.len()
            ),
            claims,
        })
    }

    /// Stake an evaluation suite: each case asserts the assistant's output on a
    /// fixed input contains the expected token. Re-run on every model/prompt
    /// change, so an eval that silently regresses refutes itself.
    pub fn enroll_eval_suite(&self, req: EvalSuiteRequest) -> Result<EnrollReport, AppError> {
        let suite = require(&req.suite, "suite")?;
        if req.cases.is_empty() {
            return Err(AppError::Validation(
                "an eval suite needs at least one case".into(),
            ));
        }
        let proposer = req
            .proposer
            .unwrap_or_else(|| format!("eval_{}", sanitize(&suite)));
        let mut claims = Vec::new();
        for case in req.cases {
            let desc = require(&case.description, "case.description")?;
            let command = require(&case.command, "case.command")?;
            let expect = require(&case.expect, "case.expect")?;
            claims.push(self.enroll_one(
                proposer.clone(),
                format!("[eval {suite}] {desc}"),
                ClaimKind::Assertion,
                Check::ShellOutputContains {
                    command,
                    substring: expect,
                },
                Domain::AiSafety,
                format!("{suite}: {}", truncate(&desc, 44)),
                vec![format!("suite:{suite}")],
            )?);
        }
        Ok(EnrollReport {
            domain: Domain::AiSafety,
            summary: format!("enrolled eval suite {suite} ({} cases)", claims.len()),
            claims,
        })
    }

    // ── 8. Verified data-quality & data contracts ───────────────────────

    /// Stake a data contract as a set of checks over your warehouse (via its
    /// shell/SQL CLI). Range/null/integrity/reconciliation expectations re-verify
    /// continuously and auto-refute on drift; a "must be impossible" expectation
    /// (e.g. a duplicate primary key) is banked as a first-class impossibility.
    pub fn enroll_data_contract(&self, req: DataContractRequest) -> Result<EnrollReport, AppError> {
        let dataset = require(&req.dataset, "dataset")?;
        if req.expectations.is_empty() {
            return Err(AppError::Validation(
                "a data contract needs at least one expectation".into(),
            ));
        }
        let proposer = req
            .proposer
            .unwrap_or_else(|| format!("data_{}", sanitize(&dataset)));
        let mut claims = Vec::new();
        for expectation in req.expectations {
            let (desc, kind, check) = match expectation {
                DataExpectation::RowsMatch {
                    description,
                    command,
                    substring,
                } => (
                    require(&description, "expectation.description")?,
                    ClaimKind::Assertion,
                    Check::ShellOutputContains {
                        command: require(&command, "command")?,
                        substring: require(&substring, "substring")?,
                    },
                ),
                DataExpectation::Holds {
                    description,
                    command,
                } => (
                    require(&description, "expectation.description")?,
                    ClaimKind::Assertion,
                    Check::CommandSucceeds {
                        command: require(&command, "command")?,
                    },
                ),
                DataExpectation::Impossible {
                    description,
                    command,
                } => (
                    require(&description, "expectation.description")?,
                    ClaimKind::Impossibility,
                    Check::CommandFails {
                        command: require(&command, "command")?,
                    },
                ),
            };
            claims.push(self.enroll_one(
                proposer.clone(),
                format!("[data {dataset}] {desc}"),
                kind,
                check,
                Domain::DataContract,
                format!("{dataset}: {}", truncate(&desc, 44)),
                vec![format!("dataset:{dataset}")],
            )?);
        }
        Ok(EnrollReport {
            domain: Domain::DataContract,
            summary: format!(
                "enrolled data contract for {dataset} ({} expectations)",
                claims.len()
            ),
            claims,
        })
    }

    // ── 9. Verified migration / modernization (behavioral equivalence) ──

    /// Phase 2 of a verified migration: stake a **behavioral-equivalence** claim
    /// — the new path produces the same result as the old. The equivalence command
    /// exits 0 iff old and new agree (the common idiom diffs their outputs). When
    /// `spec_claim_id` is given, the claim depends on that minted spec clause
    /// (Phase 1, via [`mine_spec`](Self::mine_spec)). Every rewrite must keep this
    /// passing under attack — de-risked, verified equivalence by construction.
    pub fn enroll_equivalence(&self, req: EquivalenceRequest) -> Result<EnrolledClaim, AppError> {
        let aspect = require(&req.aspect, "aspect")?;
        let command = require(&req.equivalence_command, "equivalence_command")?;
        let proposer = req.proposer.unwrap_or_else(|| "migration".into());
        let depends_on: Vec<String> = req
            .spec_claim_id
            .into_iter()
            .filter(|s| !s.trim().is_empty())
            .collect();
        self.enroll_with_deps(
            proposer,
            format!("behavioral equivalence: {aspect}"),
            ClaimKind::Assertion,
            Check::CommandSucceeds { command },
            Domain::Equivalence,
            format!("equiv: {}", truncate(&aspect, 50)),
            vec![format!("migration:{aspect}")],
            depends_on,
        )
    }

    // ── 10. A knowledge base that cannot lie ────────────────────────────

    /// Back an operational fact with a re-runnable check, so it physically cannot
    /// go stale-and-silent: the moment reality drifts (the deploy command changes,
    /// a config value moves, a dependency disappears) the fact refutes itself and
    /// the drift ledger raises the flag. The soft "what was I doing" memory lives
    /// in the [chronicle](crate::chronicle); these are the verified facts that
    /// can't lie.
    pub fn record_fact(&self, req: KnowledgeFactRequest) -> Result<EnrolledClaim, AppError> {
        let fact = require(&req.fact, "fact")?;
        let proposer = req.proposer.unwrap_or_else(|| "knowledge".into());
        let check = match req.check {
            KnowledgeCheck::FileContains { path, substring } => Check::FileContains {
                path: require(&path, "path")?,
                substring: require(&substring, "substring")?,
            },
            KnowledgeCheck::CommandOutputContains { command, substring } => {
                Check::ShellOutputContains {
                    command: require(&command, "command")?,
                    substring: require(&substring, "substring")?,
                }
            }
            KnowledgeCheck::CommandSucceeds { command } => Check::CommandSucceeds {
                command: require(&command, "command")?,
            },
        };
        self.enroll_one(
            proposer,
            fact.clone(),
            ClaimKind::Assertion,
            check,
            Domain::Knowledge,
            truncate(&fact, 58),
            req.evidence,
        )
    }

    /// The knowledge base, partitioned by reality: facts that currently hold vs
    /// facts that have gone false (a wrong fact that refuted itself and raised a
    /// flag). The demo: change the deploy script, sweep, watch it move to `broken`.
    pub fn knowledge_base(&self) -> Result<KnowledgeBase, AppError> {
        let mut verified = Vec::new();
        let mut broken = Vec::new();
        for watch in self.sentinel.watches(Some(Domain::Knowledge))? {
            let entry = KnowledgeEntry {
                watch_id: watch.watch_id,
                fact: watch.label,
                holding: watch.holding,
                last_detail: watch.last_detail,
                claim_id: watch.claim_id,
            };
            if entry.holding {
                verified.push(entry);
            } else {
                broken.push(entry);
            }
        }
        Ok(KnowledgeBase { verified, broken })
    }

    /// Recall facts by plain-language query, with their current truth state.
    /// Lexical match (see the module's honesty note).
    pub fn recall_fact(&self, query: KnowledgeQuery) -> Result<Vec<KnowledgeEntry>, AppError> {
        let concern = require(&query.concern, "concern")?;
        let mut ranked: Vec<(KnowledgeEntry, f64)> = self
            .sentinel
            .watches(Some(Domain::Knowledge))?
            .into_iter()
            .map(|watch| {
                let score = crate::text_match::similarity(&concern, &watch.label);
                (
                    KnowledgeEntry {
                        watch_id: watch.watch_id,
                        fact: watch.label,
                        holding: watch.holding,
                        last_detail: watch.last_detail,
                        claim_id: watch.claim_id,
                    },
                    score,
                )
            })
            .filter(|(_, score)| *score > 0.0)
            .collect();
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        Ok(ranked.into_iter().map(|(entry, _)| entry).collect())
    }

    // ── 11. A platform-engineering team in a box (golden paths) ─────────

    /// Stake a golden path as a proof-minted, continuously-verified capability:
    /// its smoke test must keep passing for the path to be trusted. Combined with
    /// the [Forge](crate::forge) (fabricates + proves any missing tool), the
    /// [Architect](crate::architect) (composes the agents), and
    /// [weave](crate::weave) (turns NL intent into a composed run), a golden path
    /// can build and prove itself — and the drift ledger flags the instant it
    /// regresses across a version bump (the heal trigger).
    pub fn enroll_golden_path(&self, req: GoldenPathRequest) -> Result<EnrollReport, AppError> {
        let name = require(&req.name, "name")?;
        let command = require(&req.smoke_command, "smoke_command")?;
        let proposer = req
            .proposer
            .unwrap_or_else(|| format!("platform_{}", sanitize(&name)));
        let check = match req
            .expect
            .as_deref()
            .map(str::trim)
            .filter(|e| !e.is_empty())
        {
            Some(sub) => Check::ShellOutputContains {
                command,
                substring: sub.to_string(),
            },
            None => Check::CommandSucceeds { command },
        };
        let claim = self.enroll_one(
            proposer,
            format!("golden path '{name}' works"),
            ClaimKind::Assertion,
            check,
            Domain::GoldenPath,
            format!("path: {}", truncate(&name, 50)),
            vec![format!("golden_path:{name}")],
        )?;
        Ok(EnrollReport {
            domain: Domain::GoldenPath,
            summary: format!("enrolled golden path '{name}' (proof-gated, continuously verified)"),
            claims: vec![claim],
        })
    }

    // ── Shared adversary (the continuous red team / partition hunter) ───

    /// Drive the proposer/refuter populations for one round. The attack budget is
    /// spent value-of-information-ordered across the **entire open claim pool** —
    /// including every claim enrolled by the domains above — so this single driver
    /// is the AI red team, the data-contract partition hunter, and the migration
    /// equivalence attacker at once: it mints what survives and refutes what
    /// breaks. Needs a configured reasoner.
    pub async fn run_adversary(&self, req: AdversaryRequest) -> Result<RoundReport, AppError> {
        let topic = require(&req.topic, "topic")?;
        let conductor = self.conductor.clone().ok_or_else(|| {
            AppError::Validation(
                "the adversary needs a configured reasoner (no proof conductor attached)".into(),
            )
        })?;
        conductor
            .run_round(RoundConfig {
                topic,
                num_proposals: req.num_proposals.unwrap_or(1),
                attack_budget: req.attack_budget.unwrap_or(12),
            })
            .await
    }

    // ── shared enrollment path ──────────────────────────────────────────

    /// Stake a claim (its check runs for real on arrival) and, unless it was born
    /// violated, place its check under continuous re-verification.
    fn enroll_one(
        &self,
        proposer: String,
        statement: String,
        kind: ClaimKind,
        check: Check,
        domain: Domain,
        label: String,
        evidence: Vec<String>,
    ) -> Result<EnrolledClaim, AppError> {
        self.enroll_with_deps(
            proposer,
            statement,
            kind,
            check,
            domain,
            label,
            evidence,
            Vec::new(),
        )
    }

    /// As [`enroll_one`](Self::enroll_one) but records the minted claims this one
    /// builds on (e.g. a migration equivalence claim depending on a mined spec
    /// clause), so the dependency is durable in the ledger.
    #[allow(clippy::too_many_arguments)]
    fn enroll_with_deps(
        &self,
        proposer: String,
        statement: String,
        kind: ClaimKind,
        check: Check,
        domain: Domain,
        label: String,
        evidence: Vec<String>,
        depends_on: Vec<String>,
    ) -> Result<EnrolledClaim, AppError> {
        let claim = self.economy.propose(ProposeRequest {
            statement,
            kind,
            proposer,
            verification: check.clone(),
            evidence,
            depends_on,
        })?;
        let born_violated = claim.status == ClaimStatus::Refuted;
        let watch_id = if born_violated {
            None
        } else {
            let watch = self.sentinel.register(RegisterWatch {
                claim_id: Some(claim.claim_id.clone()),
                domain,
                label,
                check,
            })?;
            Some(watch.watch_id)
        };
        Ok(EnrolledClaim {
            claim,
            watch_id,
            born_violated,
        })
    }
}

// ── Domain request/response types ───────────────────────────────────────

/// idea 1 — a reproducibility capsule.
#[derive(Debug, Clone, Deserialize)]
pub struct ReproRequest {
    pub finding: String,
    /// The command that re-runs the analysis (e.g. `python analyze.py`).
    pub command: String,
    /// A substring of the output that proves the reported effect (e.g. `p=0.001`).
    pub expect_output: String,
    /// Optional working directory the command is run from.
    #[serde(default)]
    pub work_dir: Option<String>,
    /// Optional dataset path whose disappearance should refute reproducibility.
    #[serde(default)]
    pub dataset_path: Option<String>,
    #[serde(default)]
    pub proposer: Option<String>,
    #[serde(default)]
    pub evidence: Vec<String>,
}

/// idea 2 — a behavioral dependency contract.
#[derive(Debug, Clone, Deserialize)]
pub struct DependencyContractRequest {
    pub package: String,
    #[serde(default)]
    pub guarantees: Vec<BehaviorGuarantee>,
    #[serde(default)]
    pub prohibitions: Vec<BehaviorProhibition>,
    #[serde(default)]
    pub proposer: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BehaviorGuarantee {
    pub description: String,
    pub command: String,
    /// If set, the command's output must contain this; else it need only exit 0.
    #[serde(default)]
    pub expect: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BehaviorProhibition {
    pub description: String,
    /// A sandbox harness that exits 0 **iff** the forbidden behavior occurred
    /// (e.g. importing the package then attempting an outbound call). While it
    /// keeps failing, the prohibition holds.
    pub attempt_command: String,
}

/// idea 3 — a compliance control expressed as a software check.
#[derive(Debug, Clone, Deserialize)]
pub struct ControlRequest {
    pub framework: String,
    pub control_id: String,
    pub description: String,
    pub assertion: ControlAssertion,
    #[serde(default)]
    pub proposer: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ControlAssertion {
    /// The violating operation must fail (e.g. reading a bucket publicly).
    MustFail { command: String },
    /// The control operation must succeed.
    MustSucceed { command: String },
    /// The control command's output must contain the expected token.
    OutputMustContain { command: String, substring: String },
}

/// idea 4 — point the round at an opaque system to mine its spec.
#[derive(Debug, Clone, Deserialize)]
pub struct MineRequest {
    pub system: String,
    #[serde(default)]
    pub probes: Option<usize>,
    #[serde(default)]
    pub attack_budget: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MineReport {
    pub system: String,
    pub round: RoundReport,
    /// Minted behavioral claims = the adversarially-hardened discovered spec.
    pub spec: Vec<SpecClause>,
    pub watch_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SpecClause {
    pub claim_id: String,
    pub statement: String,
    pub check: Check,
}

/// idea 5 — bank a proven impossibility.
#[derive(Debug, Clone, Deserialize)]
pub struct ImpossibilityRequest {
    pub statement: String,
    /// A command that, if it ever succeeds, refutes the impossibility.
    pub attempt_command: String,
    #[serde(default)]
    pub evidence: Vec<String>,
    #[serde(default)]
    pub proposer: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImpossibilityRegistry {
    pub minted: Vec<Claim>,
    pub pending: Vec<Claim>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeadEndQuery {
    pub statement: String,
    #[serde(default)]
    pub threshold: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeadEndAnswer {
    pub known_dead_end: bool,
    pub score: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claim: Option<Claim>,
    pub note: String,
}

/// idea 6 — a vibe-coder's plain-language guardrail.
#[derive(Debug, Clone, Deserialize)]
pub struct GuardrailRequest {
    /// Plain-language intent: "my app must never expose user emails publicly".
    pub intent: String,
    /// A command that exits 0 **iff** the intent is violated (the leak is found).
    /// The common idiom is a probe piped into a matcher, e.g.
    /// `curl -s localhost:3000/api/users | grep -Eq "[a-z0-9.]+@[a-z]+"`.
    pub violation_command: String,
    #[serde(default)]
    pub proposer: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GuardrailQuery {
    /// A plain-language concern: "is my app safe re: leaking emails?"
    pub concern: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GuardrailMatch {
    pub watch_id: String,
    pub intent: String,
    pub holding: bool,
    pub score: f64,
    pub last_detail: String,
}

// ── Production-scale domain types (features 7–11) ───────────────────────

/// feature 1 — AI safety guardrails, continuously red-teamed.
#[derive(Debug, Clone, Deserialize)]
pub struct AiGuardrailRequest {
    pub policy: String,
    pub probes: Vec<SafetyProbe>,
    #[serde(default)]
    pub proposer: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SafetyProbe {
    /// The jailbreak/leak class this probe represents (e.g. "PII exfiltration").
    pub jailbreak_class: String,
    /// A command that exits 0 **iff** the assistant violated the policy on this
    /// probe (the jailbreak worked / PII leaked). While it keeps failing, the
    /// guardrail holds.
    pub probe_command: String,
}

/// feature 1 — an evaluation suite run as staked assertions.
#[derive(Debug, Clone, Deserialize)]
pub struct EvalSuiteRequest {
    pub suite: String,
    pub cases: Vec<EvalCase>,
    #[serde(default)]
    pub proposer: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EvalCase {
    pub description: String,
    /// Invokes the assistant harness on the fixed input.
    pub command: String,
    /// A substring the output must contain to pass.
    pub expect: String,
}

/// feature 2 — a data contract over a warehouse.
#[derive(Debug, Clone, Deserialize)]
pub struct DataContractRequest {
    pub dataset: String,
    pub expectations: Vec<DataExpectation>,
    #[serde(default)]
    pub proposer: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DataExpectation {
    /// A query whose output must contain the expected token (e.g. a count in
    /// range, a reconciled total).
    RowsMatch {
        description: String,
        command: String,
        substring: String,
    },
    /// A validation command that must exit 0 (e.g. a referential-integrity check).
    Holds {
        description: String,
        command: String,
    },
    /// A query that must FAIL — e.g. "select duplicate primary keys" returns no
    /// rows, so a duplicate is a banked impossibility.
    Impossible {
        description: String,
        command: String,
    },
}

/// feature 3 — a behavioral-equivalence claim (migration Phase 2).
#[derive(Debug, Clone, Deserialize)]
pub struct EquivalenceRequest {
    pub aspect: String,
    /// Exits 0 iff old and new behave identically (the common idiom diffs the
    /// outputs of the old and new code paths).
    pub equivalence_command: String,
    /// Optional minted spec clause (from `mine_spec`) this equivalence depends on.
    #[serde(default)]
    pub spec_claim_id: Option<String>,
    #[serde(default)]
    pub proposer: Option<String>,
}

/// feature 4 — an operational fact backed by a check.
#[derive(Debug, Clone, Deserialize)]
pub struct KnowledgeFactRequest {
    pub fact: String,
    pub check: KnowledgeCheck,
    #[serde(default)]
    pub evidence: Vec<String>,
    #[serde(default)]
    pub proposer: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum KnowledgeCheck {
    /// A file still contains the documented value (e.g. a config `key=value`).
    FileContains { path: String, substring: String },
    /// A command's output still contains the documented token.
    CommandOutputContains { command: String, substring: String },
    /// A documented command still succeeds (e.g. the deploy command still works).
    CommandSucceeds { command: String },
}

#[derive(Debug, Clone, Deserialize)]
pub struct KnowledgeQuery {
    pub concern: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct KnowledgeEntry {
    pub watch_id: String,
    pub fact: String,
    pub holding: bool,
    pub last_detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claim_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct KnowledgeBase {
    /// Facts whose check currently passes.
    pub verified: Vec<KnowledgeEntry>,
    /// Facts that have gone false — they refuted themselves and raised a flag.
    pub broken: Vec<KnowledgeEntry>,
}

/// feature 5 — a golden path (platform-engineering self-service).
#[derive(Debug, Clone, Deserialize)]
pub struct GoldenPathRequest {
    pub name: String,
    /// The smoke test that proves the path still works.
    pub smoke_command: String,
    #[serde(default)]
    pub expect: Option<String>,
    #[serde(default)]
    pub proposer: Option<String>,
}

/// The shared adversary driver input (continuous red team / partition hunter).
#[derive(Debug, Clone, Deserialize)]
pub struct AdversaryRequest {
    pub topic: String,
    #[serde(default)]
    pub num_proposals: Option<usize>,
    #[serde(default)]
    pub attack_budget: Option<usize>,
}

// ── small helpers ───────────────────────────────────────────────────────

fn require(value: &str, field: &str) -> Result<String, AppError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Err(AppError::Validation(format!("{field} must not be empty")))
    } else {
        Ok(trimmed.to_string())
    }
}

fn sanitize(value: &str) -> String {
    let cleaned: String = value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches('_').to_string();
    if trimmed.is_empty() {
        "pkg".into()
    } else {
        trimmed
    }
}

fn truncate(value: &str, max: usize) -> String {
    let trimmed = value.trim();
    if trimmed.chars().count() <= max {
        trimmed.to_string()
    } else {
        let head: String = trimmed.chars().take(max).collect();
        format!("{head}\u{2026}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device_agent::{DeviceCapabilities, DevicePolicy};
    use crate::proof_economy::AttackRequest;
    use std::sync::Arc;

    fn parts() -> (ProofEconomy, ContinuumService) {
        let device = Arc::new(DeviceCapabilities::new(DevicePolicy::permissive()));
        let economy = ProofEconomy::in_memory(device.clone()).unwrap();
        let sentinel = Sentinel::in_memory(device).unwrap();
        let continuum = ContinuumService::new(economy.clone(), sentinel);
        (economy, continuum)
    }

    #[test]
    fn reproducibility_finding_becomes_a_watched_claim() {
        let (_economy, continuum) = parts();
        let report = continuum
            .enroll_reproducibility(ReproRequest {
                finding: "treatment raises score by 0.42".into(),
                command: "echo effect size 0.42".into(),
                expect_output: "0.42".into(),
                work_dir: None,
                dataset_path: None,
                proposer: None,
                evidence: vec![],
            })
            .unwrap();
        assert_eq!(report.claims.len(), 1);
        let enrolled = &report.claims[0];
        assert!(!enrolled.born_violated, "the finding reproduces on arrival");
        assert_eq!(enrolled.claim.status, ClaimStatus::Proposed);
        assert!(
            enrolled.watch_id.is_some(),
            "it is now continuously watched"
        );
    }

    #[test]
    fn reproducibility_that_does_not_hold_is_born_violated_and_unwatched() {
        let (_economy, continuum) = parts();
        let report = continuum
            .enroll_reproducibility(ReproRequest {
                finding: "echo prints a number it does not".into(),
                command: "echo nothing-here".into(),
                expect_output: "0.42".into(),
                work_dir: None,
                dataset_path: None,
                proposer: None,
                evidence: vec![],
            })
            .unwrap();
        let enrolled = &report.claims[0];
        assert!(enrolled.born_violated);
        assert_eq!(enrolled.claim.status, ClaimStatus::Refuted);
        assert!(enrolled.watch_id.is_none());
    }

    #[test]
    fn dependency_contract_stakes_guarantee_and_impossibility() {
        let (_economy, continuum) = parts();
        let report = continuum
            .enroll_dependency_contract(DependencyContractRequest {
                package: "leftpad".into(),
                guarantees: vec![BehaviorGuarantee {
                    description: "pads to width".into(),
                    command: "echo XXXhi".into(),
                    expect: Some("XXXhi".into()),
                }],
                prohibitions: vec![BehaviorProhibition {
                    description: "make an outbound network call".into(),
                    attempt_command: "astra_nonexistent_zzz --connect example.com".into(),
                }],
                proposer: None,
            })
            .unwrap();
        assert_eq!(report.claims.len(), 2);
        // The guarantee holds; the impossibility holds (the call cannot be made).
        assert!(report.claims.iter().all(|c| !c.born_violated));
        assert!(
            report
                .claims
                .iter()
                .any(|c| c.claim.kind == ClaimKind::Impossibility)
        );
        assert!(report.claims.iter().all(|c| c.watch_id.is_some()));
    }

    #[test]
    fn compliance_must_fail_control_is_an_impossibility() {
        let (_economy, continuum) = parts();
        let report = continuum
            .enroll_control(ControlRequest {
                framework: "SOC2".into(),
                control_id: "CC6.1".into(),
                description: "no bucket is publicly readable".into(),
                assertion: ControlAssertion::MustFail {
                    command: "astra_nonexistent_zzz --read-public-bucket".into(),
                },
                proposer: None,
            })
            .unwrap();
        let enrolled = &report.claims[0];
        assert_eq!(enrolled.claim.kind, ClaimKind::Impossibility);
        assert!(!enrolled.born_violated, "the control holds under check");
        assert!(enrolled.watch_id.is_some());
    }

    #[test]
    fn impossibility_registry_banks_then_recalls_a_dead_end() {
        let (economy, continuum) = parts();
        let enrolled = continuum
            .register_impossibility(ImpossibilityRequest {
                statement: "compiling this kernel with -O3 cannot succeed here".into(),
                attempt_command: "astra_nonexistent_zzz --compile -O3".into(),
                evidence: vec![],
                proposer: None,
            })
            .unwrap();
        // Not yet a banked dead end — it must survive attacks to mint.
        assert!(
            !continuum
                .is_known_dead_end(DeadEndQuery {
                    statement: "compiling this kernel with -O3 cannot succeed here".into(),
                    threshold: None,
                })
                .unwrap()
                .known_dead_end
        );

        // Two failed attacks (the op really cannot succeed) mint it.
        for who in ["skeptic1", "skeptic2"] {
            economy
                .attack(
                    &enrolled.claim.claim_id,
                    AttackRequest {
                        attacker: who.into(),
                        note: "try it".into(),
                        counter: None,
                    },
                )
                .unwrap();
        }
        let registry = continuum.impossibility_registry().unwrap();
        assert_eq!(registry.minted.len(), 1);

        // Now a semantically-similar query recalls the banked dead end.
        let answer = continuum
            .is_known_dead_end(DeadEndQuery {
                statement: "can compiling this kernel with -O3 succeed here".into(),
                threshold: Some(0.3),
            })
            .unwrap();
        assert!(
            answer.known_dead_end,
            "should recall the minted impossibility"
        );
        assert!(answer.score >= 0.3);
    }

    #[test]
    fn guardrail_is_held_and_recalled_in_plain_language() {
        let (_economy, continuum) = parts();
        continuum
            .enroll_guardrail(GuardrailRequest {
                intent: "never expose user emails publicly".into(),
                // exits 0 iff a leak is found; this finds nothing → guardrail holds.
                violation_command: "astra_nonexistent_zzz --find-leaked-emails".into(),
                proposer: None,
            })
            .unwrap();

        let matches = continuum
            .query_guardrails(GuardrailQuery {
                concern: "are user emails exposed".into(),
            })
            .unwrap();
        assert!(!matches.is_empty(), "plain-language recall should find it");
        assert!(matches[0].holding, "the guardrail currently holds");
        assert!(matches[0].score > 0.0);
    }

    #[test]
    fn spec_mining_without_a_conductor_is_a_clean_error() {
        let (_economy, continuum) = parts();
        let result = actix_web::rt::System::new().block_on(continuum.mine_spec(MineRequest {
            system: "an undocumented service".into(),
            probes: Some(1),
            attack_budget: Some(1),
        }));
        assert!(result.is_err(), "mining needs a configured reasoner");
    }

    // ── features 7–11 ──

    #[test]
    fn ai_guardrails_stake_red_team_impossibilities() {
        let (_economy, continuum) = parts();
        let report = continuum
            .enroll_ai_guardrails(AiGuardrailRequest {
                policy: "no-pii".into(),
                probes: vec![SafetyProbe {
                    jailbreak_class: "PII exfiltration".into(),
                    // exits 0 iff the model leaked PII; this finds nothing → holds.
                    probe_command: "astra_nonexistent_zzz --leak-pii".into(),
                }],
                proposer: None,
            })
            .unwrap();
        assert_eq!(report.claims.len(), 1);
        assert_eq!(report.claims[0].claim.kind, ClaimKind::Impossibility);
        assert!(!report.claims[0].born_violated, "the guardrail holds");
        assert!(report.claims[0].watch_id.is_some());
    }

    #[test]
    fn eval_suite_case_holds_when_output_matches() {
        let (_economy, continuum) = parts();
        let report = continuum
            .enroll_eval_suite(EvalSuiteRequest {
                suite: "smoke".into(),
                cases: vec![EvalCase {
                    description: "greets".into(),
                    command: "echo hello world".into(),
                    expect: "hello".into(),
                }],
                proposer: None,
            })
            .unwrap();
        assert!(!report.claims[0].born_violated);
        assert_eq!(report.claims[0].claim.status, ClaimStatus::Proposed);
    }

    #[test]
    fn data_contract_mixes_assertions_and_an_impossibility() {
        let (_economy, continuum) = parts();
        let report = continuum
            .enroll_data_contract(DataContractRequest {
                dataset: "orders".into(),
                expectations: vec![
                    DataExpectation::RowsMatch {
                        description: "row count in range".into(),
                        command: "echo count=1000".into(),
                        substring: "count=1000".into(),
                    },
                    DataExpectation::Impossible {
                        description: "no duplicate primary key".into(),
                        command: "astra_nonexistent_zzz --select-dup-pks".into(),
                    },
                ],
                proposer: None,
            })
            .unwrap();
        assert_eq!(report.claims.len(), 2);
        assert!(
            report
                .claims
                .iter()
                .any(|c| c.claim.kind == ClaimKind::Impossibility)
        );
        assert!(report.claims.iter().all(|c| !c.born_violated));
    }

    #[test]
    fn equivalence_claim_holds_and_records_its_spec_dependency() {
        let (_economy, continuum) = parts();
        let enrolled = continuum
            .enroll_equivalence(EquivalenceRequest {
                aspect: "GET /users/1 shape".into(),
                equivalence_command: "echo same".into(), // exits 0 → equivalent
                spec_claim_id: Some("claim_spec_xyz".into()),
                proposer: None,
            })
            .unwrap();
        assert!(!enrolled.born_violated);
        assert_eq!(
            enrolled.claim.depends_on,
            vec!["claim_spec_xyz".to_string()]
        );
        assert!(enrolled.watch_id.is_some());
    }

    #[test]
    fn knowledge_fact_self_refutes_when_reality_drifts() {
        let (_economy, continuum) = parts();
        let dir = std::env::temp_dir().join(format!("astra-kb-{}", crate::common::now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("deploy.txt");
        std::fs::write(&file, "deploy: kubectl apply -f prod.yaml").unwrap();
        let path = file.to_string_lossy().to_string();

        let enrolled = continuum
            .record_fact(KnowledgeFactRequest {
                fact: "deploy with kubectl apply -f prod.yaml".into(),
                check: KnowledgeCheck::FileContains {
                    path: path.clone(),
                    substring: "kubectl apply -f prod.yaml".into(),
                },
                evidence: vec![],
                proposer: None,
            })
            .unwrap();
        assert!(!enrolled.born_violated);

        // It is a verified fact now.
        let kb = continuum.knowledge_base().unwrap();
        assert_eq!(kb.verified.len(), 1);
        assert_eq!(kb.broken.len(), 0);

        // Reality drifts: the deploy command changes. Sweep → the fact goes false.
        std::fs::write(&file, "deploy: helm upgrade prod").unwrap();
        let report = continuum.sweep().unwrap();
        assert_eq!(report.broke, 1);
        let kb = continuum.knowledge_base().unwrap();
        assert_eq!(kb.verified.len(), 0);
        assert_eq!(kb.broken.len(), 1, "the wrong fact refuted itself");

        // Plain-language recall still finds it, flagged as no longer holding.
        let hits = continuum
            .recall_fact(KnowledgeQuery {
                concern: "how do we deploy".into(),
            })
            .unwrap();
        assert!(!hits.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn golden_path_is_a_proof_gated_capability() {
        let (_economy, continuum) = parts();
        let report = continuum
            .enroll_golden_path(GoldenPathRequest {
                name: "new-service".into(),
                smoke_command: "echo scaffold-ok".into(),
                expect: Some("scaffold-ok".into()),
                proposer: None,
            })
            .unwrap();
        assert!(!report.claims[0].born_violated);
        assert!(report.claims[0].watch_id.is_some());
    }

    #[test]
    fn adversary_without_a_conductor_is_a_clean_error() {
        let (_economy, continuum) = parts();
        let result =
            actix_web::rt::System::new().block_on(continuum.run_adversary(AdversaryRequest {
                topic: "break the guardrails".into(),
                num_proposals: Some(1),
                attack_budget: Some(1),
            }));
        assert!(result.is_err());
    }
}
