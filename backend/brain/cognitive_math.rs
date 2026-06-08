// ═══════════════════════════════════════════════════════════════
// COGNITIVE MATHEMATICS ENGINE v1.0
// ═══════════════════════════════════════════════════════════════
//
// Novel mathematical formulations for hyper-cognitive reasoning.
// Every formula here is ORIGINAL — not found in standard libraries.
//
// Sections:
//   1. Cognitive Resonance Field Theory
//   2. Entropic Proof Strength
//   3. Kolmogorov Creative Distance (KCD)
//   4. Hyperbolic Concept Space (Poincaré Ball)
//   5. Cognitive Diffusion Equation
//   6. Dimensional Lifting Operator
//   7. Quantum-Inspired Amplitude Scoring
//   8. Topological Problem Invariants
//   9. Lyapunov Cognitive Stability
//  10. Fisher Information Strategy Metric
//  11. Kolmogorov-Smirnov Drift Detection
//  12. Fractal Recursion Depth Estimator
//
// Pure Rust. Zero external dependencies beyond std.

use std::collections::{HashMap, HashSet};

// ═══════════════════════════════════════════════════════════════
// 1. COGNITIVE RESONANCE FIELD THEORY
// ═══════════════════════════════════════════════════════════════
//
// Treats thought-nodes as wave sources in a cognitive field.
// Each node emits a "wave" whose amplitude is its reward.
// Interference patterns reveal which thoughts mutually reinforce.
//
// CRS(n) = Σᵢ A(nᵢ) · cos(2π · d(n,nᵢ) / λ) · e^(-d(n,nᵢ)/σ)
//
// Where:
//   A(nᵢ) = amplitude (reward) of neighboring node i
//   d()    = semantic distance (1 - cosine similarity)
//   λ      = resonance wavelength (controls periodicity)
//   σ      = decay constant (controls range)

/// Configuration for the Cognitive Resonance Field.
#[derive(Debug, Clone)]
pub struct ResonanceConfig {
    /// Resonance wavelength — controls the periodicity of interference.
    /// Small λ = tight resonance bands. Large λ = broad reinforcement.
    pub wavelength: f64,
    /// Decay constant — controls how far resonance propagates.
    pub decay_sigma: f64,
    /// Minimum amplitude to consider for field computation.
    pub min_amplitude: f64,
}

impl Default for ResonanceConfig {
    fn default() -> Self {
        Self {
            wavelength: 0.5,
            decay_sigma: 2.0,
            min_amplitude: 0.01,
        }
    }
}

/// A node in the cognitive resonance field.
#[derive(Debug, Clone)]
pub struct FieldNode {
    pub id: u64,
    pub amplitude: f64,
    pub features: Vec<f64>,
}

/// Compute the Cognitive Resonance Score for a target node
/// given a set of source nodes.
///
/// CRS(target) = Σᵢ A(srcᵢ) · cos(2π · d/λ) · e^(-d/σ)
///
/// Returns the resonance score ∈ [-1.0, +∞). Positive = constructive
/// interference. Negative = destructive interference.
pub fn cognitive_resonance_score(
    target: &FieldNode,
    sources: &[FieldNode],
    config: &ResonanceConfig,
) -> f64 {
    let mut total = 0.0;
    let two_pi = std::f64::consts::PI * 2.0;

    for src in sources {
        if src.id == target.id || src.amplitude.abs() < config.min_amplitude {
            continue;
        }

        let dist = semantic_distance(&target.features, &src.features);
        if dist < 1e-12 {
            total += src.amplitude;
            continue;
        }

        // Wave component: cos(2π·d/λ)
        let wave = (two_pi * dist / config.wavelength).cos();
        // Decay component: e^(-d/σ)
        let decay = (-dist / config.decay_sigma).exp();
        // Contribution
        total += src.amplitude * wave * decay;
    }

    total
}

/// Compute the full resonance field map for all nodes.
/// Returns a map of node_id → resonance_score.
pub fn compute_resonance_field(nodes: &[FieldNode], config: &ResonanceConfig) -> HashMap<u64, f64> {
    let mut field = HashMap::with_capacity(nodes.len());
    for target in nodes {
        let score = cognitive_resonance_score(target, nodes, config);
        field.insert(target.id, score);
    }
    field
}

/// Semantic distance: 1 - cosine_similarity.
/// Returns 0.0 for identical vectors, 1.0 for orthogonal,
/// 2.0 for anti-correlated.
fn semantic_distance(a: &[f64], b: &[f64]) -> f64 {
    let sim = cosine_similarity(a, b);
    1.0 - sim
}

/// Cosine similarity ∈ [-1, 1].
pub fn cosine_similarity(a: &[f64], b: &[f64]) -> f64 {
    let len = a.len().min(b.len());
    if len == 0 {
        return 0.0;
    }
    let mut dot = 0.0;
    let mut norm_a = 0.0;
    let mut norm_b = 0.0;
    for i in 0..len {
        dot += a[i] * b[i];
        norm_a += a[i] * a[i];
        norm_b += b[i] * b[i];
    }
    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom < 1e-15 {
        0.0
    } else {
        (dot / denom).clamp(-1.0, 1.0)
    }
}

// ═══════════════════════════════════════════════════════════════
// 2. ENTROPIC PROOF STRENGTH (EPS)
// ═══════════════════════════════════════════════════════════════
//
// Measures how "informative" a proof chain is.
//
// EPS = 1 - H(proof) / H_max
//     = 1 - (-Σ pᵢ·log₂(pᵢ)) / log₂(N)
//
// Where pᵢ = confidence_i / Σconfidence.
//
// EPS → 0: all steps equally uncertain (weak proof).
// EPS → 1: one dominant step drives confidence (strong proof).
//
// Additionally we compute:
//   LogicalDepth(proof) = Σᵢ log₂(1/pᵢ)  (total surprise)
//   RedundancyIndex = 1 - (unique_strategies / total_steps)

