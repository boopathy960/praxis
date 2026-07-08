//! Neural Orchestration Network — the plastic connectome that wires the mind.
//!
//! Every subsystem in this project — the [Sentinel](crate::sentinel),
//! [active inference](crate::active_inference), the [proof economy]
//! (crate::proof_economy), the [Forge](crate::forge), the [Architect]
//! (crate::architect), the [CEO](crate::ceo), [governance](crate::governance), the
//! [agentic loop](crate::agentic_loop) — is a capable organ. But organs are wired
//! together by hand, in `lib.rs`, in a fixed call graph. A brain is not a fixed
//! call graph: it is a *connectome* whose pathways carry signal with learned
//! strengths and **rewire from experience**. This module is that connectome.
//!
//! It models the system as a directed, weighted graph:
//!   * **Nodes** are the cognitive organs ([`NodeKind`]). Each has a resting bias.
//!   * **Edges** are synapses: a `weight` that says how strongly activation at the
//!     source should drive the target.
//!
//! Given a **stimulus** — a drift event, a spike in free energy, a user goal, a
//! governance alert — the network seeds activation at the organs the signal
//! touches and **propagates** it across the synapses for a few hops, with a
//! saturating nonlinearity and distance decay. The organs that light up above
//! threshold are the **global workspace** for that signal: an [`OrchestrationPlan`]
//! — an ordered set of subsystems to engage, with the network's own priority. This
//! is the missing coordinator: instead of a hard-coded "drift ⇒ call X", the
//! network *decides* which organs a signal should recruit, and in what order.
//!
//! And it **learns**. After a plan runs and an outcome is known, [`reinforce`]
//! (crate::neural_orchestration::NeuralOrchestrator::reinforce) applies a
//! reward-modulated Hebbian update: synapses between organs that co-fired and led
//! to a good outcome are strengthened, the rest relatively weaken ("cells that
//! fire together wire together", gated by reward). The routing topology is not
//! designed once — it is grown, and re-grown, by what actually works. Closed with
//! [active inference](crate::active_inference) (perceive → route → act → measure
//! surprise → reinforce), the collection of organs starts to behave like one mind.
//!
//! Honest boundary: this is a spreading-activation connectome with Hebbian
//! plasticity, not a differentiable neural network trained by backpropagation.
//! "Activation" is a bounded scalar, weights move by a deliberate local rule, and
//! the plan it emits is a routing recommendation — the engaged subsystems still do
//! the real, verified work. It is a coordinator and a learner, not an oracle.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use parking_lot::Mutex;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::common::{AppError, new_id, now_ms};

// ── Propagation & plasticity constants ─────────────────────────────────────
/// Fraction of a node's activation carried into the next hop (distance decay).
const CARRY: f64 = 0.45;
/// Gain on signal crossing a synapse before the nonlinearity.
const SYNAPTIC_GAIN: f64 = 0.9;
/// Hebbian learning rate for reward-modulated weight updates.
const HEBB_RATE: f64 = 0.15;
/// Maximum synaptic weight — keeps a runaway loop from saturating the graph.
const WEIGHT_MAX: f64 = 3.0;
/// Minimum synaptic weight (synapses do not vanish entirely).
const WEIGHT_MIN: f64 = 0.0;

/// A cognitive organ — one subsystem the connectome can recruit. Mirrors the real
/// services constructed in `lib.rs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    /// Continuous re-verification & the drift ledger.
    Sentinel,
    /// The generative model: prediction, surprise, attention.
    ActiveInference,
    /// The staked, adversarial knowledge ledger.
    ProofEconomy,
    /// The self-driving proposer/refuter round.
    ProofConductor,
    /// The six verification domains over the proof substrate.
    Continuum,
    /// The self-healing, proof-governed capability fabric.
    Forge,
    /// Agent-level genesis: composes & proves agent specs.
    Architect,
    /// The autonomous chief executive under governance.
    Ceo,
    /// The single governance authority (the court).
    Governance,
    /// Plan → act → observe → verify → replan.
    AgenticLoop,
    /// NL-intent → composed run orchestration.
    Weave,
    /// Continuity memory (episodes, decay, insight).
    Chronicle,
    /// The supervised host eyes & hands.
    DeviceLayer,
    /// Event enrichment & auditing.
    Nexus,
    /// The understanding organ: compress verified knowledge into theories,
    /// imagine their consequences, and pay rent in risky predictions.
    Noesis,
    /// Anything else placed on the connectome.
    Other,
}

/// A node in the connectome — a cognitive organ with a resting bias.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub node_id: String,
    pub kind: NodeKind,
    pub label: String,
    /// Resting potential added during propagation (an organ's baseline excitability).
    pub activation_bias: f64,
    /// How many times this node has fired (reached threshold in a plan).
    pub fired_count: u64,
    pub last_activation: f64,
    pub created_at_ms: i64,
    pub active: bool,
}

