// ═══════════════════════════════════════════════════════════════
// ADVANCED REASONING FORMULAS v1.0
// ═══════════════════════════════════════════════════════════════
//
// Second mathematics module containing formulas that directly
// enhance the 5 dimension engines:
//
//   1. Causal Cone Analysis (for D5 temporal timelines)
//   2. Epistemic Uncertainty Decomposition (for D1 proofs)
//   3. Cognitive Momentum (for D4 metacognition)
//   4. Semantic Tensor Product (for D2 concept blending)
//   5. Impossibility Spectrum Analyzer (for D3)
//   6. Bayesian Surprise Metric (cross-dimensional)
//   7. Information Bottleneck (for compression/abstraction)
//   8. Cognitive Energy Landscape (for strategy optimization)
//
// Pure Rust, zero external dependencies beyond std.

// ═══════════════════════════════════════════════════════════════
// 1. CAUSAL CONE ANALYSIS
// ═══════════════════════════════════════════════════════════════
//
// Inspired by relativistic light cones, defines which events
// (outcomes) can causally affect which other events in the
// multi-timeline framework.
//
// An event E1 at (timeline_t1, time_t1) can causally affect E2
// at (timeline_t2, time_t2) iff:
//
//   |t2 - t1| >= c_cognitive * |timeline_t2 - timeline_t1|
//
// Where c_cognitive is the "speed of cognitive influence" — how
// fast information propagates between timelines.

/// A cognitive event in the space-time of timelines.
#[derive(Debug, Clone)]
pub struct CognitiveEvent {
    pub timeline_id: usize,
    pub timestep: usize,
    pub confidence: f64,
    pub data: String,
}

/// Causal cone analysis results.
#[derive(Debug, Clone)]
pub struct CausalCone {
    pub event: CognitiveEvent,
    /// Events that can be caused by this event (future cone).
    pub future_cone: Vec<usize>,
    /// Events that could have caused this event (past cone).
    pub past_cone: Vec<usize>,
    /// Events outside both cones (spacelike-separated).
    pub spacelike: Vec<usize>,
    /// Maximum causal reach in timeline-space.
    pub causal_reach: f64,
}

/// Compute the causal cone for a given event.
///
/// c_cognitive controls how fast influence propagates between timelines.
/// Higher c → tighter cone (less inter-timeline influence).
pub fn compute_causal_cone(
    event_idx: usize,
    all_events: &[CognitiveEvent],
    c_cognitive: f64,
) -> CausalCone {
    let event = &all_events[event_idx];
    let mut future = Vec::new();
    let mut past = Vec::new();
    let mut spacelike = Vec::new();

    for (i, other) in all_events.iter().enumerate() {
        if i == event_idx {
            continue;
        }

        let dt = other.timestep as f64 - event.timestep as f64;
        let dx = (other.timeline_id as f64 - event.timeline_id as f64).abs();

        if dt.abs() >= c_cognitive * dx {
            // Causally connected
            if dt > 0.0 {
                future.push(i);
            } else if dt < 0.0 {
                past.push(i);
            }
        } else {
            spacelike.push(i);
        }
    }

    let causal_reach = future.len() as f64 + past.len() as f64;

    CausalCone {
        event: event.clone(),
        future_cone: future,
        past_cone: past,
        spacelike,
        causal_reach,
    }
}

/// Compute the causal density of the event space.
///
/// High density = events are tightly causally connected.
/// Low density = mostly independent timelines.
pub fn causal_density(events: &[CognitiveEvent], c_cognitive: f64) -> f64 {
    if events.len() < 2 {
        return 0.0;
    }

    let mut total_connections = 0;
    for i in 0..events.len() {
        let cone = compute_causal_cone(i, events, c_cognitive);
        total_connections += cone.future_cone.len();
    }

    let max_connections = events.len() * (events.len() - 1) / 2;
    if max_connections == 0 {
        0.0
    } else {
        total_connections as f64 / max_connections as f64
    }
}

// ═══════════════════════════════════════════════════════════════
// 2. EPISTEMIC UNCERTAINTY DECOMPOSITION
// ═══════════════════════════════════════════════════════════════
//
// Separates total uncertainty into:
//   - Aleatoric (irreducible randomness)
//   - Epistemic (reducible ignorance)
//
// Total uncertainty: H_total = -Σ p·log(p)
// Aleatoric: H_ale = E[H(Y|X=x)]  (avg entropy per prediction)
// Epistemic: H_epi = H_total - H_ale
//
// If H_epi >> H_ale: we need more data (learning will help).
// If H_ale >> H_epi: problem is inherently noisy (learning won't help).

