// ─────────────────────────────────────────────────────────────
// TurboQuant — Online Vector Quantization (arXiv:2504.19874)
// ─────────────────────────────────────────────────────────────
// Implements the two quantizers from "TurboQuant: Online Vector Quantization
// with Near-optimal Distortion Rate" (Zandieh, Daliri, Mirrokni, Hadian 2025):
//
//   * Q_mse (Algorithm 1): randomly rotate the input so every coordinate of a
//     unit vector follows a Beta distribution that converges to N(0, 1/d)
//     (Lemma 1), then quantize each coordinate independently with the optimal
//     Lloyd-Max scalar codebook for that distribution. Distortion is provably
//     D_mse <= (3/2) * 4^-b (Theorem 1; refined 0.36 / 0.117 / 0.03 / 0.009
//     for b = 1..4), within a ~2.7x factor of the Shannon lower bound
//     D >= 4^-b (Theorem 3).
//
//   * Q_prod (Algorithm 2): MSE-optimal quantizers are biased for inner
//     products, so the inner-product quantizer spends b-1 bits on Q_mse and
//     applies the 1-bit Quantized JL transform (QJL, Definition 1) to the
//     residual: sign(S r) with S i.i.d. N(0,1) and dequantization
//     sqrt(pi/2)/d * S^T z. This yields an *unbiased* inner-product estimator
//     with variance <= (pi / 2d) * ||y||^2 (Lemma 4) and total distortion
//     D_prod <= 32/d * 4^-b (Theorem 2).
//
// The rotation is a seeded randomized Hadamard pipeline (two sign-flip +
// Walsh-Hadamard rounds), the paper's accelerator-friendly choice: O(d log d)
// per vector, data-oblivious, and suitable for online use. The non-unit-norm
// case is handled exactly as the paper prescribes: the L2 norm is stored in
// floating point and the dequantized point is rescaled.
//
// Memory: a d-dim f64 vector costs 8d bytes; TurboQuant stores b bits per
// coordinate plus two f64 norms — a 16x reduction at b = 4. Inner products
// against the compressed form are computed directly in the rotated domain
// (rotations preserve inner products), so retrieval never reconstructs the
// full corpus.

use serde::{Deserialize, Serialize};

/// Lloyd-Max optimal centroids for the standard normal distribution, the
/// limiting coordinate distribution after rotation (paper Section 3.1: for
/// b = 1, 2 the optimal centroids are +-sqrt(2/pi)/sqrt(d) and
/// {+-0.453, +-1.51}/sqrt(d); values here are for the unit-variance variable
/// z = y * sqrt(d) and extend the table to b = 3, 4 via Max (1960)).
const CODEBOOK_B1: [f64; 2] = [-0.797_884_560_8, 0.797_884_560_8];
const CODEBOOK_B2: [f64; 4] = [
    -1.510_417_608_7,
    -0.452_780_039_8,
    0.452_780_039_8,
    1.510_417_608_7,
];
const CODEBOOK_B3: [f64; 8] = [
    -2.151_945_799_0,
    -1.343_909_261_3,
    -0.756_005_259_0,
    -0.245_172_423_4,
    0.245_172_423_4,
    0.756_005_259_0,
    1.343_909_261_3,
    2.151_945_799_0,
];
const CODEBOOK_B4: [f64; 16] = [
    -2.732_693_549_6,
    -2.069_011_811_0,
    -1.618_046_450_3,
    -1.256_230_947_9,
    -0.942_397_612_2,
    -0.656_817_195_9,
    -0.388_092_476_3,
    -0.128_403_702_4,
    0.128_403_702_4,
    0.388_092_476_3,
    0.656_817_195_9,
    0.942_397_612_2,
    1.256_230_947_9,
    1.618_046_450_3,
    2.069_011_811_0,
    2.732_693_549_6,
];

const SQRT_HALF_PI: f64 = 1.253_314_137_3; // sqrt(pi/2), the QJL dequant scale
const EPS: f64 = 1e-12;

fn codebook(bits: u8) -> &'static [f64] {
    match bits {
        1 => &CODEBOOK_B1,
        2 => &CODEBOOK_B2,
        3 => &CODEBOOK_B3,
        _ => &CODEBOOK_B4,
    }
}