/// A synapse — a weighted, directed connection between two organs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub edge_id: String,
    pub from_node: String,
    pub to_node: String,
    /// Synaptic strength: how strongly source activation drives the target.
    pub weight: f64,
    /// Times both endpoints co-fired in a reinforced episode.
    pub co_activations: u64,
    /// Sum of reward applied across reinforcements (the synapse's track record).
    pub reward_sum: f64,
    pub updates: u64,
    pub created_at_ms: i64,
    pub last_fired_at_ms: i64,
}

/// One seed of a stimulus: where to inject signal, and how hard. `node` matches a
/// node id directly, or a [`NodeKind`] tag (snake_case) to seed every organ of
/// that kind.
#[derive(Debug, Clone, Deserialize)]
pub struct StimulusSeed {
    pub node: String,
    pub intensity: f64,
}

/// A signal to route through the connectome.
#[derive(Debug, Clone, Deserialize)]
pub struct StimulusRequest {
    /// What triggered this (e.g. "drift", "free_energy", "goal:deploy").
    pub origin: String,
    pub seeds: Vec<StimulusSeed>,
    /// Propagation depth (clamped 1..=6, default 3).
    #[serde(default)]
    pub hops: Option<usize>,
    /// Activation below this is not part of the firing set (default 0.12).
    #[serde(default)]
    pub threshold: Option<f64>,
    /// Cap on how many organs the plan engages (default 8).
    #[serde(default)]
    pub max_fire: Option<usize>,
}

/// An organ that fired for a stimulus, with the network's priority.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FiringNode {
    pub node_id: String,
    pub kind: NodeKind,
    pub label: String,
    pub activation: f64,
    pub rank: usize,
}

/// One propagation hop, for inspecting how a signal spread.
#[derive(Debug, Clone, Serialize)]
pub struct PropagationStep {
    pub hop: usize,
    /// Top activated nodes after this hop (label → activation).
    pub top: Vec<(String, f64)>,
}

/// The routing decision for one stimulus — the global workspace for that signal.
#[derive(Debug, Clone, Serialize)]
pub struct OrchestrationPlan {
    pub episode_id: String,
    pub origin: String,
    /// Engaged organs, highest priority first.
    pub firing: Vec<FiringNode>,
    pub trace: Vec<PropagationStep>,
    /// Sum of firing activations — how strongly the whole workspace lit up.
    pub workspace_energy: f64,
    pub at_ms: i64,
}

/// Apply learning to the synapses that fired in a past episode.
#[derive(Debug, Clone, Deserialize)]
pub struct ReinforceRequest {
    pub episode_id: String,
    /// Outcome signal in `-1.0..=1.0`. Positive rewards the pathway that fired;
    /// negative punishes it.
    pub reward: f64,
}