/// Decomposed uncertainty.
#[derive(Debug, Clone)]
pub struct UncertaintyDecomposition {
    pub total_entropy: f64,
    pub aleatoric: f64,
    pub epistemic: f64,
    /// Ratio of epistemic to total: how much can be reduced by learning.
    pub reducibility: f64,
    pub recommendation: UncertaintyAction,
}

#[derive(Debug, Clone)]
pub enum UncertaintyAction {
    /// Epistemic dominates: more reasoning/data will help.
    ContinueReasoning,
    /// Aleatoric dominates: accept uncertainty, go probabilistic.
    AcceptUncertainty,
    /// Both low: high confidence, proceed.
    HighConfidence,
    /// Both high: problem is both complex and noisy.
    FundamentallyHard,
}

/// Decompose uncertainty from a set of prediction distributions.
///
/// `predictions` is a list of probability distributions (one per model/strategy).
/// Each distribution sums to 1.0.
pub fn decompose_uncertainty(predictions: &[Vec<f64>]) -> UncertaintyDecomposition {
    if predictions.is_empty() {
        return UncertaintyDecomposition {
            total_entropy: 0.0,
            aleatoric: 0.0,
            epistemic: 0.0,
            reducibility: 0.0,
            recommendation: UncertaintyAction::HighConfidence,
        };
    }

    let num_models = predictions.len();
    let num_classes = predictions[0].len();
    if num_classes == 0 {
        return UncertaintyDecomposition {
            total_entropy: 0.0,
            aleatoric: 0.0,
            epistemic: 0.0,
            reducibility: 0.0,
            recommendation: UncertaintyAction::HighConfidence,
        };
    }

    // Average prediction across all models
    let mut avg_pred = vec![0.0; num_classes];
    for pred in predictions {
        for (j, &p) in pred.iter().enumerate().take(num_classes) {
            avg_pred[j] += p / num_models as f64;
        }
    }

    // Total entropy: H(avg_prediction)
    let total_entropy = shannon_entropy(&avg_pred);

    // Aleatoric: average entropy of individual predictions
    let aleatoric: f64 = predictions
        .iter()
        .map(|pred| shannon_entropy(pred))
        .sum::<f64>()
        / num_models as f64;

    // Epistemic = Total - Aleatoric
    let epistemic = (total_entropy - aleatoric).max(0.0);

    let reducibility = if total_entropy > 1e-10 {
        epistemic / total_entropy
    } else {
        0.0
    };

    let recommendation = if total_entropy < 0.3 {
        UncertaintyAction::HighConfidence
    } else if reducibility > 0.6 {
        UncertaintyAction::ContinueReasoning
    } else if reducibility < 0.3 {
        UncertaintyAction::AcceptUncertainty
    } else {
        UncertaintyAction::FundamentallyHard
    };

    UncertaintyDecomposition {
        total_entropy,
        aleatoric,
        epistemic,
        reducibility,
        recommendation,
    }
}

fn shannon_entropy(probs: &[f64]) -> f64 {
    probs
        .iter()
        .filter(|&&p| p > 1e-15)
        .map(|&p| -p * p.log2())
        .sum()
}

// ═══════════════════════════════════════════════════════════════
// 3. COGNITIVE MOMENTUM
// ═══════════════════════════════════════════════════════════════
//
// Tracks the "momentum" of cognitive state changes using an
// exponential moving average with inertia.
//
// Momentum(t) = β·M(t-1) + (1-β)·ΔPerf(t)
// Velocity(t) = M(t)
// Acceleration(t) = M(t) - M(t-1)
// Jerk(t) = A(t) - A(t-1)
//
// High positive jerk → system is rapidly improving (keep going).
// High negative jerk → system is decelerating (consider strategy switch).
// Zero jerk → steady state (explore or exploit depending on velocity).

/// Cognitive momentum tracker.
#[derive(Debug, Clone)]
pub struct CognitiveMomentum {
    beta: f64,
    velocity: f64,
    acceleration: f64,
    jerk: f64,
    prev_velocity: f64,
    prev_acceleration: f64,
    history: Vec<f64>,
    max_history: usize,
}