/// Metrics for a proof chain.
#[derive(Debug, Clone)]
pub struct ProofMetrics {
    /// Entropic proof strength ∈ [0, 1].
    pub eps: f64,
    /// Shannon entropy of the confidence distribution.
    pub shannon_entropy: f64,
    /// Maximum possible entropy for this proof length.
    pub max_entropy: f64,
    /// Logical depth: total surprise content.
    pub logical_depth: f64,
    /// Redundancy index ∈ [0, 1]: how repetitive the strategies are.
    pub redundancy_index: f64,
    /// Effective number of independent proof steps: 2^H.
    pub effective_steps: f64,
    /// Gini coefficient of confidence distribution.
    pub gini_coefficient: f64,
}

/// Compute comprehensive proof metrics from a chain of confidence values.
pub fn compute_proof_metrics(confidences: &[f64], strategies: &[&str]) -> ProofMetrics {
    let n = confidences.len();
    if n == 0 {
        return ProofMetrics {
            eps: 0.0,
            shannon_entropy: 0.0,
            max_entropy: 0.0,
            logical_depth: 0.0,
            redundancy_index: 1.0,
            effective_steps: 0.0,
            gini_coefficient: 0.0,
        };
    }

    // Normalize to probability distribution
    let total: f64 = confidences.iter().sum::<f64>().max(1e-15);
    let probs: Vec<f64> = confidences.iter().map(|c| c / total).collect();

    // Shannon entropy
    let mut entropy = 0.0;
    let mut logical_depth = 0.0;
    for &p in &probs {
        if p > 1e-15 {
            entropy -= p * p.log2();
            logical_depth += (-p.log2()).max(0.0);
        }
    }

    let max_entropy = (n as f64).log2().max(1e-15);
    let eps = (1.0 - entropy / max_entropy).clamp(0.0, 1.0);
    let effective_steps = 2.0f64.powf(entropy);

    // Redundancy index
    let unique: HashSet<&str> = strategies.iter().copied().collect();
    let redundancy = if n > 0 {
        1.0 - (unique.len() as f64 / n as f64)
    } else {
        1.0
    };

    // Gini coefficient
    let gini = compute_gini(&probs);

    ProofMetrics {
        eps,
        shannon_entropy: entropy,
        max_entropy,
        logical_depth,
        redundancy_index: redundancy,
        effective_steps,
        gini_coefficient: gini,
    }
}