/// One synapse's change after reinforcement.
#[derive(Debug, Clone, Serialize)]
pub struct EdgeDelta {
    pub edge_id: String,
    pub from_node: String,
    pub to_node: String,
    pub weight_before: f64,
    pub weight_after: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReinforceReport {
    pub episode_id: String,
    pub reward: f64,
    pub edges_updated: usize,
    pub deltas: Vec<EdgeDelta>,
}

/// The whole connectome — organs and synapses.
#[derive(Debug, Clone, Serialize)]
pub struct Connectome {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

/// A snapshot of the network's structure & plasticity.
#[derive(Debug, Clone, Serialize)]
pub struct NetworkState {
    pub nodes: usize,
    pub edges: usize,
    pub mean_weight: f64,
    pub episodes: usize,
    /// The single strongest pathway right now (from → to, weight).
    pub strongest_pathway: Option<(String, String, f64)>,
}

/// A stored stimulus→plan episode, so reinforcement can replay which organs fired.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Episode {
    pub episode_id: String,
    pub origin: String,
    /// node_id → activation for every organ that fired.
    pub firing: BTreeMap<String, f64>,
    pub workspace_energy: f64,
    pub at_ms: i64,
}

/// The Neural Orchestration Network. Holds the durable connectome and the episode
/// ledger that makes its routing decisions reinforceable.
#[derive(Clone)]
pub struct NeuralOrchestrator {
    store: Arc<Mutex<Connection>>,
}

impl NeuralOrchestrator {
    pub fn new(data_dir: impl AsRef<Path>) -> Result<Self, AppError> {
        let path = data_dir.as_ref().join("neural_orchestration.sqlite");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                AppError::Internal(format!("failed to create neural orchestration dir: {e}"))
            })?;
        }
        let connection = Connection::open(&path).map_err(|e| {
            AppError::Internal(format!("failed to open neural orchestration ledger: {e}"))
        })?;
        connection.execute_batch(SCHEMA).map_err(sql_err)?;
        Ok(Self {
            store: Arc::new(Mutex::new(connection)),
        })
    }

    /// Test/in-memory variant.
    pub fn in_memory() -> Result<Self, AppError> {
        let connection = Connection::open_in_memory().map_err(sql_err)?;
        connection.execute_batch(SCHEMA).map_err(sql_err)?;
        Ok(Self {
            store: Arc::new(Mutex::new(connection)),
        })
    }

    /// Bootstrap the innate wiring: one node per cognitive organ and a sensible
    /// initial topology of synapses connecting them. Idempotent — nodes and edges
    /// use deterministic ids and are inserted only if absent, so re-seeding never
    /// clobbers weights the network has learned.
    pub fn seed_default_connectome(&self) -> Result<Connectome, AppError> {
        let now = now_ms();
        for (kind, label, bias) in DEFAULT_NODES {
            let node = Node {
                node_id: node_id_for(*kind),
                kind: *kind,
                label: (*label).into(),
                activation_bias: *bias,
                fired_count: 0,
                last_activation: 0.0,
                created_at_ms: now,
                active: true,
            };
            self.insert_node_if_absent(&node)?;
        }
        for (from, to, weight) in DEFAULT_EDGES {
            self.insert_edge_if_absent(*from, *to, *weight, now)?;
        }
        self.connectome()
    }

    /// Register (or update) an organ on the connectome.
    pub fn register_node(
        &self,
        kind: NodeKind,
        label: String,
        activation_bias: f64,
    ) -> Result<Node, AppError> {
        let node = Node {
            node_id: node_id_for(kind),
            kind,
            label: if label.trim().is_empty() {
                default_label(kind).to_string()
            } else {
                label.trim().to_string()
            },
            activation_bias: activation_bias.clamp(0.0, 1.0),
            fired_count: 0,
            last_activation: 0.0,
            created_at_ms: now_ms(),
            active: true,
        };
        self.save_node(&node)?;
        Ok(node)
    }

    /// Wire (or re-weight) a synapse between two organs.
    pub fn connect(&self, from: NodeKind, to: NodeKind, weight: f64) -> Result<Edge, AppError> {
        if from == to {
            return Err(AppError::Validation(
                "a synapse cannot connect an organ to itself".into(),
            ));
        }
        let from_id = node_id_for(from);
        let to_id = node_id_for(to);
        let now = now_ms();
        let edge = match self.find_edge(&from_id, &to_id)? {
            Some(mut existing) => {
                existing.weight = weight.clamp(WEIGHT_MIN, WEIGHT_MAX);
                existing
            }
            None => Edge {
                edge_id: new_id("syn"),
                from_node: from_id,
                to_node: to_id,
                weight: weight.clamp(WEIGHT_MIN, WEIGHT_MAX),
                co_activations: 0,
                reward_sum: 0.0,
                updates: 0,
                created_at_ms: now,
                last_fired_at_ms: now,
            },
        };
        self.save_edge(&edge)?;
        Ok(edge)
    }

    /// Route a stimulus through the connectome and return the organs to engage.
    /// Seeds activation at the named organs, propagates it across the synapses with
    /// distance decay and a saturating nonlinearity, then returns the supra-
    /// threshold firing set (the global workspace) ordered by activation. The
    /// episode is recorded so the resulting plan can later be reinforced.
    pub fn stimulate(&self, request: StimulusRequest) -> Result<OrchestrationPlan, AppError> {
        let hops = request.hops.unwrap_or(3).clamp(1, 6);
        let threshold = request.threshold.unwrap_or(0.12).clamp(0.0, 1.0);
        let max_fire = request.max_fire.unwrap_or(8).clamp(1, 64);
        let nodes = self.active_nodes()?;
        if nodes.is_empty() {
            return Err(AppError::Validation(
                "the connectome has no nodes — seed it first".into(),
            ));
        }
        let edges = self.active_edges()?;
        let label_of: BTreeMap<String, (NodeKind, String, f64)> = nodes
            .iter()
            .map(|n| {
                (
                    n.node_id.clone(),
                    (n.kind, n.label.clone(), n.activation_bias),
                )
            })
            .collect();

        // ── Seed activation ──
        let mut activation: BTreeMap<String, f64> = BTreeMap::new();
        for seed in &request.seeds {
            let intensity = seed.intensity.clamp(0.0, 1.0);
            for node_id in self.resolve_seed(&seed.node, &nodes) {
                *activation.entry(node_id).or_insert(0.0) += intensity;
            }
        }
        if activation.is_empty() {
            return Err(AppError::Validation(
                "no stimulus seed resolved to a node on the connectome".into(),
            ));
        }

        // ── Spreading activation ──
        let mut trace = Vec::new();
        for hop in 0..hops {
            let mut next: BTreeMap<String, f64> = BTreeMap::new();
            // Carry each node's current activation forward, decayed by distance.
            for (id, value) in &activation {
                *next.entry(id.clone()).or_insert(0.0) += value * CARRY;
            }
            // Drive targets across synapses.
            for edge in &edges {
                let source = activation.get(&edge.from_node).copied().unwrap_or(0.0);
                if source <= 0.0 {
                    continue;
                }
                *next.entry(edge.to_node.clone()).or_insert(0.0) +=
                    source * edge.weight * SYNAPTIC_GAIN;
            }
            // Resting bias + saturating nonlinearity keep activation in [0,1).
            for (id, value) in next.iter_mut() {
                let bias = label_of.get(id).map(|(_, _, b)| *b).unwrap_or(0.0);
                *value = (*value + bias).tanh();
            }
            trace.push(PropagationStep {
                hop,
                top: top_n(&next, &label_of, 4),
            });
            activation = next;
        }

        // ── Firing set ──
        let mut firing: Vec<FiringNode> = activation
            .iter()
            .filter(|(_, v)| **v >= threshold)
            .filter_map(|(id, v)| {
                label_of.get(id).map(|(kind, label, _)| FiringNode {
                    node_id: id.clone(),
                    kind: *kind,
                    label: label.clone(),
                    activation: *v,
                    rank: 0,
                })
            })
            .collect();
        firing.sort_by(|a, b| {
            b.activation
                .partial_cmp(&a.activation)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        firing.truncate(max_fire);
        for (index, node) in firing.iter_mut().enumerate() {
            node.rank = index + 1;
        }
        let workspace_energy = firing.iter().map(|n| n.activation).sum();
        let now = now_ms();
        let episode = Episode {
            episode_id: new_id("episode"),
            origin: request.origin.trim().to_string(),
            firing: firing
                .iter()
                .map(|n| (n.node_id.clone(), n.activation))
                .collect(),
            workspace_energy,
            at_ms: now,
        };
        self.save_episode(&episode)?;
        self.bump_fired(&firing, now)?;

        Ok(OrchestrationPlan {
            episode_id: episode.episode_id,
            origin: episode.origin,
            firing,
            trace,
            workspace_energy,
            at_ms: now,
        })
    }

    /// Reward-modulated Hebbian learning: for the synapses whose *both* endpoints
    /// fired in `episode_id`, move the weight by `rate * reward * a_from * a_to`.
    /// A pathway that fired and led to a good outcome is strengthened; one that led
    /// to a bad outcome is weakened. This is how the routing topology adapts.
    pub fn reinforce(&self, request: ReinforceRequest) -> Result<ReinforceReport, AppError> {
        let reward = request.reward.clamp(-1.0, 1.0);
        let episode = self.get_episode(&request.episode_id)?;
        let fired = &episode.firing;
        let mut deltas = Vec::new();
        let now = now_ms();
        for mut edge in self.active_edges()? {
            let (Some(&a_from), Some(&a_to)) =
                (fired.get(&edge.from_node), fired.get(&edge.to_node))
            else {
                continue;
            };
            let before = edge.weight;
            let delta = HEBB_RATE * reward * a_from * a_to;
            edge.weight = (edge.weight + delta).clamp(WEIGHT_MIN, WEIGHT_MAX);
            edge.co_activations += 1;
            edge.reward_sum += reward;
            edge.updates += 1;
            edge.last_fired_at_ms = now;
            self.save_edge(&edge)?;
            deltas.push(EdgeDelta {
                edge_id: edge.edge_id,
                from_node: edge.from_node,
                to_node: edge.to_node,
                weight_before: before,
                weight_after: edge.weight,
            });
        }
        Ok(ReinforceReport {
            episode_id: request.episode_id,
            reward,
            edges_updated: deltas.len(),
            deltas,
        })
    }

    /// The whole connectome — organs and synapses.
    pub fn connectome(&self) -> Result<Connectome, AppError> {
        Ok(Connectome {
            nodes: self.all_nodes()?,
            edges: self.active_edges()?,
        })
    }

    pub fn episodes(&self, limit: usize) -> Result<Vec<Episode>, AppError> {
        let limit = limit.clamp(1, 1000) as i64;
        let store = self.store.lock();
        let mut stmt = store
            .prepare("SELECT payload FROM no_episodes ORDER BY at_ms DESC LIMIT ?1")
            .map_err(sql_err)?;
        let rows = stmt
            .query_map(params![limit], |row| row.get::<_, String>(0))
            .map_err(sql_err)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(serde_json::from_str(&row.map_err(sql_err)?).map_err(de_err)?);
        }
        Ok(out)
    }

    pub fn status(&self) -> Result<NetworkState, AppError> {
        let nodes = self.all_nodes()?;
        let edges = self.active_edges()?;
        let mean_weight = if edges.is_empty() {
            0.0
        } else {
            edges.iter().map(|e| e.weight).sum::<f64>() / edges.len() as f64
        };
        let label_of: BTreeMap<String, String> = nodes
            .iter()
            .map(|n| (n.node_id.clone(), n.label.clone()))
            .collect();
        let strongest_pathway = edges
            .iter()
            .max_by(|a, b| {
                a.weight
                    .partial_cmp(&b.weight)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|edge| {
                (
                    label_of
                        .get(&edge.from_node)
                        .cloned()
                        .unwrap_or_else(|| edge.from_node.clone()),
                    label_of
                        .get(&edge.to_node)
                        .cloned()
                        .unwrap_or_else(|| edge.to_node.clone()),
                    edge.weight,
                )
            });
        Ok(NetworkState {
            nodes: nodes.len(),
            edges: edges.len(),
            mean_weight,
            episodes: self.count_episodes()?,
            strongest_pathway,
        })
    }

    // ── seed resolution ──────────────────────────────────────────────────────

    /// Resolve a seed token to node ids: an exact node id, or a [`NodeKind`] tag
    /// matching every node of that kind.
    fn resolve_seed(&self, token: &str, nodes: &[Node]) -> Vec<String> {
        let token = token.trim();
        if let Some(node) = nodes.iter().find(|n| n.node_id == token) {
            return vec![node.node_id.clone()];
        }
        match parse_kind(token) {
            Some(kind) => nodes
                .iter()
                .filter(|n| n.kind == kind)
                .map(|n| n.node_id.clone())
                .collect(),
            None => Vec::new(),
        }
    }

    // ── persistence ─────────────────────────────────────────────────────────

    fn all_nodes(&self) -> Result<Vec<Node>, AppError> {
        let store = self.store.lock();
        let mut stmt = store
            .prepare("SELECT payload FROM no_nodes ORDER BY created_at_ms ASC")
            .map_err(sql_err)?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(sql_err)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(serde_json::from_str(&row.map_err(sql_err)?).map_err(de_err)?);
        }
        Ok(out)
    }

    fn active_nodes(&self) -> Result<Vec<Node>, AppError> {
        Ok(self.all_nodes()?.into_iter().filter(|n| n.active).collect())
    }

    fn active_edges(&self) -> Result<Vec<Edge>, AppError> {
        let store = self.store.lock();
        let mut stmt = store
            .prepare("SELECT payload FROM no_edges ORDER BY created_at_ms ASC")
            .map_err(sql_err)?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(sql_err)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(serde_json::from_str(&row.map_err(sql_err)?).map_err(de_err)?);
        }
        Ok(out)
    }

    fn find_edge(&self, from_id: &str, to_id: &str) -> Result<Option<Edge>, AppError> {
        Ok(self
            .active_edges()?
            .into_iter()
            .find(|e| e.from_node == from_id && e.to_node == to_id))
    }

    fn save_node(&self, node: &Node) -> Result<(), AppError> {
        let payload = serde_json::to_string(node).map_err(ser_err)?;
        self.store
            .lock()
            .execute(
                "INSERT OR REPLACE INTO no_nodes (node_id, payload, created_at_ms) VALUES (?1,?2,?3)",
                params![node.node_id, payload, node.created_at_ms],
            )
            .map_err(sql_err)?;
        Ok(())
    }

    fn insert_node_if_absent(&self, node: &Node) -> Result<(), AppError> {
        let payload = serde_json::to_string(node).map_err(ser_err)?;
        self.store
            .lock()
            .execute(
                "INSERT OR IGNORE INTO no_nodes (node_id, payload, created_at_ms) VALUES (?1,?2,?3)",
                params![node.node_id, payload, node.created_at_ms],
            )
            .map_err(sql_err)?;
        Ok(())
    }

    fn save_edge(&self, edge: &Edge) -> Result<(), AppError> {
        let payload = serde_json::to_string(edge).map_err(ser_err)?;
        self.store
            .lock()
            .execute(
                "INSERT OR REPLACE INTO no_edges (edge_id, payload, created_at_ms) VALUES (?1,?2,?3)",
                params![edge.edge_id, payload, edge.created_at_ms],
            )
            .map_err(sql_err)?;
        Ok(())
    }

    fn insert_edge_if_absent(
        &self,
        from: NodeKind,
        to: NodeKind,
        weight: f64,
        now: i64,
    ) -> Result<(), AppError> {
        let from_id = node_id_for(from);
        let to_id = node_id_for(to);
        if self.find_edge(&from_id, &to_id)?.is_some() {
            return Ok(());
        }
        let edge = Edge {
            edge_id: format!("syn_{}_{}", kind_tag(from), kind_tag(to)),
            from_node: from_id,
            to_node: to_id,
            weight: weight.clamp(WEIGHT_MIN, WEIGHT_MAX),
            co_activations: 0,
            reward_sum: 0.0,
            updates: 0,
            created_at_ms: now,
            last_fired_at_ms: now,
        };
        self.save_edge(&edge)
    }

    fn bump_fired(&self, firing: &[FiringNode], now: i64) -> Result<(), AppError> {
        for fired in firing {
            if let Ok(mut node) = self.get_node(&fired.node_id) {
                node.fired_count += 1;
                node.last_activation = fired.activation;
                let _ = self.save_node(&node);
            }
        }
        let _ = now;
        Ok(())
    }

    fn get_node(&self, node_id: &str) -> Result<Node, AppError> {
        let payload: String = self
            .store
            .lock()
            .query_row(
                "SELECT payload FROM no_nodes WHERE node_id=?1",
                [node_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_err)?
            .ok_or_else(|| AppError::NotFound(format!("node {node_id}")))?;
        serde_json::from_str(&payload).map_err(de_err)
    }

    fn save_episode(&self, episode: &Episode) -> Result<(), AppError> {
        let payload = serde_json::to_string(episode).map_err(ser_err)?;
        self.store
            .lock()
            .execute(
                "INSERT OR REPLACE INTO no_episodes (episode_id, payload, at_ms) VALUES (?1,?2,?3)",
                params![episode.episode_id, payload, episode.at_ms],
            )
            .map_err(sql_err)?;
        Ok(())
    }

    fn get_episode(&self, episode_id: &str) -> Result<Episode, AppError> {
        let payload: String = self
            .store
            .lock()
            .query_row(
                "SELECT payload FROM no_episodes WHERE episode_id=?1",
                [episode_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_err)?
            .ok_or_else(|| AppError::NotFound(format!("episode {episode_id}")))?;
        serde_json::from_str(&payload).map_err(de_err)
    }

    fn count_episodes(&self) -> Result<usize, AppError> {
        let store = self.store.lock();
        let count: i64 = store
            .query_row("SELECT COUNT(*) FROM no_episodes", [], |row| row.get(0))
            .map_err(sql_err)?;
        Ok(count as usize)
    }
}

const SCHEMA: &str = "PRAGMA journal_mode=WAL;
    CREATE TABLE IF NOT EXISTS no_nodes (
        node_id TEXT PRIMARY KEY, payload TEXT NOT NULL, created_at_ms INTEGER NOT NULL);
    CREATE TABLE IF NOT EXISTS no_edges (
        edge_id TEXT PRIMARY KEY, payload TEXT NOT NULL, created_at_ms INTEGER NOT NULL);
    CREATE TABLE IF NOT EXISTS no_episodes (
        episode_id TEXT PRIMARY KEY, payload TEXT NOT NULL, at_ms INTEGER NOT NULL);
    CREATE INDEX IF NOT EXISTS no_episodes_at ON no_episodes(at_ms);";

/// The innate organs: (kind, label, resting bias).
const DEFAULT_NODES: &[(NodeKind, &str, f64)] = &[
    (NodeKind::Sentinel, "Sentinel (continuous re-verify)", 0.05),
    (
        NodeKind::ActiveInference,
        "Active inference (predict & attend)",
        0.05,
    ),
    (NodeKind::ProofEconomy, "Proof economy (staked truth)", 0.0),
    (
        NodeKind::ProofConductor,
        "Proof conductor (propose/refute)",
        0.0,
    ),
    (NodeKind::Continuum, "Continuum (verification domains)", 0.0),
    (NodeKind::Forge, "Forge (capability fabric)", 0.0),
    (NodeKind::Architect, "Architect (agent genesis)", 0.0),
    (NodeKind::Ceo, "CEO (executive cognition)", 0.02),
    (NodeKind::Governance, "Governance (the court)", 0.0),
    (NodeKind::AgenticLoop, "Agentic loop (act & verify)", 0.0),
    (NodeKind::Weave, "Weave (intent orchestration)", 0.0),
    (NodeKind::Chronicle, "Chronicle (continuity memory)", 0.0),
    (NodeKind::DeviceLayer, "Device layer (eyes & hands)", 0.0),
    (NodeKind::Nexus, "Nexus (event enrichment)", 0.0),
    (NodeKind::Noesis, "Noēsis (understand & imagine)", 0.02),
];

/// The innate synapses: (from, to, initial weight). The starting reflexes the
/// network refines from experience.
const DEFAULT_EDGES: &[(NodeKind, NodeKind, f64)] = &[
    // Perception: drift feeds the generative model; the model re-models the world.
    (NodeKind::Sentinel, NodeKind::ActiveInference, 0.7),
    (NodeKind::ProofEconomy, NodeKind::ActiveInference, 0.5),
    (NodeKind::Chronicle, NodeKind::ActiveInference, 0.3),
    (NodeKind::Nexus, NodeKind::Sentinel, 0.3),
    (NodeKind::DeviceLayer, NodeKind::Sentinel, 0.3),
    // Surprise escalates: to the executive, to capability repair, to fresh proof.
    (NodeKind::ActiveInference, NodeKind::Ceo, 0.5),
    (NodeKind::ActiveInference, NodeKind::Forge, 0.5),
    (NodeKind::ActiveInference, NodeKind::ProofConductor, 0.5),
    (NodeKind::ActiveInference, NodeKind::Continuum, 0.4),
    // Understanding: surprise and settled facts demand a better theory; a theory
    // demands proof of its conjectures and re-seeds the model's priors.
    (NodeKind::ActiveInference, NodeKind::Noesis, 0.5),
    (NodeKind::ProofEconomy, NodeKind::Noesis, 0.4),
    (NodeKind::Noesis, NodeKind::ProofConductor, 0.5),
    (NodeKind::Noesis, NodeKind::ActiveInference, 0.4),
    // Verification substrate.
    (NodeKind::Continuum, NodeKind::Sentinel, 0.7),
    (NodeKind::Continuum, NodeKind::ProofEconomy, 0.6),
    (NodeKind::ProofConductor, NodeKind::ProofEconomy, 0.8),
    (NodeKind::AgenticLoop, NodeKind::ProofEconomy, 0.6),
    // Capability growth.
    (NodeKind::Forge, NodeKind::Architect, 0.6),
    (NodeKind::Architect, NodeKind::AgenticLoop, 0.6),
    (NodeKind::Weave, NodeKind::AgenticLoop, 0.5),
    (NodeKind::Weave, NodeKind::Architect, 0.4),
    // Executive & governance.
    (NodeKind::Ceo, NodeKind::Governance, 0.7),
    (NodeKind::Ceo, NodeKind::Forge, 0.4),
    (NodeKind::Ceo, NodeKind::Architect, 0.4),
    (NodeKind::Governance, NodeKind::DeviceLayer, 0.5),
];

fn node_id_for(kind: NodeKind) -> String {
    format!("node_{}", kind_tag(kind))
}

fn kind_tag(kind: NodeKind) -> String {
    serde_json::to_value(kind)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_else(|| "other".into())
}

fn parse_kind(token: &str) -> Option<NodeKind> {
    serde_json::from_value(serde_json::Value::String(token.trim().to_ascii_lowercase())).ok()
}

fn default_label(kind: NodeKind) -> &'static str {
    DEFAULT_NODES
        .iter()
        .find(|(k, _, _)| *k == kind)
        .map(|(_, label, _)| *label)
        .unwrap_or("organ")
}