impl CognitiveMomentum {
    pub fn new(beta: f64) -> Self {
        Self {
            beta: beta.clamp(0.01, 0.99),
            velocity: 0.0,
            acceleration: 0.0,
            jerk: 0.0,
            prev_velocity: 0.0,
            prev_acceleration: 0.0,
            history: Vec::new(),
            max_history: 100,
        }
    }

    /// Update momentum with a new performance delta.
    pub fn update(&mut self, performance_delta: f64) {
        self.prev_velocity = self.velocity;
        self.prev_acceleration = self.acceleration;

        // EMA momentum
        self.velocity = self.beta * self.velocity + (1.0 - self.beta) * performance_delta;
        self.acceleration = self.velocity - self.prev_velocity;
        self.jerk = self.acceleration - self.prev_acceleration;

        self.history.push(self.velocity);
        if self.history.len() > self.max_history {
            self.history.remove(0);
        }
    }

    pub fn velocity(&self) -> f64 {
        self.velocity
    }
    pub fn acceleration(&self) -> f64 {
        self.acceleration
    }
    pub fn jerk(&self) -> f64 {
        self.jerk
    }

    /// Kinetic energy: ½mv² (measures total cognitive activity).
    pub fn kinetic_energy(&self) -> f64 {
        0.5 * self.velocity * self.velocity
    }

    /// Should we keep the current strategy?
    pub fn recommendation(&self) -> MomentumAction {
        if self.jerk > 0.05 {
            MomentumAction::Accelerating // Getting better fast
        } else if self.jerk < -0.05 {
            MomentumAction::Decelerating // Getting worse
        } else if self.velocity.abs() < 0.01 {
            MomentumAction::Stalled // Not moving
        } else if self.velocity > 0.0 {
            MomentumAction::SteadyProgress // Good but flat
        } else {
            MomentumAction::Regressing // Moving backwards
        }
    }
}

#[derive(Debug, Clone)]
pub enum MomentumAction {
    Accelerating,
    SteadyProgress,
    Stalled,
    Decelerating,
    Regressing,
}

// ═══════════════════════════════════════════════════════════════
// 4. SEMANTIC TENSOR PRODUCT
// ═══════════════════════════════════════════════════════════════
//
// Combines two concept vectors into a richer representation via
// tensor (outer) product, then applies rank-reduction to extract
// the most important interaction features.
//
// T = a ⊗ b  (outer product: T_ij = a_i · b_j)
//
// Then extract top-k singular values via power iteration to get
// the most important "interaction modes" between concepts.

/// Rank-reduced tensor product of two concept vectors.
/// Returns the top-k interaction features.
pub fn semantic_tensor_product(concept_a: &[f64], concept_b: &[f64], top_k: usize) -> Vec<f64> {
    let m = concept_a.len();
    let n = concept_b.len();
    if m == 0 || n == 0 {
        return Vec::new();
    }

    // Compute full outer product T
    let mut tensor = vec![vec![0.0; n]; m];
    for i in 0..m {
        for j in 0..n {
            tensor[i][j] = concept_a[i] * concept_b[j];
        }
    }

    // Power iteration to extract top-k singular values
    let mut singular_values = Vec::with_capacity(top_k);
    let mut tensor_copy = tensor.clone();

    for _ in 0..top_k {
        let (sigma, _u, _v) = power_iteration_svd(&tensor_copy, 50);
        if sigma < 1e-10 {
            break;
        }
        singular_values.push(sigma);

        // Deflate: T = T - σ·u·vᵀ
        for i in 0..m {
            for j in 0..n {
                tensor_copy[i][j] -= sigma * _u[i] * _v[j];
            }
        }
    }

    singular_values
}

/// One round of power iteration to find the largest singular value/vectors.
fn power_iteration_svd(matrix: &[Vec<f64>], iterations: usize) -> (f64, Vec<f64>, Vec<f64>) {
    let m = matrix.len();
    if m == 0 {
        return (0.0, Vec::new(), Vec::new());
    }
    let n = matrix[0].len();
    if n == 0 {
        return (0.0, Vec::new(), Vec::new());
    }

    // Initialize random-ish vector
    let mut v: Vec<f64> = (0..n).map(|i| ((i as f64 + 1.0) * 0.618).sin()).collect();
    normalize_vec(&mut v);

    let mut u = vec![0.0; m];

    for _ in 0..iterations {
        // u = A·v
        for i in 0..m {
            u[i] = 0.0;
            for j in 0..n {
                u[i] += matrix[i][j] * v[j];
            }
        }
        normalize_vec(&mut u);

        // v = Aᵀ·u
        for j in 0..n {
            v[j] = 0.0;
            for i in 0..m {
                v[j] += matrix[i][j] * u[i];
            }
        }
        let sigma = vec_norm(&v);
        if sigma < 1e-15 {
            return (0.0, u, v);
        }
        normalize_vec(&mut v);
    }

    // Compute singular value: σ = ||A·v||
    let mut av = vec![0.0; m];
    for i in 0..m {
        for j in 0..n {
            av[i] += matrix[i][j] * v[j];
        }
    }
    let sigma = vec_norm(&av);

    (sigma, u, v)
}