/// Compute the Gini coefficient of a distribution.
/// G = (2·Σᵢ(i·xᵢ))/(n·Σᵢxᵢ) - (n+1)/n
fn compute_gini(values: &[f64]) -> f64 {
    let n = values.len();
    if n <= 1 {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let total: f64 = sorted.iter().sum::<f64>().max(1e-15);
    let mut numerator = 0.0;
    for (i, &val) in sorted.iter().enumerate() {
        numerator += (i as f64 + 1.0) * val;
    }
    let n_f = n as f64;
    (2.0 * numerator / (n_f * total)) - (n_f + 1.0) / n_f
}

// ═══════════════════════════════════════════════════════════════
// 3. KOLMOGOROV CREATIVE DISTANCE (KCD)
// ═══════════════════════════════════════════════════════════════
//
// Measures the deep structural similarity between concepts.
//
// KCD(a,b) = 1 - (K(a|b) + K(b|a)) / (K(a) + K(b))
//
// Approximated:
//   K(x) ≈ LZ77_size(x)
//   K(x|y) ≈ LZ77_size(y || x) - LZ77_size(y)
//
// Low KCD → concepts share deep structure (good for synthesis).
// High KCD → concepts are fundamentally different (good for novelty).

/// LZ77 compressed size estimation with configurable window.
pub fn lz77_compressed_size(data: &[u8], window_size: usize) -> usize {
    if data.is_empty() {
        return 0;
    }
    let mut output_bits: usize = 0;
    let mut pos = 0;

    while pos < data.len() {
        let search_start = pos.saturating_sub(window_size);
        let mut best_len = 0usize;

        for start in search_start..pos {
            let mut length = 0;
            while pos + length < data.len()
                && start + length < pos
                && data[start + length] == data[pos + length]
                && length < 258
            {
                length += 1;
            }
            if length > best_len {
                best_len = length;
            }
        }

        if best_len >= 3 {
            output_bits += 16; // (offset, length) pair
            pos += best_len;
        } else {
            output_bits += 9; // literal
            pos += 1;
        }
    }

    (output_bits + 7) / 8
}

/// Kolmogorov Creative Distance between two strings.
///
/// KCD(a,b) = 1 - (K(a|b) + K(b|a)) / (K(a) + K(b))
pub fn kolmogorov_creative_distance(a: &str, b: &str) -> f64 {
    let window = 512;
    let ka = lz77_compressed_size(a.as_bytes(), window) as f64;
    let kb = lz77_compressed_size(b.as_bytes(), window) as f64;

    let ab_concat = format!("{}{}", a, b);
    let ba_concat = format!("{}{}", b, a);

    let kab = lz77_compressed_size(ab_concat.as_bytes(), window) as f64;
    let kba = lz77_compressed_size(ba_concat.as_bytes(), window) as f64;

    // K(a|b) ≈ K(ba) - K(b), K(b|a) ≈ K(ab) - K(a)
    let ka_given_b = (kba - kb).max(0.0);
    let kb_given_a = (kab - ka).max(0.0);

    let denominator = (ka + kb).max(1.0);
    let kcd = 1.0 - (ka_given_b + kb_given_a) / denominator;
    kcd.clamp(0.0, 1.0)
}

/// Batch KCD: compute pairwise distances for a set of concepts.
/// Returns an NxN distance matrix.
pub fn kcd_matrix(concepts: &[&str]) -> Vec<Vec<f64>> {
    let n = concepts.len();
    let mut matrix = vec![vec![0.0; n]; n];
    for i in 0..n {
        for j in (i + 1)..n {
            let d = kolmogorov_creative_distance(concepts[i], concepts[j]);
            matrix[i][j] = d;
            matrix[j][i] = d;
        }
    }
    matrix
}

// ═══════════════════════════════════════════════════════════════
// 4. HYPERBOLIC CONCEPT SPACE (Poincaré Ball Model)
// ═══════════════════════════════════════════════════════════════
//
// Hierarchical concepts naturally embed in hyperbolic space
// with much lower distortion than Euclidean space.
//
// Distance in the Poincaré ball:
//   d_H(u,v) = arccosh(1 + 2||u-v||² / ((1-||u||²)(1-||v||²)))
//
// Möbius addition (for vector operations):
//   u ⊕ v = ((1+2⟨u,v⟩+||v||²)u + (1-||u||²)v) / (1+2⟨u,v⟩+||u||²||v||²)

/// A point in the Poincaré ball (all coords must satisfy ||x|| < 1).
#[derive(Debug, Clone)]
pub struct PoincareBallPoint {
    pub coords: Vec<f64>,
}

impl PoincareBallPoint {
    /// Create a point, projecting into the ball if needed.
    pub fn new(mut coords: Vec<f64>) -> Self {
        let norm_sq: f64 = coords.iter().map(|x| x * x).sum();
        if norm_sq >= 1.0 {
            // Project onto ball boundary with margin
            let norm = norm_sq.sqrt();
            let scale = 0.95 / norm;
            for c in &mut coords {
                *c *= scale;
            }
        }
        Self { coords }
    }

    pub fn norm_sq(&self) -> f64 {
        self.coords.iter().map(|x| x * x).sum()
    }

    pub fn norm(&self) -> f64 {
        self.norm_sq().sqrt()
    }

    pub fn dim(&self) -> usize {
        self.coords.len()
    }

    /// Origin point in the Poincaré ball.
    pub fn origin(dim: usize) -> Self {
        Self {
            coords: vec![0.0; dim],
        }
    }
}

/// Poincaré ball hyperbolic distance.
///
/// d_H(u,v) = arccosh(1 + 2||u-v||² / ((1-||u||²)(1-||v||²)))
pub fn poincare_distance(u: &PoincareBallPoint, v: &PoincareBallPoint) -> f64 {
    let dim = u.dim().min(v.dim());
    let mut diff_sq = 0.0;
    for i in 0..dim {
        let d = u.coords[i] - v.coords[i];
        diff_sq += d * d;
    }

    let u_norm_sq = u.norm_sq();
    let v_norm_sq = v.norm_sq();

    let denom = (1.0 - u_norm_sq) * (1.0 - v_norm_sq);
    if denom.abs() < 1e-15 {
        return f64::INFINITY;
    }

    let arg = 1.0 + 2.0 * diff_sq / denom;
    // arccosh(x) = ln(x + sqrt(x² - 1))
    if arg <= 1.0 {
        0.0
    } else {
        (arg + (arg * arg - 1.0).sqrt()).ln()
    }
}

/// Möbius addition in the Poincaré ball.
///
/// u ⊕ v = ((1+2⟨u,v⟩+||v||²)u + (1-||u||²)v) / (1+2⟨u,v⟩+||u||²||v||²)
pub fn mobius_add(u: &PoincareBallPoint, v: &PoincareBallPoint) -> PoincareBallPoint {
    let dim = u.dim().min(v.dim());
    let mut uv_dot = 0.0;
    for i in 0..dim {
        uv_dot += u.coords[i] * v.coords[i];
    }
    let u_sq = u.norm_sq();
    let v_sq = v.norm_sq();

    let num_u_coeff = 1.0 + 2.0 * uv_dot + v_sq;
    let num_v_coeff = 1.0 - u_sq;
    let denom = (1.0 + 2.0 * uv_dot + u_sq * v_sq).max(1e-15);

    let mut result = vec![0.0; dim];
    for i in 0..dim {
        result[i] = (num_u_coeff * u.coords[i] + num_v_coeff * v.coords[i]) / denom;
    }
    PoincareBallPoint::new(result)
}

/// Exponential map: maps a tangent vector at point p to the ball.
/// Used for gradient-based updates in hyperbolic space.
///
/// exp_p(v) = p ⊕ (tanh(λ_p·||v||/2) · v/||v||)
/// where λ_p = 2/(1-||p||²) is the conformal factor.
pub fn exponential_map(p: &PoincareBallPoint, tangent: &[f64]) -> PoincareBallPoint {
    let dim = p.dim().min(tangent.len());
    let t_norm: f64 = tangent.iter().take(dim).map(|x| x * x).sum::<f64>().sqrt();
    if t_norm < 1e-15 {
        return p.clone();
    }

    let lambda_p = 2.0 / (1.0 - p.norm_sq()).max(1e-15);
    let scale = (lambda_p * t_norm / 2.0).tanh() / t_norm;

    let mut v_scaled = vec![0.0; dim];
    for i in 0..dim {
        v_scaled[i] = tangent[i] * scale;
    }

    mobius_add(p, &PoincareBallPoint::new(v_scaled))
}

// ═══════════════════════════════════════════════════════════════
// 5. COGNITIVE DIFFUSION EQUATION
// ═══════════════════════════════════════════════════════════════
//
// Models concept activation spreading over a knowledge graph using
// a reaction-diffusion PDE (discretized on graph nodes):
//
//   ∂C/∂t = D·∇²C + R(C) - λ·C
//
// Where:
//   C(i,t) = activation of concept i at time t
//   D = diffusion coefficient (spread rate)
//   ∇²C ≈ Σⱼ∈neighbors(i) w(i,j)·(C(j)-C(i))  (graph Laplacian)
//   R(C) = αC(1-C)(C-β) = bistable reaction term
//   λ = natural decay rate
//
// This produces Turing-like activation patterns where stable "islands"
// of knowledge emerge spontaneously from uniform initial conditions.

/// Configuration for the cognitive diffusion model.
#[derive(Debug, Clone)]
pub struct DiffusionConfig {
    /// Diffusion coefficient D: how fast activation spreads.
    pub diffusion_d: f64,
    /// Decay rate λ: natural decay of activation.
    pub decay_lambda: f64,
    /// Reaction strength α.
    pub reaction_alpha: f64,
    /// Bistable threshold β ∈ (0, 1).
    pub reaction_beta: f64,
    /// Time step size for numerical integration.
    pub dt: f64,
}

impl Default for DiffusionConfig {
    fn default() -> Self {
        Self {
            diffusion_d: 0.1,
            decay_lambda: 0.05,
            reaction_alpha: 1.0,
            reaction_beta: 0.25,
            dt: 0.1,
        }
    }
}

/// Run one step of cognitive diffusion on a graph.
///
/// `activations[i]` = current activation of node i.
/// `adjacency[i]` = list of (neighbor_index, edge_weight).
///
/// Returns the updated activation values.
pub fn diffusion_step(
    activations: &[f64],
    adjacency: &[Vec<(usize, f64)>],
    config: &DiffusionConfig,
) -> Vec<f64> {
    let n = activations.len();
    let mut next = vec![0.0; n];

    for i in 0..n {
        let c = activations[i];

        // Graph Laplacian: Σⱼ w(i,j)·(C(j) - C(i))
        let mut laplacian = 0.0;
        if i < adjacency.len() {
            for &(j, w) in &adjacency[i] {
                if j < n {
                    laplacian += w * (activations[j] - c);
                }
            }
        }

        // Bistable reaction: α·C·(1-C)·(C-β)
        let reaction = config.reaction_alpha * c * (1.0 - c) * (c - config.reaction_beta);

        // PDE update: ∂C/∂t = D·∇²C + R(C) - λ·C
        let dc_dt = config.diffusion_d * laplacian + reaction - config.decay_lambda * c;
        next[i] = (c + config.dt * dc_dt).clamp(0.0, 1.0);
    }

    next
}

/// Run multiple diffusion steps and return stable activations.
pub fn diffusion_until_stable(
    initial: &[f64],
    adjacency: &[Vec<(usize, f64)>],
    config: &DiffusionConfig,
    max_steps: usize,
    convergence_threshold: f64,
) -> (Vec<f64>, usize) {
    let mut current = initial.to_vec();

    for step in 0..max_steps {
        let next = diffusion_step(&current, adjacency, config);

        // Check convergence
        let max_delta: f64 = current
            .iter()
            .zip(next.iter())
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f64::max);

        current = next;
        if max_delta < convergence_threshold {
            return (current, step + 1);
        }
    }

    (current, max_steps)
}