fn top_n(
    activation: &BTreeMap<String, f64>,
    label_of: &BTreeMap<String, (NodeKind, String, f64)>,
    n: usize,
) -> Vec<(String, f64)> {
    let mut items: Vec<(String, f64)> = activation
        .iter()
        .map(|(id, v)| {
            let label = label_of
                .get(id)
                .map(|(_, l, _)| l.clone())
                .unwrap_or_else(|| id.clone());
            (label, *v)
        })
        .collect();
    items.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    items.truncate(n);
    items
}

/// Spawn the continuous orchestration reflex on its own thread+runtime. Gated
/// behind `ASTRA_NEURAL_ORCHESTRATION=on`; every
/// `ASTRA_NEURAL_ORCHESTRATION_INTERVAL_SECS` (default 100, min 15) it reads the
/// live state of [active inference](crate::active_inference) (free energy) and the
/// [Sentinel](crate::sentinel) (drift), encodes them as a stimulus, routes it, and
/// — closing the loop — reinforces the pathway by whether free energy fell since
/// the last tick. This is the connectome learning, online, which subsystems to
/// recruit when the system is surprised.
pub fn spawn_neural_orchestrator(
    orchestrator: NeuralOrchestrator,
    active_inference: crate::active_inference::ActiveInference,
    sentinel: crate::sentinel::Sentinel,
) {
    let enabled = std::env::var("ASTRA_NEURAL_ORCHESTRATION")
        .map(|v| matches!(v.as_str(), "1" | "true" | "on" | "yes" | "TRUE"))
        .unwrap_or(false);
    if !enabled {
        tracing::info!(
            "neural orchestration disabled (set ASTRA_NEURAL_ORCHESTRATION=on to enable the routing reflex)"
        );
        return;
    }
    // Make sure the innate wiring exists before the reflex runs.
    if let Err(error) = orchestrator.seed_default_connectome() {
        tracing::warn!(%error, "neural orchestration could not seed its connectome");
        return;
    }
    let interval = std::env::var("ASTRA_NEURAL_ORCHESTRATION_INTERVAL_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|s| *s >= 15)
        .unwrap_or(100);
    std::thread::Builder::new()
        .name("astra-neural-orchestrator".into())
        .spawn(move || {
            actix_web::rt::System::new().block_on(async move {
                let mut last_free_energy: Option<f64> = None;
                loop {
                    // Read the system's live drives.
                    let mind = active_inference.status().ok();
                    let free_energy = mind.as_ref().map(|m| m.resting_free_energy).unwrap_or(0.0);
                    let drift = sentinel
                        .drift_log(None, 50)
                        .map(|log| log.iter().filter(|e| e.is_regression()).count())
                        .unwrap_or(0);
                    // Encode drives as stimulus intensities (saturating).
                    let fe_intensity = (free_energy / 5.0).clamp(0.0, 1.0);
                    let drift_intensity = (drift as f64 / 5.0).clamp(0.0, 1.0);
                    let seeds = vec![
                        StimulusSeed {
                            node: "active_inference".into(),
                            intensity: fe_intensity.max(0.1),
                        },
                        StimulusSeed {
                            node: "sentinel".into(),
                            intensity: drift_intensity.max(0.1),
                        },
                    ];
                    match orchestrator.stimulate(StimulusRequest {
                        origin: "reflex:free_energy+drift".into(),
                        seeds,
                        hops: Some(3),
                        threshold: Some(0.12),
                        max_fire: Some(8),
                    }) {
                        Ok(plan) => {
                            // Reward the pathway if the system's surprise fell.
                            if let Some(prev) = last_free_energy {
                                let reward = (prev - free_energy).clamp(-1.0, 1.0);
                                if reward.abs() > f64::EPSILON {
                                    let _ = orchestrator.reinforce(ReinforceRequest {
                                        episode_id: plan.episode_id.clone(),
                                        reward,
                                    });
                                }
                            }
                            let engaged: Vec<&str> =
                                plan.firing.iter().map(|n| n.label.as_str()).collect();
                            tracing::info!(
                                workspace_energy = plan.workspace_energy,
                                free_energy,
                                drift_regressions = drift,
                                engaged = ?engaged,
                                "neural orchestration routed a reflex"
                            );
                        }
                        Err(error) => {
                            tracing::warn!(error = %error, "neural orchestration stimulus failed")
                        }
                    }
                    last_free_energy = Some(free_energy);
                    actix_web::rt::time::sleep(std::time::Duration::from_secs(interval)).await;
                }
            });
        })
        .expect("failed to spawn neural orchestrator thread");
}

fn sql_err(e: rusqlite::Error) -> AppError {
    AppError::Internal(format!("neural orchestration sql error: {e}"))
}
fn ser_err(e: serde_json::Error) -> AppError {
    AppError::Internal(format!("neural orchestration serialize error: {e}"))
}
fn de_err(e: serde_json::Error) -> AppError {
    AppError::Internal(format!("neural orchestration decode error: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn net() -> NeuralOrchestrator {
        let net = NeuralOrchestrator::in_memory().expect("net");
        net.seed_default_connectome().expect("seed");
        net
    }

    #[test]
    fn seeding_is_idempotent_and_builds_the_connectome() {
        let net = net();
        let first = net.connectome().unwrap();
        assert_eq!(first.nodes.len(), DEFAULT_NODES.len());
        assert_eq!(first.edges.len(), DEFAULT_EDGES.len());
        // Re-seeding must not duplicate or reset anything.
        net.seed_default_connectome().unwrap();
        let second = net.connectome().unwrap();
        assert_eq!(second.nodes.len(), DEFAULT_NODES.len());
        assert_eq!(second.edges.len(), DEFAULT_EDGES.len());
    }

    #[test]
    fn a_stimulus_at_the_sentinel_recruits_active_inference() {
        let net = net();
        let plan = net
            .stimulate(StimulusRequest {
                origin: "drift".into(),
                seeds: vec![StimulusSeed {
                    node: "sentinel".into(),
                    intensity: 1.0,
                }],
                hops: Some(3),
                threshold: Some(0.1),
                max_fire: Some(8),
            })
            .unwrap();
        assert!(!plan.firing.is_empty(), "the workspace should light up");
        let kinds: Vec<NodeKind> = plan.firing.iter().map(|n| n.kind).collect();
        assert!(
            kinds.contains(&NodeKind::ActiveInference),
            "Sentinel → ActiveInference is an innate pathway and should fire"
        );
    }

    #[test]
    fn positive_reward_strengthens_the_pathway_that_fired() {
        let net = net();
        let plan = net
            .stimulate(StimulusRequest {
                origin: "drift".into(),
                seeds: vec![StimulusSeed {
                    node: "sentinel".into(),
                    intensity: 1.0,
                }],
                hops: Some(2),
                threshold: Some(0.05),
                max_fire: Some(16),
            })
            .unwrap();
        let weight_before = net
            .connectome()
            .unwrap()
            .edges
            .into_iter()
            .find(|e| e.from_node == node_id_for(NodeKind::Sentinel))
            .map(|e| e.weight)
            .unwrap();
        let report = net
            .reinforce(ReinforceRequest {
                episode_id: plan.episode_id.clone(),
                reward: 1.0,
            })
            .unwrap();
        assert!(report.edges_updated > 0, "co-fired synapses should update");
        let weight_after = net
            .connectome()
            .unwrap()
            .edges
            .into_iter()
            .find(|e| e.from_node == node_id_for(NodeKind::Sentinel))
            .map(|e| e.weight)
            .unwrap();
        assert!(
            weight_after > weight_before,
            "a rewarded co-firing pathway should strengthen ({weight_before} -> {weight_after})"
        );
    }

    #[test]
    fn negative_reward_weakens_the_pathway() {
        let net = net();
        let plan = net
            .stimulate(StimulusRequest {
                origin: "bad-route".into(),
                seeds: vec![StimulusSeed {
                    node: "active_inference".into(),
                    intensity: 1.0,
                }],
                hops: Some(2),
                threshold: Some(0.05),
                max_fire: Some(16),
            })
            .unwrap();
        let edge_from = node_id_for(NodeKind::ActiveInference);
        let before = net
            .connectome()
            .unwrap()
            .edges
            .into_iter()
            .find(|e| e.from_node == edge_from)
            .map(|e| e.weight)
            .unwrap();
        net.reinforce(ReinforceRequest {
            episode_id: plan.episode_id,
            reward: -1.0,
        })
        .unwrap();
        let after = net
            .connectome()
            .unwrap()
            .edges
            .into_iter()
            .find(|e| e.from_node == node_id_for(NodeKind::ActiveInference))
            .map(|e| e.weight)
            .unwrap();
        assert!(after < before, "a punished pathway should weaken");
    }

    #[test]
    fn unknown_seed_is_a_clean_error() {
        let net = net();
        let result = net.stimulate(StimulusRequest {
            origin: "x".into(),
            seeds: vec![StimulusSeed {
                node: "not_a_real_organ".into(),
                intensity: 1.0,
            }],
            hops: None,
            threshold: None,
            max_fire: None,
        });
        assert!(
            result.is_err(),
            "a seed that resolves to nothing should error"
        );
    }
}