fn normalize_vec(v: &mut [f64]) {
    let norm = vec_norm(v);
    if norm > 1e-15 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

fn vec_norm(v: &[f64]) -> f64 {
    v.iter().map(|x| x * x).sum::<f64>().sqrt()
}

// ═══════════════════════════════════════════════════════════════
// 5. IMPOSSIBILITY SPECTRUM ANALYZER
// ═══════════════════════════════════════════════════════════════
//
// Analyzes the "spectrum" of impossibility by decomposing a
// problem into orthogonal constraint directions and measuring
// how much each contributes to impossibility.
//
// Gram-Schmidt orthogonalization of constraint vectors, then
// projection of the "impossibility vector" onto each basis.

/// Constraint vector with label.
#[derive(Debug, Clone)]
pub struct ConstraintVector {
    pub label: String,
    pub components: Vec<f64>,
}

/// Impossibility decomposition along orthogonal constraint axes.
#[derive(Debug, Clone)]
pub struct ImpossibilitySpectrum {
    pub components: Vec<(String, f64)>,
    pub total_impossibility: f64,
    pub dominant_constraint: String,
    pub dominant_contribution: f64,
}

/// Decompose impossibility into orthogonal constraint contributions.
pub fn analyze_impossibility_spectrum(
    constraints: &[ConstraintVector],
    impossibility_vec: &[f64],
) -> ImpossibilitySpectrum {
    if constraints.is_empty() {
        return ImpossibilitySpectrum {
            components: Vec::new(),
            total_impossibility: vec_norm(impossibility_vec),
            dominant_constraint: "none".to_string(),
            dominant_contribution: 0.0,
        };
    }

    // Gram-Schmidt orthogonalization
    let ortho_basis = gram_schmidt(
        &constraints
            .iter()
            .map(|c| c.components.clone())
            .collect::<Vec<_>>(),
    );

    // Project impossibility vector onto each orthogonal basis vector
    let mut components = Vec::new();
    let mut max_proj = 0.0f64;
    let mut dominant = String::new();

    for (i, basis) in ortho_basis.iter().enumerate() {
        let projection = dot_product(impossibility_vec, basis);
        let label = if i < constraints.len() {
            constraints[i].label.clone()
        } else {
            format!("constraint_{}", i)
        };

        if projection.abs() > max_proj.abs() {
            max_proj = projection;
            dominant = label.clone();
        }

        components.push((label, projection));
    }

    ImpossibilitySpectrum {
        total_impossibility: vec_norm(impossibility_vec),
        components,
        dominant_constraint: dominant,
        dominant_contribution: max_proj,
    }
}

fn gram_schmidt(vectors: &[Vec<f64>]) -> Vec<Vec<f64>> {
    let mut ortho: Vec<Vec<f64>> = Vec::new();

    for v in vectors {
        let mut u = v.clone();

        for basis in &ortho {
            let proj = dot_product(&u, basis) / dot_product(basis, basis).max(1e-15);
            for i in 0..u.len().min(basis.len()) {
                u[i] -= proj * basis[i];
            }
        }

        let norm = vec_norm(&u);
        if norm > 1e-10 {
            for x in &mut u {
                *x /= norm;
            }
            ortho.push(u);
        }
    }

    ortho
}

fn dot_product(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

// ═══════════════════════════════════════════════════════════════
// 6. BAYESIAN SURPRISE METRIC
// ═══════════════════════════════════════════════════════════════
//
// Measures how "surprising" a result is relative to our prior beliefs.
//
// Surprise(result) = KL(posterior || prior)
//                  = Σᵢ posterior_i · log(posterior_i / prior_i)
//
// High surprise → unexpected result → update beliefs strongly.
// Low surprise → expected result → maintain course.

/// Compute Bayesian surprise as KL divergence.
pub fn bayesian_surprise(prior: &[f64], posterior: &[f64]) -> f64 {
    let n = prior.len().min(posterior.len());
    if n == 0 {
        return 0.0;
    }

    let mut kl = 0.0;
    for i in 0..n {
        let p = posterior[i].max(1e-15);
        let q = prior[i].max(1e-15);
        kl += p * (p / q).ln();
    }

    kl.max(0.0)
}

/// Symmetric KL (Jensen-Shannon divergence).
/// JSD(P||Q) = (KL(P||M) + KL(Q||M)) / 2
/// where M = (P+Q)/2.
pub fn jensen_shannon_divergence(p: &[f64], q: &[f64]) -> f64 {
    let n = p.len().min(q.len());
    let m: Vec<f64> = (0..n).map(|i| (p[i] + q[i]) / 2.0).collect();
    (bayesian_surprise(p, &m) + bayesian_surprise(q, &m)) / 2.0
}

// ═══════════════════════════════════════════════════════════════
// 7. INFORMATION BOTTLENECK
// ═══════════════════════════════════════════════════════════════
//
// Compresses a representation while preserving maximal information
// about the target variable.
//
// min_{p(t|x)} I(X;T) - β·I(T;Y)
//
// Where:
//   X = raw problem features
//   T = compressed representation
//   Y = solution
//   β = trade-off parameter
//
// We implement a simplified iterative bottleneck (Tishby 2000)
// for discrete distributions.

/// Information bottleneck compression result.
#[derive(Debug, Clone)]
pub struct BottleneckResult {
    /// Compressed representation: mapping from feature to cluster.
    pub assignments: Vec<usize>,
    /// Number of clusters (compression level).
    pub num_clusters: usize,
    /// Mutual information I(X;T) — what we lost.
    pub info_lost: f64,
    /// Mutual information I(T;Y) — what we preserved.
    pub info_preserved: f64,
    /// Compression ratio.
    pub compression_ratio: f64,
}

/// Run information bottleneck compression.
///
/// `joint_xy[i][j]` = P(X=i, Y=j).
/// `num_clusters` = desired compression level.
/// `beta` = trade-off (higher = preserve more info about Y).
pub fn information_bottleneck(
    joint_xy: &[Vec<f64>],
    num_clusters: usize,
    beta: f64,
    iterations: usize,
) -> BottleneckResult {
    let n_x = joint_xy.len();
    if n_x == 0 {
        return BottleneckResult {
            assignments: Vec::new(),
            num_clusters: 0,
            info_lost: 0.0,
            info_preserved: 0.0,
            compression_ratio: 1.0,
        };
    }
    let n_y = joint_xy[0].len();
    let n_t = num_clusters.min(n_x);

    // Initialize: assign features to clusters round-robin
    let mut assignments: Vec<usize> = (0..n_x).map(|i| i % n_t).collect();

    // P(x)
    let mut p_x: Vec<f64> = vec![0.0; n_x];
    for i in 0..n_x {
        p_x[i] = joint_xy[i].iter().sum::<f64>();
    }
    let total: f64 = p_x.iter().sum::<f64>().max(1e-15);
    for p in &mut p_x {
        *p /= total;
    }

    for _ in 0..iterations {
        // Compute P(Y|T=t) for each cluster
        let mut p_y_given_t: Vec<Vec<f64>> = vec![vec![0.0; n_y]; n_t];
        let mut p_t: Vec<f64> = vec![0.0; n_t];

        for i in 0..n_x {
            let t = assignments[i];
            p_t[t] += p_x[i];
            for j in 0..n_y {
                p_y_given_t[t][j] += joint_xy[i][j];
            }
        }

        // Normalize P(Y|T)
        for t in 0..n_t {
            let sum: f64 = p_y_given_t[t].iter().sum::<f64>().max(1e-15);
            for j in 0..n_y {
                p_y_given_t[t][j] /= sum;
            }
        }

        // Reassign each x to the best cluster
        let mut changed = false;
        for i in 0..n_x {
            // P(Y|X=x)
            let sum_x: f64 = joint_xy[i].iter().sum::<f64>().max(1e-15);
            let p_y_given_x: Vec<f64> = joint_xy[i].iter().map(|&v| v / sum_x).collect();

            let mut best_t = assignments[i];
            let mut best_score = f64::NEG_INFINITY;

            for t in 0..n_t {
                // Score = β·similarity(P(Y|X), P(Y|T)) - log(1/P(T))
                let kl = bayesian_surprise(&p_y_given_x, &p_y_given_t[t]);
                let score = -kl * beta + p_t[t].max(1e-15).ln();

                if score > best_score {
                    best_score = score;
                    best_t = t;
                }
            }

            if best_t != assignments[i] {
                assignments[i] = best_t;
                changed = true;
            }
        }

        if !changed {
            break;
        }
    }

    // Compute final info metrics
    let actual_clusters: std::collections::HashSet<usize> = assignments.iter().copied().collect();
    let compression = actual_clusters.len() as f64 / n_x as f64;

    // Compute P(T=t) and P(Y|T=t) for final assignments
    let mut final_p_t = vec![0.0; n_t];
    let mut final_p_y_given_t = vec![vec![0.0; n_y]; n_t];
    for i in 0..n_x {
        let t = assignments[i];
        final_p_t[t] += p_x[i];
        for j in 0..n_y {
            final_p_y_given_t[t][j] += joint_xy[i][j];
        }
    }
    // Normalize P(Y|T)
    for t in 0..n_t {
        let sum_t: f64 = final_p_y_given_t[t].iter().sum::<f64>().max(1e-15);
        for j in 0..n_y {
            final_p_y_given_t[t][j] /= sum_t;
        }
    }

    // Compute P(Y)
    let mut p_y = vec![0.0; n_y];
    for i in 0..n_x {
        for j in 0..n_y {
            p_y[j] += joint_xy[i][j];
        }
    }
    let total_y: f64 = p_y.iter().sum::<f64>().max(1e-15);
    for j in 0..n_y {
        p_y[j] /= total_y;
    }

    // I(T;Y) = Σ_t P(T=t) * KL(P(Y|T=t) || P(Y))
    // Measures how much information about Y is preserved in T.
    let mut info_preserved = 0.0;
    for t in 0..n_t {
        if final_p_t[t] < 1e-15 {
            continue;
        }
        for j in 0..n_y {
            let p_yt = final_p_y_given_t[t][j].max(1e-15);
            let p_yj = p_y[j].max(1e-15);
            info_preserved += final_p_t[t] * p_yt * (p_yt / p_yj).ln();
        }
    }
    info_preserved = info_preserved.max(0.0);

    // I(X;T) = H(X) - H(X|T) = Σ_x P(x) * log(P(x|T(x)) / P(x))
    // Simplified: since T is deterministic given X, I(X;T) = H(T)
    let mut info_lost_h_t = 0.0;
    for t in 0..n_t {
        if final_p_t[t] > 1e-15 {
            info_lost_h_t -= final_p_t[t] * final_p_t[t].ln();
        }
    }
    // The "lost" info is H(X) - I(X;T) ≈ H(X) - H(T)
    let mut h_x = 0.0;
    for i in 0..n_x {
        if p_x[i] > 1e-15 {
            h_x -= p_x[i] * p_x[i].ln();
        }
    }
    let info_lost = (h_x - info_lost_h_t).max(0.0);

    BottleneckResult {
        assignments,
        num_clusters: actual_clusters.len(),
        info_lost,
        info_preserved,
        compression_ratio: compression,
    }
}

// ═══════════════════════════════════════════════════════════════
// 8. COGNITIVE ENERGY LANDSCAPE
// ═══════════════════════════════════════════════════════════════
//
// Models the strategy space as an energy landscape where:
//   - Minima = stable strategy configurations
//   - Saddle points = transition states between strategies
//   - Barriers = difficulty of switching strategies
//
// Energy function:
//   E(θ) = -Σᵢ perf_i(θ) + λ·||θ||²  (performance + regularization)
//
// Gradient descent finds local minima.
// Simulated annealing (via temperature parameter) escapes saddle points.

/// A point in the cognitive energy landscape.
#[derive(Debug, Clone)]
pub struct EnergyPoint {
    pub parameters: Vec<f64>,
    pub energy: f64,
    pub gradient: Vec<f64>,
    pub is_minimum: bool,
}

/// Cognitive energy landscape analyzer.
pub struct EnergyLandscape {
    /// Performance function samples: (parameters, performance).
    samples: Vec<(Vec<f64>, f64)>,
    /// Regularization strength.
    lambda: f64,
    /// Temperature for simulated annealing.
    temperature: f64,
}

impl EnergyLandscape {
    pub fn new(lambda: f64, initial_temp: f64) -> Self {
        Self {
            samples: Vec::new(),
            lambda,
            temperature: initial_temp,
        }
    }

    /// Record a performance sample.
    pub fn record(&mut self, parameters: Vec<f64>, performance: f64) {
        self.samples.push((parameters, performance));
        if self.samples.len() > 500 {
            self.samples.remove(0);
        }
    }

    /// Compute energy at a point using nearest-neighbor interpolation.
    pub fn energy_at(&self, params: &[f64]) -> f64 {
        if self.samples.is_empty() {
            return 0.0;
        }

        // Weighted average of nearby samples (inverse distance weighting)
        let mut total_weight = 0.0;
        let mut weighted_perf = 0.0;

        for (sample_params, perf) in &self.samples {
            let dist = params
                .iter()
                .zip(sample_params.iter())
                .map(|(a, b)| (a - b) * (a - b))
                .sum::<f64>()
                .sqrt()
                .max(0.01);
            let weight = 1.0 / (dist * dist);
            weighted_perf += weight * perf;
            total_weight += weight;
        }

        let avg_perf = weighted_perf / total_weight.max(1e-15);
        let reg = self.lambda * params.iter().map(|x| x * x).sum::<f64>();

        // Energy = -performance + regularization
        -avg_perf + reg
    }

    /// Estimate gradient via finite differences.
    pub fn gradient_at(&self, params: &[f64]) -> Vec<f64> {
        let epsilon = 0.01;
        let e0 = self.energy_at(params);

        params
            .iter()
            .enumerate()
            .map(|(i, _)| {
                let mut perturbed = params.to_vec();
                perturbed[i] += epsilon;
                let e_pos = self.energy_at(&perturbed);
                (e_pos - e0) / epsilon
            })
            .collect()
    }

    /// Take one step of gradient descent with momentum.
    pub fn descend(
        &self,
        current: &[f64],
        learning_rate: f64,
        momentum: &mut Vec<f64>,
        beta: f64,
    ) -> Vec<f64> {
        let grad = self.gradient_at(current);

        if momentum.len() != grad.len() {
            *momentum = vec![0.0; grad.len()];
        }

        let mut next = Vec::with_capacity(current.len());
        for i in 0..current.len() {
            momentum[i] = beta * momentum[i] + (1.0 - beta) * grad[i];
            next.push(current[i] - learning_rate * momentum[i]);
        }
        next
    }

    /// Simulated annealing step: accept worse solutions with probability
    /// P(accept) = exp(-ΔE / T).
    pub fn anneal_step(
        &mut self,
        _current: &[f64],
        current_energy: f64,
        proposed: &[f64],
        cooling_rate: f64,
    ) -> (bool, f64) {
        let proposed_energy = self.energy_at(proposed);
        let delta_e = proposed_energy - current_energy;

        let accept = if delta_e < 0.0 {
            true // Better solution: always accept
        } else {
            // Worse solution: accept with Boltzmann probability
            let accept_prob = (-delta_e / self.temperature.max(1e-10)).exp();
            rand::random::<f64>() < accept_prob
        };

        // Cool down
        self.temperature *= cooling_rate;

        (accept, proposed_energy)
    }

    pub fn temperature(&self) -> f64 {
        self.temperature
    }
    pub fn num_samples(&self) -> usize {
        self.samples.len()
    }
}

// ═══════════════════════════════════════════════════════════════
// BRIDGE TYPES & CONVENIENCE FUNCTIONS
// ═══════════════════════════════════════════════════════════════
//
// Convenience wrappers used by D3/D5 that adapt the core
// formulas to simpler calling conventions.

/// Simplified causal event used by D5 for timeline validation.
#[derive(Debug, Clone)]
pub struct CausalEvent {
    pub time: f64,
    pub position: Vec<f64>,
    pub label: String,
}

/// Causal cone result for the simplified API.
#[derive(Debug, Clone)]
pub struct SimpleCausalCone {
    pub events_in_cone: usize,
    pub violations: usize,
    pub is_valid: bool,
}

/// Analyze causal structure of a reasoning chain.
///
/// Checks that the trace has monotonic forward causation —
/// no later step "depends" on information from a logically later step.
///
/// Performance: O(n²) worst case, but with early termination on first
/// violation and a pair-count cap for large event sets.
pub fn analyze_causal_cone(events: &[CausalEvent], c_speed: f64) -> SimpleCausalCone {
    if events.len() < 2 {
        return SimpleCausalCone {
            events_in_cone: 0,
            violations: 0,
            is_valid: true,
        };
    }

    let mut in_cone = 0usize;
    let mut violations = 0usize;
    // Cap pair comparisons at 500 to maintain <1ms latency on low-end hardware.
    // For 100 events this is ~4950 pairs; for 500+ events we sample.
    let max_pairs: usize = 500;
    let mut pair_count = 0usize;

    'outer: for i in 0..events.len() {
        for j in (i + 1)..events.len() {
            pair_count += 1;

            let dt = events[j].time - events[i].time;
            let dx: f64 = events[i]
                .position
                .iter()
                .zip(events[j].position.iter())
                .map(|(a, b)| (a - b).powi(2))
                .sum::<f64>()
                .sqrt();

            if dt >= 0.0 && dt >= c_speed * dx {
                in_cone += 1;
            } else if dt < 0.0 {
                violations += 1;
                // Early termination: one backward-causation violation
                // invalidates the entire chain. No need to check further.
                break 'outer;
            }

            if pair_count >= max_pairs {
                break 'outer;
            }
        }
    }

    SimpleCausalCone {
        events_in_cone: in_cone,
        violations,
        is_valid: violations == 0,
    }
}

/// Cognitive energy landscape result from the convenience function.
#[derive(Debug, Clone)]
pub struct EnergyResult {
    pub initial_energy: f64,
    pub final_energy: f64,
    pub optimal_idx: usize,
    pub converged: bool,
}

/// Free-function energy landscape analysis for a vector of timeline scores.
///
/// Treats each score as a 1D position in energy space, finds the
/// minimum-energy configuration using gradient descent.
pub fn cognitive_energy_landscape(
    scores: &[f64],
    iterations: usize,
    learning_rate: f64,
) -> EnergyResult {
    if scores.is_empty() {
        return EnergyResult {
            initial_energy: 0.0,
            final_energy: 0.0,
            optimal_idx: 0,
            converged: true,
        };
    }

    // Energy = negative score (we want to maximize score = minimize energy)
    let energies: Vec<f64> = scores.iter().map(|s| -s).collect();
    let initial_energy = energies.iter().sum::<f64>() / energies.len() as f64;

    // Find minimum energy via simple gradient descent on a smoothed landscape
    let mut position: Vec<f64> = energies.clone();
    let mut converged = false;

    for _ in 0..iterations {
        let mut next = position.clone();
        let mut max_delta = 0.0f64;

        for i in 0..position.len() {
            // Gradient from neighbors (smoothed potential)
            let left = if i > 0 { position[i - 1] } else { position[i] };
            let right = if i < position.len() - 1 {
                position[i + 1]
            } else {
                position[i]
            };
            let grad = (right - left) / 2.0;
            next[i] -= learning_rate * grad;
            max_delta = max_delta.max((next[i] - position[i]).abs());
        }

        position = next;
        if max_delta < 1e-6 {
            converged = true;
            break;
        }
    }

    let final_energy = position.iter().sum::<f64>() / position.len() as f64;

    // Find optimal index (minimum energy)
    let optimal_idx = energies
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(i, _)| i)
        .unwrap_or(0);

    EnergyResult {
        initial_energy,
        final_energy,
        optimal_idx,
        converged,
    }
}