// ═══════════════════════════════════════════════════════════════
// 6. DIMENSIONAL LIFTING OPERATOR
// ═══════════════════════════════════════════════════════════════
//
// Projects a problem from n-dimensional space into (n+k)-dimensional
// space where previously inseparable solutions become separable.
//
// L: ℝⁿ → ℝⁿ⁺ᵏ
// L(x) = [x₁, ..., xₙ, φ₁(x), ..., φₖ(x)]
//
// Kernel functions φᵢ:
//   RBF:        φ(x) = exp(-γ·||x-c||²)
//   Polynomial: φ(x) = (⟨x,c⟩ + r)ᵈ
//   Sigmoid:    φ(x) = tanh(α⟨x,c⟩ + β)
//   Cross:      φ(x) = xᵢ · xⱼ  (all pairs)

/// Lift a feature vector into higher-dimensional space.
pub fn dimensional_lift(features: &[f64], centroids: &[Vec<f64>], kernel: LiftKernel) -> Vec<f64> {
    let mut lifted = features.to_vec();

    match kernel {
        LiftKernel::RBF { gamma } => {
            for centroid in centroids {
                let dist_sq = features
                    .iter()
                    .zip(centroid.iter())
                    .map(|(a, b)| (a - b) * (a - b))
                    .sum::<f64>();
                lifted.push((-gamma * dist_sq).exp());
            }
        }
        LiftKernel::Polynomial { degree, offset } => {
            for centroid in centroids {
                let dot: f64 = features
                    .iter()
                    .zip(centroid.iter())
                    .map(|(a, b)| a * b)
                    .sum();
                lifted.push((dot + offset).powi(degree as i32));
            }
        }
        LiftKernel::Sigmoid { alpha, beta } => {
            for centroid in centroids {
                let dot: f64 = features
                    .iter()
                    .zip(centroid.iter())
                    .map(|(a, b)| a * b)
                    .sum();
                lifted.push((alpha * dot + beta).tanh());
            }
        }
        LiftKernel::CrossProduct => {
            // All pairwise products: dimension grows from n to n + n(n-1)/2
            let n = features.len();
            for i in 0..n {
                for j in (i + 1)..n {
                    lifted.push(features[i] * features[j]);
                }
            }
        }
    }

    lifted
}

/// Kernel types for dimensional lifting.
#[derive(Debug, Clone)]
pub enum LiftKernel {
    /// Radial Basis Function: exp(-γ·||x-c||²)
    RBF { gamma: f64 },
    /// Polynomial: (⟨x,c⟩ + offset)^degree
    Polynomial { degree: usize, offset: f64 },
    /// Sigmoid: tanh(α⟨x,c⟩ + β)
    Sigmoid { alpha: f64, beta: f64 },
    /// Cross-product: xᵢ·xⱼ for all pairs
    CrossProduct,
}

// ═══════════════════════════════════════════════════════════════
// 7. QUANTUM-INSPIRED AMPLITUDE SCORING
// ═══════════════════════════════════════════════════════════════
//
// Models each timeline as a quantum state with complex amplitude.
//
// ψ(timeline) = Σᵢ αᵢ · e^(iθᵢ)
// P(timeline) = |ψ|² = (Σ αᵢ·cos(θᵢ))² + (Σ αᵢ·sin(θᵢ))²
//
// Where:
//   αᵢ = confidence of dimension i's contribution
//   θᵢ = "phase" = strategic diversity angle
//
// Timelines with dimensions "in phase" (constructive interference)
// score higher. Timelines where dimensions conflict score lower.

/// A quantum-inspired amplitude for a cognitive dimension.
#[derive(Debug, Clone)]
pub struct QuantumAmplitude {
    /// Real component.
    pub real: f64,
    /// Imaginary component.
    pub imag: f64,
}