/// A vector compressed by TurboQuant. `codes` holds `mse_bits` per coordinate
/// (packed); `residual_signs` holds the 1-bit QJL sketch of the residual when
/// the vector was quantized for unbiased inner products. Norms are kept in
/// floating point per the paper's rescaling scheme.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuantizedVector {
    pub dim: u32,
    pub mse_bits: u8,
    pub norm: f64,
    pub residual_norm: f64,
    pub codes: Vec<u8>,
    pub residual_signs: Vec<u8>,
}

impl QuantizedVector {
    /// Bytes actually held by the compressed representation.
    #[must_use]
    pub fn compressed_bytes(&self) -> usize {
        self.codes.len() + self.residual_signs.len() + 2 * std::mem::size_of::<f64>()
    }

    /// Bytes the raw f64 vector would occupy.
    #[must_use]
    pub fn raw_bytes(&self) -> usize {
        self.dim as usize * std::mem::size_of::<f64>()
    }

    /// Compact text encoding for persistence (e.g. a SQLite TEXT column):
    /// `tq1:<dim>:<mse_bits>:<norm>:<residual_norm>:<hex codes>:<hex signs>`.
    #[must_use]
    pub fn to_compact_string(&self) -> String {
        format!(
            "tq1:{}:{}:{}:{}:{}:{}",
            self.dim,
            self.mse_bits,
            self.norm,
            self.residual_norm,
            hex_encode(&self.codes),
            hex_encode(&self.residual_signs),
        )
    }

    /// Decodes [`Self::to_compact_string`]; `None` for foreign/legacy payloads.
    #[must_use]
    pub fn from_compact_string(value: &str) -> Option<Self> {
        let mut parts = value.split(':');
        if parts.next()? != "tq1" {
            return None;
        }
        let dim: u32 = parts.next()?.parse().ok()?;
        let mse_bits: u8 = parts.next()?.parse().ok()?;
        let norm: f64 = parts.next()?.parse().ok()?;
        let residual_norm: f64 = parts.next()?.parse().ok()?;
        let codes = hex_decode(parts.next()?)?;
        let residual_signs = hex_decode(parts.next()?)?;
        if parts.next().is_some() || mse_bits > 4 {
            return None;
        }
        Some(Self {
            dim,
            mse_bits,
            norm,
            residual_norm,
            codes,
            residual_signs,
        })
    }
}

/// A query prepared once per request: rotated into the quantizer's basis (so
/// inner products can be taken directly against codebook values) and sketched
/// through the QJL matrix (so the residual correction is a sign dot-product
/// per candidate instead of a matrix multiply).
#[derive(Debug, Clone)]
pub struct PreparedQuery {
    pub norm: f64,
    rotated: Vec<f64>,
    sketch: Vec<f64>,
}

/// Online, data-oblivious vector quantizer per the paper. All randomness is
/// derived from the seed, so quantization is reproducible across restarts —
/// vectors quantized in one process decode in any other given the same seed.
#[derive(Debug, Clone)]
pub struct TurboQuant {
    dim: usize,
    padded: usize,
    bit_width: u8,
    seed: u64,
    signs1: Vec<f64>,
    signs2: Vec<f64>,
}

impl TurboQuant {
    /// `bit_width` is the *total* bits per coordinate (1..=4 supported, as in
    /// the paper's precomputed codebooks).
    #[must_use]
    pub fn new(dim: usize, bit_width: u8, seed: u64) -> Self {
        let bit_width = bit_width.clamp(1, 4);
        let padded = dim.next_power_of_two().max(2);
        let mut rng = SplitMix64::new(seed);
        let signs1 = (0..padded)
            .map(|_| if rng.next() & 1 == 0 { 1.0 } else { -1.0 })
            .collect();
        let signs2 = (0..padded)
            .map(|_| if rng.next() & 1 == 0 { 1.0 } else { -1.0 })
            .collect();
        Self {
            dim,
            padded,
            bit_width,
            seed,
            signs1,
            signs2,
        }
    }