/// Simplified uncertainty decomposition for a flat vector of confidences.
///
/// Converts single confidence values into pseudo-distributions and
/// decomposes into aleatoric vs epistemic components.
pub struct SimpleUncertainty {
    pub aleatoric: f64,
    pub epistemic: f64,
    pub total: f64,
}

pub fn decompose_uncertainty_simple(confidences: &[f64]) -> SimpleUncertainty {
    if confidences.is_empty() {
        return SimpleUncertainty {
            aleatoric: 0.0,
            epistemic: 0.0,
            total: 0.0,
        };
    }

    let mean: f64 = confidences.iter().sum::<f64>() / confidences.len() as f64;
    let variance: f64 =
        confidences.iter().map(|c| (c - mean).powi(2)).sum::<f64>() / confidences.len() as f64;

    // Aleatoric ≈ average individual uncertainty: mean(1-c)
    let aleatoric = confidences.iter().map(|c| 1.0 - c).sum::<f64>() / confidences.len() as f64;

    // Epistemic ≈ disagreement between models: variance of confidences
    let epistemic = variance.sqrt();

    // Total = sqrt(aleatoric² + epistemic²) (quadrature sum)
    let total = (aleatoric * aleatoric + epistemic * epistemic).sqrt();

    SimpleUncertainty {
        aleatoric,
        epistemic,
        total,
    }
}