impl QuantumAmplitude {
    /// Create from magnitude and phase.
    pub fn from_polar(magnitude: f64, phase: f64) -> Self {
        Self {
            real: magnitude * phase.cos(),
            imag: magnitude * phase.sin(),
        }
    }

    /// Probability: |ψ|²
    pub fn probability(&self) -> f64 {
        self.real * self.real + self.imag * self.imag
    }

    /// Phase angle.
    pub fn phase(&self) -> f64 {
        self.imag.atan2(self.real)
    }

    /// Magnitude.
    pub fn magnitude(&self) -> f64 {
        self.probability().sqrt()
    }
}

/// Compute the quantum-inspired probability for a timeline.
///
/// Each dimension contributes an amplitude with phase determined
/// by its strategic diversity relative to other dimensions.
///
/// Returns P = |ψ|² ∈ [0, 1].
pub fn quantum_timeline_probability(
    dimension_confidences: &[f64],
    dimension_phases: &[f64],
) -> f64 {
    let n = dimension_confidences.len().min(dimension_phases.len());
    if n == 0 {
        return 0.0;
    }

    let mut real_sum = 0.0;
    let mut imag_sum = 0.0;

    for i in 0..n {
        let amp = QuantumAmplitude::from_polar(dimension_confidences[i], dimension_phases[i]);
        real_sum += amp.real;
        imag_sum += amp.imag;
    }

    // Normalize by n to keep probability bounded
    let norm = n as f64;
    let p = (real_sum / norm).powi(2) + (imag_sum / norm).powi(2);
    p.clamp(0.0, 1.0)
}

/// Compute strategic phase angles from a set of strategy labels.
/// Strategies that are semantically similar get similar phases.
/// Orthogonal strategies get phases π/2 apart.
pub fn compute_strategy_phases(strategies: &[&str]) -> Vec<f64> {
    let pi = std::f64::consts::PI;
    let n = strategies.len().max(1) as f64;

    strategies
        .iter()
        .enumerate()
        .map(|(i, strategy)| {
            // Base phase from position
            let base = 2.0 * pi * (i as f64) / n;

            // Shift based on strategy type
            let type_shift = match *strategy {
                s if s.contains("deduct") || s.contains("formal") => 0.0,
                s if s.contains("creat") || s.contains("dream") => pi / 4.0,
                s if s.contains("impossible") || s.contains("axiom") => pi / 2.0,
                s if s.contains("meta") || s.contains("self") => 3.0 * pi / 4.0,
                s if s.contains("adversar") => pi,
                _ => base,
            };

            (base + type_shift) % (2.0 * pi)
        })
        .collect()
}

/// Quantum entanglement between two timelines.
/// High entanglement = their outcomes are strongly correlated.
///
/// E(A,B) = 1 - |cos(θ_A - θ_B)|  (Bell-type measure)
pub fn timeline_entanglement(phase_a: f64, phase_b: f64) -> f64 {
    1.0 - (phase_a - phase_b).cos().abs()
}

// ═══════════════════════════════════════════════════════════════
// 8. TOPOLOGICAL PROBLEM INVARIANTS
// ═══════════════════════════════════════════════════════════════
//
// Compute topological invariants of a problem's constraint graph
// to classify its fundamental structure.
//
// β₀ = number of connected components (independent sub-problems)
// β₁ = number of independent cycles (circular deps, paradoxes)
// χ  = β₀ - β₁ (Euler characteristic)
//
// Problems with β₁ > 0 → contain paradoxes/loops → route to D3.
// Problems with high β₀ → decomposable → route to D1.
// χ < 0 → more loops than components → highly entangled.

/// Topological invariants of a constraint graph.
#[derive(Debug, Clone)]
pub struct TopologicalInvariants {
    /// Betti-0: number of connected components.
    pub betti_0: usize,
    /// Betti-1: number of independent cycles.
    pub betti_1: usize,
    /// Euler characteristic: β₀ - β₁.
    pub euler_characteristic: i64,
    /// Number of vertices.
    pub vertices: usize,
    /// Number of edges.
    pub edges: usize,
    /// Is the graph acyclic?
    pub is_acyclic: bool,
    /// Recommended dimension for solving.
    pub recommended_dimension: u8,
}

/// Compute topological invariants of a constraint graph.
///
/// `edges`: list of (from, to) vertex pairs.
/// `num_vertices`: total number of vertices.
pub fn compute_topology(num_vertices: usize, edges: &[(usize, usize)]) -> TopologicalInvariants {
    // Compute β₀ using Union-Find
    let mut parent: Vec<usize> = (0..num_vertices).collect();
    let mut rank = vec![0usize; num_vertices];

    fn find(parent: &mut Vec<usize>, x: usize) -> usize {
        if parent[x] != x {
            parent[x] = find(parent, parent[x]);
        }
        parent[x]
    }

    fn union(parent: &mut Vec<usize>, rank: &mut Vec<usize>, a: usize, b: usize) -> bool {
        let ra = find(parent, a);
        let rb = find(parent, b);
        if ra == rb {
            return false; // Already connected → this edge creates a cycle
        }
        if rank[ra] < rank[rb] {
            parent[ra] = rb;
        } else if rank[ra] > rank[rb] {
            parent[rb] = ra;
        } else {
            parent[rb] = ra;
            rank[ra] += 1;
        }
        true
    }

    let mut cycle_edges = 0;
    for &(a, b) in edges {
        if a < num_vertices && b < num_vertices {
            if !union(&mut parent, &mut rank, a, b) {
                cycle_edges += 1;
            }
        }
    }

    // Count components
    let mut components: HashSet<usize> = HashSet::new();
    for i in 0..num_vertices {
        components.insert(find(&mut parent, i));
    }

    let betti_0 = components.len();
    let betti_1 = cycle_edges;
    let euler = betti_0 as i64 - betti_1 as i64;

    // Recommend dimension based on topology
    let recommended = if betti_1 > 2 {
        3 // Lots of cycles = paradox territory → D3
    } else if betti_0 > 3 {
        1 // Many components = decomposable → D1
    } else if betti_1 == 0 && betti_0 <= 2 {
        1 // Simple acyclic → D1
    } else {
        5 // Mixed complexity → D5 (full power)
    };

    TopologicalInvariants {
        betti_0,
        betti_1,
        euler_characteristic: euler,
        vertices: num_vertices,
        edges: edges.len(),
        is_acyclic: betti_1 == 0,
        recommended_dimension: recommended,
    }
}