    /// Algorithm 1 (Q_mse): rotate, then snap each coordinate to the nearest
    /// Lloyd-Max centroid using all `bit_width` bits. Lowest distortion for
    /// reconstruction, biased for inner products.
    #[must_use]
    pub fn quantize(&self, vector: &[f64]) -> QuantizedVector {
        self.quantize_internal(vector, self.bit_width, false)
    }

    /// Algorithm 2 (Q_prod): spend `bit_width - 1` bits on Q_mse, then apply
    /// the 1-bit QJL transform to the residual, producing an unbiased
    /// inner-product estimator (Theorem 2). With `bit_width == 1` this is the
    /// pure QJL quantizer.
    #[must_use]
    pub fn quantize_for_inner_product(&self, vector: &[f64]) -> QuantizedVector {
        self.quantize_internal(vector, self.bit_width - 1, true)
    }

    fn quantize_internal(&self, vector: &[f64], mse_bits: u8, qjl: bool) -> QuantizedVector {
        let norm = l2_norm(vector);
        if norm < EPS {
            return QuantizedVector {
                dim: self.dim as u32,
                mse_bits,
                norm: 0.0,
                residual_norm: 0.0,
                codes: Vec::new(),
                residual_signs: Vec::new(),
            };
        }
        // Unit-norm input, padded to a power of two; the norm is stored and
        // restored at dequantization exactly as the paper prescribes.
        let mut rotated = vec![0.0; self.padded];
        for (slot, value) in rotated.iter_mut().zip(vector.iter()) {
            *slot = value / norm;
        }
        self.rotate(&mut rotated);

        // Coordinates of the rotated unit vector are ~ N(0, 1/d'); rescale by
        // sqrt(d') and quantize against the unit-variance codebook.
        let scale = (self.padded as f64).sqrt();
        let mut codes = Vec::new();
        let mut residual = vec![0.0; self.padded];
        if mse_bits > 0 {
            let book = codebook(mse_bits);
            codes = vec![0u8; (self.padded * mse_bits as usize).div_ceil(8)];
            for (j, slot) in residual.iter_mut().enumerate() {
                let z = rotated[j] * scale;
                let idx = nearest_centroid(book, z);
                pack_code(&mut codes, j, mse_bits, idx as u8);
                *slot = (z - book[idx]) / scale;
            }
        } else {
            residual.copy_from_slice(&rotated);
        }

        let (residual_norm, residual_signs) = if qjl {
            let r_norm = l2_norm(&residual);
            if r_norm < EPS {
                (0.0, Vec::new())
            } else {
                // QJL (Definition 1): one sign bit per coordinate of S r. The
                // Gaussian rows of S are regenerated from the seed on demand,
                // so the sketch matrix itself costs no memory.
                let mut signs = vec![0u8; self.padded.div_ceil(8)];
                for i in 0..self.padded {
                    let dot = self.qjl_row_dot(i, &residual);
                    if dot >= 0.0 {
                        signs[i / 8] |= 1 << (i % 8);
                    }
                }
                (r_norm, signs)
            }
        } else {
            (0.0, Vec::new())
        };

        QuantizedVector {
            dim: self.dim as u32,
            mse_bits,
            norm,
            residual_norm,
            codes,
            residual_signs,
        }
    }

    /// DeQuant (Algorithm 1): centroid lookup, inverse rotation, norm rescale.
    #[must_use]
    pub fn dequantize(&self, quantized: &QuantizedVector) -> Vec<f64> {
        if quantized.norm < EPS {
            return vec![0.0; self.dim];
        }
        let scale = (self.padded as f64).sqrt();
        let mut rotated = vec![0.0; self.padded];
        if quantized.mse_bits > 0 && !quantized.codes.is_empty() {
            let book = codebook(quantized.mse_bits);
            for (j, slot) in rotated.iter_mut().enumerate() {
                let idx = unpack_code(&quantized.codes, j, quantized.mse_bits) as usize;
                *slot = book[idx.min(book.len() - 1)] / scale;
            }
        }
        self.rotate_inverse(&mut rotated);
        rotated
            .into_iter()
            .take(self.dim)
            .map(|value| value * quantized.norm)
            .collect()
    }