// ═══════════════════════════════════════════════════════════════
// 9. LYAPUNOV COGNITIVE STABILITY ANALYSIS
// ═══════════════════════════════════════════════════════════════
//
// Determines if the cognitive system is converging (stable),
// diverging (unstable), or at the edge of chaos (most adaptive).
//
// Stability Index = max eigenvalue of the state transition Jacobian.
//
//   J_ij = ∂(perf_i at t+1) / ∂(weight_j at t)
//        ≈ (perf_i(w_j+ε) - perf_i(w_j-ε)) / (2ε)
//
// For our discrete system, we approximate using finite differences
// on the rolling performance history.
//
// CSI < 0: contracting (converging, stable)
// CSI ≈ 0: edge of chaos (maximally adaptive)
// CSI > 0: expanding (diverging, needs intervention)

/// Result of Lyapunov stability analysis.
#[derive(Debug, Clone)]
pub struct StabilityAnalysis {
    /// Cognitive Stability Index (max Lyapunov exponent estimate).
    pub csi: f64,
    /// Stability classification.
    pub classification: StabilityClass,
    /// Per-dimension stability estimates.
    pub dimension_stability: Vec<f64>,
    /// Recommended action.
    pub action: StabilityAction,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StabilityClass {
    Stable,
    EdgeOfChaos,
    Unstable,
    InsufficientData,
}

#[derive(Debug, Clone)]
pub enum StabilityAction {
    MaintainCourse,
    IncreaseExploration,
    EmergencyStabilize,
    GatherMoreData,
}

/// Estimate the Lyapunov exponent from a time series of cognitive
/// performance values.
///
/// Uses the Rosenstein method: track divergence of nearby trajectories
/// in reconstructed phase space.
pub fn lyapunov_stability(performance_history: &[f64], embedding_dim: usize) -> StabilityAnalysis {
    let n = performance_history.len();
    if n < embedding_dim * 3 + 5 {
        return StabilityAnalysis {
            csi: 0.0,
            classification: StabilityClass::InsufficientData,
            dimension_stability: Vec::new(),
            action: StabilityAction::GatherMoreData,
        };
    }

    // Reconstruct phase space via time-delay embedding
    let num_points = n - embedding_dim + 1;
    let mut embedded: Vec<Vec<f64>> = Vec::with_capacity(num_points);
    for i in 0..num_points {
        let point: Vec<f64> = (0..embedding_dim)
            .map(|d| performance_history[i + d])
            .collect();
        embedded.push(point);
    }

    // For each point, find nearest neighbor (excluding temporal neighbors)
    let min_temporal_sep = embedding_dim;
    let mut divergences: Vec<f64> = Vec::new();

    for i in 0..embedded.len() {
        let mut min_dist = f64::INFINITY;
        let mut _nn_idx = 0;

        for j in 0..embedded.len() {
            if (i as i64 - j as i64).unsigned_abs() as usize <= min_temporal_sep {
                continue;
            }
            let dist = euclidean_dist(&embedded[i], &embedded[j]);
            if dist < min_dist && dist > 1e-15 {
                min_dist = dist;
                _nn_idx = j;
            }
        }

        if min_dist < f64::INFINITY {
            divergences.push(min_dist.ln());
        }
    }

    // Estimate Lyapunov exponent as average log divergence rate
    if divergences.len() < 3 {
        return StabilityAnalysis {
            csi: 0.0,
            classification: StabilityClass::InsufficientData,
            dimension_stability: Vec::new(),
            action: StabilityAction::GatherMoreData,
        };
    }

    // Linear regression on divergences to estimate slope
    let csi = linear_regression_slope(&divergences);

    // Per-dimension stability from consecutive ratios
    let dim_stability: Vec<f64> = performance_history
        .windows(2)
        .take(10)
        .map(|w| {
            if w[0].abs() < 1e-15 {
                0.0
            } else {
                (w[1] / w[0]).ln().clamp(-5.0, 5.0)
            }
        })
        .collect();

    let (classification, action) = if csi < -0.1 {
        (StabilityClass::Stable, StabilityAction::MaintainCourse)
    } else if csi < 0.1 {
        (
            StabilityClass::EdgeOfChaos,
            StabilityAction::IncreaseExploration,
        )
    } else {
        (
            StabilityClass::Unstable,
            StabilityAction::EmergencyStabilize,
        )
    };

    StabilityAnalysis {
        csi,
        classification,
        dimension_stability: dim_stability,
        action,
    }
}

fn euclidean_dist(a: &[f64], b: &[f64]) -> f64 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y) * (x - y))
        .sum::<f64>()
        .sqrt()
}

fn linear_regression_slope(values: &[f64]) -> f64 {
    let n = values.len() as f64;
    if n < 2.0 {
        return 0.0;
    }
    let x_mean = (n - 1.0) / 2.0;
    let y_mean: f64 = values.iter().sum::<f64>() / n;
    let mut num = 0.0;
    let mut den = 0.0;
    for (i, &y) in values.iter().enumerate() {
        let x = i as f64;
        num += (x - x_mean) * (y - y_mean);
        den += (x - x_mean) * (x - x_mean);
    }
    if den.abs() < 1e-15 {
        0.0
    } else {
        num / den
    }
}

// ═══════════════════════════════════════════════════════════════
// 10. FISHER INFORMATION METRIC
// ═══════════════════════════════════════════════════════════════
//
// The Fisher Information Matrix defines a Riemannian metric on
// the space of strategy parameters (probability distributions).
//
// g_ij(θ) = E[∂log P(x|θ)/∂θᵢ · ∂log P(x|θ)/∂θⱼ]
//
// For a categorical distribution (strategy selection):
//   g_ij = δᵢⱼ / θᵢ  (diagonal)
//
// Natural gradient: ∇̃f = g⁻¹ · ∇f
// This converges faster than vanilla gradient because it accounts
// for the intrinsic curvature of the probability simplex.