    /// Rotates and QJL-sketches a query once so that scoring each compressed
    /// candidate is O(d) regardless of how many candidates there are.
    #[must_use]
    pub fn prepare_query(&self, query: &[f64]) -> PreparedQuery {
        let mut rotated = vec![0.0; self.padded];
        for (slot, value) in rotated.iter_mut().zip(query.iter()) {
            *slot = *value;
        }
        self.rotate(&mut rotated);
        let sketch = (0..self.padded)
            .map(|i| self.qjl_row_dot(i, &rotated))
            .collect();
        PreparedQuery {
            norm: l2_norm(query),
            rotated,
            sketch,
        }
    }

    /// Unbiased inner-product estimate <query, x> against a compressed vector
    /// (Theorem 2). Both terms live in the rotated domain — rotations preserve
    /// inner products, so nothing is reconstructed:
    ///   <q, x~> = ||x|| * ( <Θq, y~> + sqrt(pi/2) * ||r|| / d * <S Θq, sign(S r)> )
    #[must_use]
    pub fn inner_product(&self, query: &PreparedQuery, quantized: &QuantizedVector) -> f64 {
        if quantized.norm < EPS {
            return 0.0;
        }
        let scale = (self.padded as f64).sqrt();
        let mut dot = 0.0;
        if quantized.mse_bits > 0 && !quantized.codes.is_empty() {
            let book = codebook(quantized.mse_bits);
            for (j, q) in query.rotated.iter().enumerate() {
                let idx = unpack_code(&quantized.codes, j, quantized.mse_bits) as usize;
                dot += q * book[idx.min(book.len() - 1)] / scale;
            }
        }
        if quantized.residual_norm > EPS && !quantized.residual_signs.is_empty() {
            let mut sign_dot = 0.0;
            for (i, sketch) in query.sketch.iter().enumerate() {
                let bit = quantized.residual_signs[i / 8] >> (i % 8) & 1;
                sign_dot += if bit == 1 { *sketch } else { -*sketch };
            }
            dot += SQRT_HALF_PI * quantized.residual_norm / self.padded as f64 * sign_dot;
        }
        quantized.norm * dot
    }

    /// Cosine similarity in [-1, 1] from the unbiased inner-product estimate.
    #[must_use]
    pub fn cosine_similarity(&self, query: &PreparedQuery, quantized: &QuantizedVector) -> f64 {
        if query.norm < EPS || quantized.norm < EPS {
            return 0.0;
        }
        (self.inner_product(query, quantized) / (query.norm * quantized.norm)).clamp(-1.0, 1.0)
    }

    /// Randomized Hadamard rotation: two rounds of sign flips + Walsh-Hadamard
    /// (each round is orthonormal), the paper's accelerator-friendly stand-in
    /// for a Haar-random rotation. O(d log d).
    fn rotate(&self, vector: &mut [f64]) {
        for (value, sign) in vector.iter_mut().zip(self.signs1.iter()) {
            *value *= sign;
        }
        walsh_hadamard(vector);
        for (value, sign) in vector.iter_mut().zip(self.signs2.iter()) {
            *value *= sign;
        }
        walsh_hadamard(vector);
    }

    fn rotate_inverse(&self, vector: &mut [f64]) {
        walsh_hadamard(vector);
        for (value, sign) in vector.iter_mut().zip(self.signs2.iter()) {
            *value *= sign;
        }
        walsh_hadamard(vector);
        for (value, sign) in vector.iter_mut().zip(self.signs1.iter()) {
            *value *= sign;
        }
    }

    /// Dot of the i-th Gaussian QJL row with `vector`, with the row streamed
    /// from the seeded generator instead of being stored.
    fn qjl_row_dot(&self, row: usize, vector: &[f64]) -> f64 {
        let mut rng =
            SplitMix64::new(self.seed ^ (row as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15));
        vector.iter().map(|value| value * rng.next_gaussian()).sum()
    }
}

/// In-place orthonormal Walsh-Hadamard transform (length must be a power of
/// two; rows scaled by 1/sqrt(n) so the transform is its own inverse).
fn walsh_hadamard(vector: &mut [f64]) {
    let n = vector.len();
    let mut h = 1;
    while h < n {
        for block in (0..n).step_by(h * 2) {
            for j in block..block + h {
                let x = vector[j];
                let y = vector[j + h];
                vector[j] = x + y;
                vector[j + h] = x - y;
            }
        }
        h *= 2;
    }
    let scale = 1.0 / (n as f64).sqrt();
    for value in vector.iter_mut() {
        *value *= scale;
    }
}