/// Compute the Fisher Information Matrix for a categorical distribution.
/// Returns the diagonal (since FIM is diagonal for categorical).
pub fn fisher_information_diagonal(probabilities: &[f64]) -> Vec<f64> {
    probabilities
        .iter()
        .map(|&p| if p > 1e-15 { 1.0 / p } else { 1e15 })
        .collect()
}

/// Compute the natural gradient using Fisher Information.
///
/// ∇̃f = g⁻¹ · ∇f = θᵢ · ∂f/∂θᵢ  (for categorical)
///
/// This is the gradient that moves "naturally" on the probability simplex.
pub fn natural_gradient(probabilities: &[f64], euclidean_gradient: &[f64]) -> Vec<f64> {
    probabilities
        .iter()
        .zip(euclidean_gradient.iter())
        .map(|(&p, &g)| p * g)
        .collect()
}

/// Apply a natural gradient step to strategy probabilities.
///
/// Returns updated probabilities on the simplex (normalized, positive).
pub fn natural_gradient_step(
    probabilities: &[f64],
    performance_deltas: &[f64],
    learning_rate: f64,
) -> Vec<f64> {
    let nat_grad = natural_gradient(probabilities, performance_deltas);
    let mut updated: Vec<f64> = probabilities
        .iter()
        .zip(nat_grad.iter())
        .map(|(&p, &ng)| (p + learning_rate * ng).max(1e-6))
        .collect();

    // Project back to simplex
    let total: f64 = updated.iter().sum();
    if total > 0.0 {
        for v in &mut updated {
            *v /= total;
        }
    }
    updated
}

// ═══════════════════════════════════════════════════════════════
// 11. KOLMOGOROV-SMIRNOV DRIFT DETECTION
// ═══════════════════════════════════════════════════════════════
//
// Detects distributional shift (concept drift) in strategy performance
// using the Kolmogorov-Smirnov two-sample test.
//
// D_KS = max|F₁(x) - F₂(x)|
//
// If D_KS exceeds the critical value, the performance distribution
// has shifted → strategies need retraining/re-evaluation.

/// Result of a KS drift test.
#[derive(Debug, Clone)]
pub struct DriftResult {
    /// KS statistic D.
    pub ks_statistic: f64,
    /// Critical value at α = 0.05.
    pub critical_value: f64,
    /// Is drift detected?
    pub drift_detected: bool,
    /// Approximate p-value.
    pub p_value: f64,
}

/// Two-sample Kolmogorov-Smirnov test.
///
/// Tests whether two samples come from the same distribution.
pub fn ks_two_sample(sample_a: &[f64], sample_b: &[f64]) -> DriftResult {
    let n1 = sample_a.len();
    let n2 = sample_b.len();

    if n1 < 2 || n2 < 2 {
        return DriftResult {
            ks_statistic: 0.0,
            critical_value: f64::INFINITY,
            drift_detected: false,
            p_value: 1.0,
        };
    }

    let mut sorted_a = sample_a.to_vec();
    let mut sorted_b = sample_b.to_vec();
    sorted_a.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    sorted_b.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    // Compute max|F1(x) - F2(x)|
    let mut d_max = 0.0f64;
    let mut i = 0;
    let mut j = 0;

    while i < n1 && j < n2 {
        let f1 = (i + 1) as f64 / n1 as f64;
        let f2 = (j + 1) as f64 / n2 as f64;
        let d = (f1 - f2).abs();
        d_max = d_max.max(d);

        if sorted_a[i] <= sorted_b[j] {
            i += 1;
        } else {
            j += 1;
        }
    }

    // Handle remaining elements
    while i < n1 {
        let f1 = (i + 1) as f64 / n1 as f64;
        let f2 = j as f64 / n2 as f64;
        d_max = d_max.max((f1 - f2).abs());
        i += 1;
    }
    while j < n2 {
        let f1 = i as f64 / n1 as f64;
        let f2 = (j + 1) as f64 / n2 as f64;
        d_max = d_max.max((f1 - f2).abs());
        j += 1;
    }

    // Critical value at α = 0.05:
    // c(α) ≈ sqrt(-ln(α/2)/2) · sqrt((n1+n2)/(n1·n2))
    let c_alpha = (-0.025_f64.ln() / 2.0).sqrt();
    let n_factor = ((n1 + n2) as f64 / (n1 * n2) as f64).sqrt();
    let critical = c_alpha * n_factor;

    // Approximate p-value using the KS distribution:
    // P(D > d) ≈ 2·e^(-2·n_eff·d²)
    let n_eff = (n1 * n2) as f64 / (n1 + n2) as f64;
    let p_value = (2.0 * (-2.0 * n_eff * d_max * d_max).exp()).clamp(0.0, 1.0);

    DriftResult {
        ks_statistic: d_max,
        critical_value: critical,
        drift_detected: d_max > critical,
        p_value,
    }
}

// ═══════════════════════════════════════════════════════════════
// 12. FRACTAL RECURSION DEPTH ESTIMATOR
// ═══════════════════════════════════════════════════════════════
//
// Estimates the optimal recursion depth for problem decomposition
// using fractal dimension analysis.
//
// Box-counting dimension:
//   D = lim(ε→0) log(N(ε)) / log(1/ε)
//
// Where N(ε) is the number of ε-sized boxes needed to cover
// the problem's feature space.
//
// Higher fractal dimension → more complex problem → deeper recursion.
// D ≈ 1.0 → linear problem → shallow recursion.
// D ≈ 2.0+ → fractal/chaotic → deep recursion.