fn nearest_centroid(book: &[f64], value: f64) -> usize {
    let mut best = 0;
    let mut best_distance = f64::INFINITY;
    for (idx, centroid) in book.iter().enumerate() {
        let distance = (value - centroid).abs();
        if distance < best_distance {
            best_distance = distance;
            best = idx;
        }
    }
    best
}

fn pack_code(codes: &mut [u8], index: usize, bits: u8, value: u8) {
    let bit_pos = index * bits as usize;
    for offset in 0..bits as usize {
        if value >> offset & 1 == 1 {
            let absolute = bit_pos + offset;
            codes[absolute / 8] |= 1 << (absolute % 8);
        }
    }
}

fn unpack_code(codes: &[u8], index: usize, bits: u8) -> u8 {
    let bit_pos = index * bits as usize;
    let mut value = 0u8;
    for offset in 0..bits as usize {
        let absolute = bit_pos + offset;
        if absolute / 8 < codes.len() && codes[absolute / 8] >> (absolute % 8) & 1 == 1 {
            value |= 1 << offset;
        }
    }
    value
}

fn l2_norm(vector: &[f64]) -> f64 {
    vector.iter().map(|value| value * value).sum::<f64>().sqrt()
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn hex_decode(text: &str) -> Option<Vec<u8>> {
    if text.len() % 2 != 0 {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).ok())
        .collect()
}

/// Deterministic seeded generator (SplitMix64) with a Box-Muller Gaussian.
/// Hand-rolled so quantized data is reproducible across crates, platforms,
/// and dependency upgrades — a requirement for persisted vectors.
struct SplitMix64 {
    state: u64,
    spare: Option<f64>,
}

impl SplitMix64 {
    fn new(seed: u64) -> Self {
        Self {
            state: seed,
            spare: None,
        }
    }

    fn next(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn next_f64(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }

    fn next_gaussian(&mut self) -> f64 {
        if let Some(value) = self.spare.take() {
            return value;
        }
        // Box-Muller with a guard against log(0).
        let u1 = self.next_f64().max(1e-300);
        let u2 = self.next_f64();
        let radius = (-2.0 * u1.ln()).sqrt();
        let angle = 2.0 * std::f64::consts::PI * u2;
        self.spare = Some(radius * angle.sin());
        radius * angle.cos()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn random_unit_vector(dim: usize, rng: &mut SplitMix64) -> Vec<f64> {
        let mut vector: Vec<f64> = (0..dim).map(|_| rng.next_gaussian()).collect();
        let norm = l2_norm(&vector);
        for value in &mut vector {
            *value /= norm;
        }
        vector
    }

    #[test]
    fn mse_distortion_matches_paper_bounds() {
        // Theorem 1 refined bounds for b = 1..4 on unit vectors. The slack
        // factor absorbs the Hadamard-vs-Haar rotation and finite dimension.
        let bounds = [0.36, 0.117, 0.03, 0.009];
        let mut rng = SplitMix64::new(7);
        for (bits, bound) in (1u8..=4).zip(bounds) {
            let quantizer = TurboQuant::new(64, bits, 42);
            let trials = 200;
            let mut total = 0.0;
            for _ in 0..trials {
                let x = random_unit_vector(64, &mut rng);
                let reconstructed = quantizer.dequantize(&quantizer.quantize(&x));
                total += x
                    .iter()
                    .zip(reconstructed.iter())
                    .map(|(a, b)| (a - b) * (a - b))
                    .sum::<f64>();
            }
            let mse = total / trials as f64;
            assert!(
                mse <= bound * 1.35,
                "b={bits}: observed MSE {mse:.4} exceeds paper bound {bound} (+35% slack)"
            );
        }
    }

    #[test]
    fn dequantize_restores_dim_and_norm() {
        let quantizer = TurboQuant::new(48, 4, 9);
        let mut rng = SplitMix64::new(3);
        let mut x = random_unit_vector(48, &mut rng);
        for value in &mut x {
            *value *= 7.5; // non-unit norm exercises the rescaling path
        }
        let quantized = quantizer.quantize(&x);
        let reconstructed = quantizer.dequantize(&quantized);
        assert_eq!(reconstructed.len(), 48);
        let norm = l2_norm(&reconstructed);
        assert!(
            (norm - 7.5).abs() < 0.8,
            "reconstructed norm {norm} too far from 7.5"
        );
    }

    #[test]
    fn inner_product_estimator_is_unbiased() {
        // Lemma 4 / Theorem 2: averaging the estimator over independent
        // quantizer seeds must converge on the true inner product.
        let dim = 64;
        let mut rng = SplitMix64::new(11);
        let x = random_unit_vector(dim, &mut rng);
        let y = random_unit_vector(dim, &mut rng);
        let truth: f64 = x.iter().zip(y.iter()).map(|(a, b)| a * b).sum();

        let trials = 400;
        let mut total = 0.0;
        for seed in 0..trials {
            let quantizer = TurboQuant::new(dim, 2, seed);
            let quantized = quantizer.quantize_for_inner_product(&x);
            let prepared = quantizer.prepare_query(&y);
            total += quantizer.inner_product(&prepared, &quantized);
        }
        let mean = total / trials as f64;
        // Var per estimate <= 0.56/d (Theorem 2, b=2) => sem ~ 0.0047.
        assert!(
            (mean - truth).abs() < 0.02,
            "estimator biased: mean {mean:.4} vs truth {truth:.4}"
        );
    }

    #[test]
    fn rotated_domain_scoring_matches_reconstruction() {
        // <q, dequant(x)> computed in the rotated domain must equal the same
        // inner product taken after explicit reconstruction (MSE part only).
        let quantizer = TurboQuant::new(32, 4, 5);
        let mut rng = SplitMix64::new(21);
        let x = random_unit_vector(32, &mut rng);
        let q = random_unit_vector(32, &mut rng);
        let quantized = quantizer.quantize(&x);
        let prepared = quantizer.prepare_query(&q);
        let fast = quantizer.inner_product(&prepared, &quantized);
        let slow: f64 = q
            .iter()
            .zip(quantizer.dequantize(&quantized).iter())
            .map(|(a, b)| a * b)
            .sum();
        assert!(
            (fast - slow).abs() < 1e-9,
            "rotated-domain dot {fast} != reconstruction dot {slow}"
        );
    }

    #[test]
    fn compact_string_round_trips() {
        let quantizer = TurboQuant::new(64, 4, 1);
        let mut rng = SplitMix64::new(33);
        let x = random_unit_vector(64, &mut rng);
        let quantized = quantizer.quantize_for_inner_product(&x);
        let encoded = quantized.to_compact_string();
        let decoded = QuantizedVector::from_compact_string(&encoded).expect("decode");
        assert_eq!(quantized, decoded);
        // Legacy / foreign payloads are rejected, not misparsed.
        assert!(QuantizedVector::from_compact_string("[0.1, 0.2]").is_none());
        assert!(QuantizedVector::from_compact_string("tq1:bad").is_none());
    }

    #[test]
    fn zero_vector_is_safe() {
        let quantizer = TurboQuant::new(16, 3, 2);
        let quantized = quantizer.quantize(&vec![0.0; 16]);
        assert_eq!(quantizer.dequantize(&quantized), vec![0.0; 16]);
        let prepared = quantizer.prepare_query(&vec![0.0; 16]);
        assert_eq!(quantizer.inner_product(&prepared, &quantized), 0.0);
    }

    #[test]
    fn compression_saves_memory() {
        let quantizer = TurboQuant::new(64, 4, 4);
        let mut rng = SplitMix64::new(8);
        let x = random_unit_vector(64, &mut rng);
        let quantized = quantizer.quantize_for_inner_product(&x);
        // 64 f64 coordinates = 512 bytes raw; 4 bits/coord + norms ~= 56 bytes.
        assert_eq!(quantized.raw_bytes(), 512);
        assert!(
            quantized.compressed_bytes() * 8 <= quantized.raw_bytes(),
            "expected >= 8x compression, got {} of {} bytes",
            quantized.compressed_bytes(),
            quantized.raw_bytes()
        );
    }
}