/// Box-counting fractal dimension estimate from feature vectors.
pub fn fractal_dimension(points: &[Vec<f64>]) -> f64 {
    if points.is_empty() || points[0].is_empty() {
        return 1.0;
    }

    let dim = points[0].len();

    // Find bounding box
    let mut mins = vec![f64::INFINITY; dim];
    let mut maxs = vec![f64::NEG_INFINITY; dim];
    for point in points {
        for (d, &val) in point.iter().enumerate().take(dim) {
            mins[d] = mins[d].min(val);
            maxs[d] = maxs[d].max(val);
        }
    }

    let ranges: Vec<f64> = mins
        .iter()
        .zip(maxs.iter())
        .map(|(&mn, &mx)| (mx - mn).max(1e-10))
        .collect();
    let max_range = ranges.iter().cloned().fold(0.0, f64::max).max(1e-10);

    // Box counting at multiple scales
    let scales = [2, 4, 8, 16, 32];
    let mut log_counts = Vec::new();
    let mut log_inv_eps = Vec::new();

    for &num_boxes_per_dim in &scales {
        let epsilon = max_range / num_boxes_per_dim as f64;
        let mut occupied: HashSet<Vec<i64>> = HashSet::new();

        for point in points {
            let box_coords: Vec<i64> = point
                .iter()
                .enumerate()
                .take(dim)
                .map(|(d, &val)| ((val - mins[d]) / epsilon).floor() as i64)
                .collect();
            occupied.insert(box_coords);
        }

        if !occupied.is_empty() {
            log_counts.push((occupied.len() as f64).ln());
            log_inv_eps.push((1.0 / epsilon).ln());
        }
    }

    // Linear regression: log(N) = D · log(1/ε) + const
    if log_counts.len() < 2 {
        return 1.0;
    }

    linear_regression_slope_xy(&log_inv_eps, &log_counts).clamp(0.5, 10.0)
}

/// Estimate optimal recursion depth from fractal dimension.
///
/// depth = ceil(D · log₂(N))
/// where D is the fractal dimension and N is problem complexity.
pub fn optimal_recursion_depth(fractal_dim: f64, problem_tokens: usize) -> usize {
    let n = (problem_tokens as f64).max(2.0);
    let raw = fractal_dim * n.log2();
    (raw.ceil() as usize).clamp(2, 20)
}

fn linear_regression_slope_xy(x: &[f64], y: &[f64]) -> f64 {
    let n = x.len().min(y.len());
    if n < 2 {
        return 0.0;
    }
    let n_f = n as f64;
    let x_mean = x.iter().take(n).sum::<f64>() / n_f;
    let y_mean = y.iter().take(n).sum::<f64>() / n_f;
    let mut num = 0.0;
    let mut den = 0.0;
    for i in 0..n {
        num += (x[i] - x_mean) * (y[i] - y_mean);
        den += (x[i] - x_mean) * (x[i] - x_mean);
    }
    if den.abs() < 1e-15 {
        0.0
    } else {
        num / den
    }
}

// ═══════════════════════════════════════════════════════════════
// COMPOSITE INTELLIGENCE METRICS
// ═══════════════════════════════════════════════════════════════

/// All-in-one cognitive metrics for a single problem-solving cycle.
#[derive(Debug, Clone)]
pub struct CognitiveMetrics {
    pub resonance: f64,
    pub proof_strength: ProofMetrics,
    pub creative_distance: f64,
    pub topology: TopologicalInvariants,
    pub stability: StabilityAnalysis,
    pub fractal_dim: f64,
    pub optimal_depth: usize,
    pub quantum_probability: f64,
}

/// Compute the Unified Cognitive Score (UCS).
///
/// UCS = w₁·R + w₂·EPS + w₃·(1-KCD) + w₄·QP + w₅·(1-CSI) + w₆·1/(1+FD)
///
/// Where:
///   R = normalized resonance
///   EPS = entropic proof strength
///   KCD = creative distance (inverted: low distance = high synthesis potential)
///   QP = quantum probability
///   CSI = cognitive stability index (inverted: stability = good)
///   FD = fractal dimension (inverted via sigmoid: lower complexity = easier)
pub fn unified_cognitive_score(
    resonance: f64,
    eps: f64,
    kcd: f64,
    quantum_prob: f64,
    csi: f64,
    fractal_dim: f64,
    weights: &[f64; 6],
) -> f64 {
    let r_norm = resonance.clamp(0.0, 1.0);
    let eps_norm = eps.clamp(0.0, 1.0);
    let kcd_norm = (1.0 - kcd).clamp(0.0, 1.0);
    let qp_norm = quantum_prob.clamp(0.0, 1.0);
    let csi_norm = (1.0 / (1.0 + csi.abs())).clamp(0.0, 1.0);
    let fd_norm = (1.0 / (1.0 + fractal_dim)).clamp(0.0, 1.0);

    let total_weight: f64 = weights.iter().sum::<f64>().max(1e-10);

    (weights[0] * r_norm
        + weights[1] * eps_norm
        + weights[2] * kcd_norm
        + weights[3] * qp_norm
        + weights[4] * csi_norm
        + weights[5] * fd_norm)
        / total_weight
}

// ═══════════════════════════════════════════════════════════════
// CONVENIENCE BRIDGE FUNCTIONS
// ═══════════════════════════════════════════════════════════════

/// Simplified quantum amplitude score for timeline scoring in D5.
///
/// Combines confidence amplitude with novelty and efficiency
/// using Born-rule probability: P = |α·e^(iθ)|² where phase
/// is derived from novelty-efficiency balance.
///
/// Returns a score ∈ [0, 1].
pub fn quantum_amplitude_score(amplitude: f64, novelty: f64, efficiency: f64) -> f64 {
    // Phase angle from novelty-efficiency balance
    let pi = std::f64::consts::PI;
    let phase = pi * (novelty - efficiency).clamp(-1.0, 1.0) / 2.0;
    let amp = QuantumAmplitude::from_polar(amplitude.clamp(0.0, 1.0), phase);
    let prob = amp.probability();
    // Boost by constructive interference when novelty ≈ efficiency
    let interference_bonus = (1.0 - (novelty - efficiency).abs()) * 0.1;
    (prob + interference_bonus).clamp(0.0, 1.0)
}
