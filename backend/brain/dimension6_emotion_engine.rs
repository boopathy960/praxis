// ═══════════════════════════════════════════════════════════════
// Dimension 6: Emotion Simulation Engine — Valence-Arousal Cognitive Affect
// ═══════════════════════════════════════════════════════════════
//
// Production-grade artificial emotion system that influences cognitive
// priority, reasoning strategy selection, and user-adaptive behavior.
//
// Architecture:
//   1. Valence-Arousal Model — Two-axis emotional state space
//   2. Emotional Momentum — Velocity/acceleration of emotional state
//   3. Empathy Module — User emotional state estimation from text signals
//   4. Affect-Driven Strategy Selector — Emotion-influenced D1-D5 routing
//   5. Emotional Memory — Persistent affect associations per topic
//   6. Homeostatic Regulator — Prevents emotional runaway (always recovers to baseline)
//
// Mathematical Foundation:
//   - State vector: E(t) = [valence, arousal] ∈ [-1.0, 1.0]²
//   - Dynamics: dE/dt = F_stimulus + F_homeostatic + F_momentum + noise
//   - Homeostasis: F_home = -k * (E - E_baseline), k = decay_rate
//   - Empathy: sentiment(text) via lexicon-based valence summation
//
// Zero external dependencies. Pure Rust. Deterministic with optional stochastic noise.

use std::collections::HashMap;
use std::time::Instant;

// ═══════════════════════════════════════════════════════════════
// CORE TYPES
// ═══════════════════════════════════════════════════════════════

/// Two-axis emotional state: valence (positive/negative) × arousal (calm/excited).
#[derive(Debug, Clone)]
pub struct EmotionalState {
    /// Positive/negative affect: -1.0 (distress) to +1.0 (joy)
    pub valence: f64,
    /// Activation level: -1.0 (calm/sleepy) to +1.0 (excited/alert)
    pub arousal: f64,
    /// Rate of change of valence
    pub valence_velocity: f64,
    /// Rate of change of arousal
    pub arousal_velocity: f64,
    /// Timestamp of last update
    pub last_update_ms: f64,
}

impl EmotionalState {
    pub fn neutral() -> Self {
        Self {
            valence: 0.0,
            arousal: 0.2, // slight alertness baseline
            valence_velocity: 0.0,
            arousal_velocity: 0.0,
            last_update_ms: 0.0,
        }
    }

    /// Magnitude of emotion (distance from origin in V-A space)
    pub fn intensity(&self) -> f64 {
        (self.valence * self.valence + self.arousal * self.arousal).sqrt()
    }

    /// Classify the current emotional state into a discrete category
    pub fn classify(&self) -> EmotionCategory {
        match (self.valence >= 0.0, self.arousal >= 0.0) {
            (true, true) => {
                if self.valence > 0.6 && self.arousal > 0.6 {
                    EmotionCategory::Elation
                } else if self.valence > 0.3 {
                    EmotionCategory::Excitement
                } else {
                    EmotionCategory::Alertness
                }
            }
            (true, false) => {
                if self.valence > 0.5 {
                    EmotionCategory::Contentment
                } else {
                    EmotionCategory::Serenity
                }
            }
            (false, true) => {
                if self.valence < -0.6 {
                    EmotionCategory::Frustration
                } else if self.arousal > 0.6 {
                    EmotionCategory::Tension
                } else {
                    EmotionCategory::Concern
                }
            }
            (false, false) => {
                if self.valence < -0.5 {
                    EmotionCategory::Dejection
                } else {
                    EmotionCategory::Boredom
                }
            }
        }
    }
}

/// Discrete emotion categories derived from the V-A space.
#[derive(Debug, Clone, PartialEq)]
pub enum EmotionCategory {
    Elation,     // High V, High A — "eureka" moments
    Excitement,  // Medium V, High A — active engagement
    Alertness,   // Low V, High A — watchful focus
    Contentment, // High V, Low A — satisfied calm
    Serenity,    // Medium V, Low A — peaceful state
    Boredom,     // Low V, Low A — disengaged
    Concern,     // Low negative V, High A — cautious
    Tension,     // Medium negative V, High A — stress
    Frustration, // High negative V, High A — blocked goals
    Dejection,   // High negative V, Low A — exhaustion/defeat
}

impl EmotionCategory {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Elation => "elation",
            Self::Excitement => "excitement",
            Self::Alertness => "alertness",
            Self::Contentment => "contentment",
            Self::Serenity => "serenity",
            Self::Boredom => "boredom",
            Self::Concern => "concern",
            Self::Tension => "tension",
            Self::Frustration => "frustration",
            Self::Dejection => "dejection",
        }
    }
}

/// A stimulus that affects emotional state.
#[derive(Debug, Clone)]
pub struct EmotionalStimulus {
    /// Source of the stimulus
    pub source: StimulusSource,
    /// Valence impulse: positive = good news, negative = bad news
    pub valence_impulse: f64,
    /// Arousal impulse: positive = activating, negative = calming
    pub arousal_impulse: f64,
    /// How quickly this stimulus decays (0.0 = instant, 1.0 = persistent)
    pub persistence: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StimulusSource {
    ProblemDifficulty,
    SolutionFound,
    SolutionFailed,
    UserUrgency,
    UserSatisfaction,
    UserFrustration,
    NoveltyDetected,
    DeadlineApproaching,
    MemoryRecall,
    SelfCorrection,
}

/// Configuration for the emotion engine.
#[derive(Debug, Clone)]
pub struct EmotionConfig {
    /// Rate at which emotions decay toward baseline (higher = faster decay)
    pub homeostatic_decay: f64,
    /// Baseline emotional state to return to
    pub baseline_valence: f64,
    pub baseline_arousal: f64,
    /// Sensitivity to stimuli (multiplier on impulses)
    pub sensitivity: f64,
    /// Maximum allowed intensity before clamping
    pub max_intensity: f64,
    /// Noise amplitude for stochastic variation
    pub noise_amplitude: f64,
    /// Enable empathy module
    pub empathy_enabled: bool,
}

impl Default for EmotionConfig {
    fn default() -> Self {
        Self {
            homeostatic_decay: 0.05,
            baseline_valence: 0.0,
            baseline_arousal: 0.2,
            sensitivity: 1.0,
            max_intensity: 1.0,
            noise_amplitude: 0.01,
            empathy_enabled: true,
        }
    }
}

/// Result of an emotion engine tick.
#[derive(Debug, Clone)]
pub struct EmotionResult {
    pub state: EmotionalState,
    pub category: EmotionCategory,
    pub strategy_bias: StrategyBias,
    pub user_sentiment: Option<SentimentResult>,
    pub audit_trail: Vec<String>,
    pub duration_ms: f64,
}

/// How the current emotional state should bias D1-D5 strategy selection.
#[derive(Debug, Clone)]
pub struct StrategyBias {
    /// Weight modifier for D1 (Deductive) — higher when calm and focused
    pub deductive_weight: f64,
    /// Weight modifier for D2 (Creative) — higher when excited and positive
    pub creative_weight: f64,
    /// Weight modifier for D3 (Impossible) — higher when frustrated
    pub impossible_weight: f64,
    /// Weight modifier for D4 (Metacognitive) — higher when tense or concerned
    pub metacognitive_weight: f64,
    /// Weight modifier for D5 (Temporal) — higher when alert
    pub temporal_weight: f64,
    /// Recommended urgency level (0.0 = low, 1.0 = critical)
    pub urgency: f64,
}

/// Result of sentiment analysis on user text.
#[derive(Debug, Clone)]
pub struct SentimentResult {
    pub valence: f64,
    pub arousal: f64,
    pub confidence: f64,
    pub detected_signals: Vec<String>,
}

// ═══════════════════════════════════════════════════════════════
// SENTIMENT LEXICON — Production-grade emotion word database
// ═══════════════════════════════════════════════════════════════

struct SentimentLexicon {
    positive: HashMap<&'static str, (f64, f64)>, // word -> (valence, arousal)
    negative: HashMap<&'static str, (f64, f64)>,
    intensifiers: HashMap<&'static str, f64>, // word -> multiplier
    negators: Vec<&'static str>,
    urgency_markers: Vec<&'static str>,
    frustration_markers: Vec<&'static str>,
    satisfaction_markers: Vec<&'static str>,
}

impl SentimentLexicon {
    fn new() -> Self {
        let mut positive = HashMap::new();
        // (valence, arousal) — both in [0.0, 1.0]
        positive.insert("great", (0.8, 0.6));
        positive.insert("good", (0.6, 0.3));
        positive.insert("excellent", (0.9, 0.7));
        positive.insert("amazing", (0.9, 0.8));
        positive.insert("perfect", (1.0, 0.6));
        positive.insert("wonderful", (0.9, 0.7));
        positive.insert("fantastic", (0.9, 0.8));
        positive.insert("awesome", (0.8, 0.7));
        positive.insert("love", (0.9, 0.6));
        positive.insert("thanks", (0.6, 0.3));
        positive.insert("thank", (0.6, 0.3));
        positive.insert("nice", (0.5, 0.2));
        positive.insert("cool", (0.6, 0.4));
        positive.insert("brilliant", (0.9, 0.7));
        positive.insert("impressive", (0.8, 0.6));
        positive.insert("beautiful", (0.8, 0.4));
        positive.insert("fast", (0.5, 0.5));
        positive.insert("helpful", (0.7, 0.3));
        positive.insert("exactly", (0.7, 0.5));
        positive.insert("yes", (0.4, 0.3));
        positive.insert("works", (0.6, 0.4));
        positive.insert("working", (0.6, 0.4));
        positive.insert("solved", (0.8, 0.6));
        positive.insert("fixed", (0.7, 0.5));
        positive.insert("done", (0.5, 0.3));
        positive.insert("wow", (0.8, 0.8));
        positive.insert("incredible", (0.9, 0.8));
        positive.insert("genius", (0.9, 0.7));
        positive.insert("elegant", (0.7, 0.3));
        positive.insert("simple", (0.5, 0.1));
        positive.insert("clean", (0.5, 0.2));

        let mut negative = HashMap::new();
        negative.insert("bad", (-0.6, 0.4));
        negative.insert("wrong", (-0.7, 0.5));
        negative.insert("error", (-0.6, 0.6));
        negative.insert("broken", (-0.7, 0.5));
        negative.insert("bug", (-0.5, 0.5));
        negative.insert("fail", (-0.7, 0.6));
        negative.insert("failed", (-0.7, 0.6));
        negative.insert("crash", (-0.8, 0.7));
        negative.insert("slow", (-0.4, 0.3));
        negative.insert("ugly", (-0.5, 0.3));
        negative.insert("terrible", (-0.9, 0.7));
        negative.insert("horrible", (-0.9, 0.7));
        negative.insert("awful", (-0.8, 0.6));
        negative.insert("hate", (-0.9, 0.7));
        negative.insert("useless", (-0.8, 0.5));
        negative.insert("stupid", (-0.7, 0.6));
        negative.insert("annoying", (-0.6, 0.6));
        negative.insert("frustrated", (-0.7, 0.7));
        negative.insert("confusing", (-0.5, 0.5));
        negative.insert("confused", (-0.5, 0.5));
        negative.insert("stuck", (-0.6, 0.6));
        negative.insert("impossible", (-0.7, 0.6));
        negative.insert("never", (-0.4, 0.3));
        negative.insert("problem", (-0.4, 0.5));
        negative.insert("issue", (-0.3, 0.4));
        negative.insert("mess", (-0.6, 0.5));
        negative.insert("waste", (-0.6, 0.4));
        negative.insert("garbage", (-0.8, 0.5));
        negative.insert("disappointed", (-0.7, 0.3));
        negative.insert("worse", (-0.6, 0.5));

        let mut intensifiers = HashMap::new();
        intensifiers.insert("very", 1.5);
        intensifiers.insert("really", 1.4);
        intensifiers.insert("extremely", 1.8);
        intensifiers.insert("incredibly", 1.7);
        intensifiers.insert("absolutely", 1.6);
        intensifiers.insert("totally", 1.5);
        intensifiers.insert("completely", 1.5);
        intensifiers.insert("so", 1.3);
        intensifiers.insert("super", 1.5);
        intensifiers.insert("quite", 1.2);
        intensifiers.insert("rather", 1.1);
        intensifiers.insert("pretty", 1.2);

        let negators = vec![
            "not",
            "no",
            "never",
            "neither",
            "nor",
            "don't",
            "doesn't",
            "didn't",
            "won't",
            "wouldn't",
            "shouldn't",
            "can't",
            "cannot",
            "isn't",
            "aren't",
            "wasn't",
            "weren't",
        ];

        let urgency_markers = vec![
            "urgent",
            "asap",
            "immediately",
            "now",
            "hurry",
            "quickly",
            "deadline",
            "critical",
            "emergency",
            "rush",
            "fast",
            "soon",
            "priority",
            "blocking",
            "blocker",
        ];

        let frustration_markers = vec![
            "still",
            "again",
            "already",
            "yet",
            "why",
            "how come",
            "keeps",
            "keep",
            "always",
            "every time",
            "same",
            "not working",
            "doesn't work",
            "won't work",
        ];

        let satisfaction_markers = vec![
            "finally",
            "at last",
            "it works",
            "nailed it",
            "got it",
            "makes sense",
            "understood",
            "clear now",
            "sorted",
        ];

        Self {
            positive,
            negative,
            intensifiers,
            negators,
            urgency_markers,
            frustration_markers,
            satisfaction_markers,
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// EMOTION ENGINE
// ═══════════════════════════════════════════════════════════════

pub struct EmotionEngine {
    state: EmotionalState,
    config: EmotionConfig,
    lexicon: SentimentLexicon,
    /// Emotional memory: topic -> average emotional response
    affect_memory: HashMap<String, (f64, f64, u32)>, // (cum_valence, cum_arousal, count)
    /// History of recent stimuli for momentum calculation
    stimulus_history: Vec<(f64, f64, f64)>, // (valence_impulse, arousal_impulse, timestamp_ms)
    /// Total ticks processed
    tick_count: u64,
    /// Simple PRNG state (xorshift64) for deterministic noise
    rng_state: u64,
}

impl EmotionEngine {
    pub fn new(config: EmotionConfig) -> Self {
        Self {
            state: EmotionalState::neutral(),
            config,
            lexicon: SentimentLexicon::new(),
            affect_memory: HashMap::new(),
            stimulus_history: Vec::new(),
            tick_count: 0,
            rng_state: 0xDEAD_BEEF_CAFE_BABE,
        }
    }

    /// Deterministic pseudo-random noise in [-1.0, 1.0]
    fn next_noise(&mut self) -> f64 {
        self.rng_state ^= self.rng_state << 13;
        self.rng_state ^= self.rng_state >> 7;
        self.rng_state ^= self.rng_state << 17;
        ((self.rng_state as f64) / (u64::MAX as f64)) * 2.0 - 1.0
    }

    /// Process a stimulus and update the emotional state.
    pub fn process_stimulus(&mut self, stimulus: &EmotionalStimulus) -> EmotionResult {
        let start = Instant::now();
        self.tick_count += 1;
        let mut audit = Vec::new();

        let now_ms = start.elapsed().as_secs_f64() * 1000.0 + self.tick_count as f64;

        audit.push(format!(
            "Tick {}: stimulus from {:?}, v_impulse={:.3}, a_impulse={:.3}",
            self.tick_count, stimulus.source, stimulus.valence_impulse, stimulus.arousal_impulse
        ));

        // 1. Apply stimulus impulse (scaled by sensitivity)
        let v_impulse = stimulus.valence_impulse * self.config.sensitivity;
        let a_impulse = stimulus.arousal_impulse * self.config.sensitivity;

        // 2. Calculate homeostatic restoring force
        let v_home =
            -self.config.homeostatic_decay * (self.state.valence - self.config.baseline_valence);
        let a_home =
            -self.config.homeostatic_decay * (self.state.arousal - self.config.baseline_arousal);

        // 3. Add stochastic noise
        let v_noise = self.next_noise() * self.config.noise_amplitude;
        let a_noise = self.next_noise() * self.config.noise_amplitude;

        // 4. Calculate momentum from recent history
        let (v_momentum, a_momentum) = self.calculate_momentum();

        // 5. Integrate: dE/dt = F_stimulus + F_homeostatic + F_momentum + noise
        let dv = v_impulse + v_home + v_momentum * 0.1 + v_noise;
        let da = a_impulse + a_home + a_momentum * 0.1 + a_noise;

        // 6. Update velocities (with damping)
        self.state.valence_velocity = self.state.valence_velocity * 0.8 + dv;
        self.state.arousal_velocity = self.state.arousal_velocity * 0.8 + da;

        // 7. Update state
        self.state.valence += self.state.valence_velocity;
        self.state.arousal += self.state.arousal_velocity;

        // 8. Clamp to max intensity
        let intensity = self.state.intensity();
        if intensity > self.config.max_intensity {
            let scale = self.config.max_intensity / intensity;
            self.state.valence *= scale;
            self.state.arousal *= scale;
            audit.push("Clamped intensity to max".into());
        }

        // 9. Hard clamp to [-1, 1]
        self.state.valence = self.state.valence.clamp(-1.0, 1.0);
        self.state.arousal = self.state.arousal.clamp(-1.0, 1.0);

        self.state.last_update_ms = now_ms;

        // 10. Record in history
        self.stimulus_history.push((v_impulse, a_impulse, now_ms));
        if self.stimulus_history.len() > 100 {
            self.stimulus_history.drain(0..50);
        }

        let category = self.state.classify();
        let strategy_bias = self.compute_strategy_bias(&category);

        audit.push(format!(
            "State: v={:.3}, a={:.3}, category={}, intensity={:.3}",
            self.state.valence,
            self.state.arousal,
            category.as_str(),
            self.state.intensity()
        ));

        EmotionResult {
            state: self.state.clone(),
            category,
            strategy_bias,
            user_sentiment: None,
            audit_trail: audit,
            duration_ms: start.elapsed().as_secs_f64() * 1000.0,
        }
    }

    /// Analyze user text for emotional signals and generate appropriate stimulus.
    pub fn analyze_user_text(&mut self, text: &str) -> EmotionResult {
        let start = Instant::now();
        let mut audit = Vec::new();

        let sentiment = self.compute_sentiment(text);
        audit.push(format!(
            "User sentiment: v={:.3}, a={:.3}, conf={:.3}",
            sentiment.valence, sentiment.arousal, sentiment.confidence
        ));

        // Generate stimulus from user sentiment
        let source = if sentiment.valence > 0.3 {
            StimulusSource::UserSatisfaction
        } else if sentiment.valence < -0.3 {
            StimulusSource::UserFrustration
        } else if sentiment.arousal > 0.5 {
            StimulusSource::UserUrgency
        } else {
            StimulusSource::UserSatisfaction
        };

        let stimulus = EmotionalStimulus {
            source,
            valence_impulse: sentiment.valence * 0.3 * sentiment.confidence,
            arousal_impulse: sentiment.arousal * 0.3 * sentiment.confidence,
            persistence: 0.5,
        };

        let mut result = self.process_stimulus(&stimulus);
        result.user_sentiment = Some(sentiment);
        result.audit_trail.extend(audit);
        result.duration_ms = start.elapsed().as_secs_f64() * 1000.0;
        result
    }

    /// Record an emotional association with a topic for persistent memory.
    pub fn record_affect(&mut self, topic: &str, valence: f64, arousal: f64) {
        let entry = self
            .affect_memory
            .entry(topic.to_lowercase())
            .or_insert((0.0, 0.0, 0));
        entry.0 += valence;
        entry.1 += arousal;
        entry.2 += 1;
    }

    /// Recall average emotional association with a topic.
    pub fn recall_affect(&self, topic: &str) -> Option<(f64, f64)> {
        self.affect_memory
            .get(&topic.to_lowercase())
            .map(|(v, a, c)| {
                let count = *c as f64;
                (v / count, a / count)
            })
    }

    /// Get current emotional state (read-only).
    pub fn current_state(&self) -> &EmotionalState {
        &self.state
    }

    /// Get statistics.
    pub fn get_stats(&self) -> serde_json::Value {
        serde_json::json!({
            "engine": "EmotionEngine v1.0 (Dimension 6)",
            "tick_count": self.tick_count,
            "current_state": {
                "valence": self.state.valence,
                "arousal": self.state.arousal,
                "category": self.state.classify().as_str(),
                "intensity": self.state.intensity(),
            },
            "affect_memory_entries": self.affect_memory.len(),
            "stimulus_history_len": self.stimulus_history.len(),
        })
    }

    // ─────────────────────────────────────────────────────
    // INTERNAL METHODS
    // ─────────────────────────────────────────────────────

    fn calculate_momentum(&self) -> (f64, f64) {
        if self.stimulus_history.len() < 2 {
            return (0.0, 0.0);
        }

        let recent = &self.stimulus_history[self.stimulus_history.len().saturating_sub(10)..];
        let n = recent.len() as f64;
        if n < 2.0 {
            return (0.0, 0.0);
        }

        // Simple linear trend of recent impulses
        let v_sum: f64 = recent.iter().map(|(v, _, _)| v).sum();
        let a_sum: f64 = recent.iter().map(|(_, a, _)| a).sum();

        (v_sum / n, a_sum / n)
    }

    fn compute_strategy_bias(&self, category: &EmotionCategory) -> StrategyBias {
        // Map emotional categories to D1-D5 weight modifiers
        match category {
            EmotionCategory::Serenity | EmotionCategory::Contentment => StrategyBias {
                deductive_weight: 1.3, // calm = deep logical thinking
                creative_weight: 1.0,
                impossible_weight: 0.8,
                metacognitive_weight: 0.9,
                temporal_weight: 1.0,
                urgency: 0.2,
            },
            EmotionCategory::Excitement | EmotionCategory::Elation => StrategyBias {
                deductive_weight: 0.9,
                creative_weight: 1.5, // excitement = creative exploration
                impossible_weight: 1.1,
                metacognitive_weight: 0.8,
                temporal_weight: 1.2,
                urgency: 0.5,
            },
            EmotionCategory::Frustration => StrategyBias {
                deductive_weight: 0.8,
                creative_weight: 1.0,
                impossible_weight: 1.6, // frustration = break the rules
                metacognitive_weight: 1.3,
                temporal_weight: 1.1,
                urgency: 0.8,
            },
            EmotionCategory::Tension | EmotionCategory::Concern => StrategyBias {
                deductive_weight: 1.0,
                creative_weight: 0.8,
                impossible_weight: 1.0,
                metacognitive_weight: 1.5, // tension = self-check
                temporal_weight: 1.2,
                urgency: 0.7,
            },
            EmotionCategory::Alertness => StrategyBias {
                deductive_weight: 1.1,
                creative_weight: 1.0,
                impossible_weight: 1.0,
                metacognitive_weight: 1.0,
                temporal_weight: 1.4, // alert = plan ahead
                urgency: 0.6,
            },
            EmotionCategory::Boredom => StrategyBias {
                deductive_weight: 0.7,
                creative_weight: 1.4, // boredom = seek novelty
                impossible_weight: 1.2,
                metacognitive_weight: 1.0,
                temporal_weight: 0.8,
                urgency: 0.1,
            },
            EmotionCategory::Dejection => StrategyBias {
                deductive_weight: 1.0,
                creative_weight: 0.6,
                impossible_weight: 0.8,
                metacognitive_weight: 1.6, // dejection = strong self-correction
                temporal_weight: 1.0,
                urgency: 0.3,
            },
        }
    }

    /// Lexicon-based sentiment analysis with negation handling and intensifiers.
    fn compute_sentiment(&self, text: &str) -> SentimentResult {
        let words: Vec<&str> = text
            .split_whitespace()
            .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
            .filter(|w| !w.is_empty())
            .collect();

        let mut total_valence = 0.0;
        let mut total_arousal = 0.0;
        let mut hit_count = 0u32;
        let mut detected = Vec::new();
        let mut negate_next = false;
        let mut intensifier_next = 1.0f64;

        for word in &words {
            let lower = word.to_lowercase();
            let lower_str = lower.as_str();

            // Check for negators
            if self.lexicon.negators.contains(&lower_str) {
                negate_next = true;
                continue;
            }

            // Check for intensifiers
            if let Some(&mult) = self.lexicon.intensifiers.get(lower_str) {
                intensifier_next = mult;
                continue;
            }

            // Check positive words
            if let Some(&(v, a)) = self.lexicon.positive.get(lower_str) {
                let final_v = if negate_next { -v } else { v } * intensifier_next;
                let final_a = a * intensifier_next;
                total_valence += final_v;
                total_arousal += final_a;
                hit_count += 1;
                detected.push(format!("{}(v={:.2})", lower, final_v));
                negate_next = false;
                intensifier_next = 1.0;
                continue;
            }

            // Check negative words
            if let Some(&(v, a)) = self.lexicon.negative.get(lower_str) {
                let final_v = if negate_next { -v } else { v } * intensifier_next;
                let final_a = a * intensifier_next;
                total_valence += final_v;
                total_arousal += final_a;
                hit_count += 1;
                detected.push(format!("{}(v={:.2})", lower, final_v));
                negate_next = false;
                intensifier_next = 1.0;
                continue;
            }

            // Reset modifiers on non-sentiment words
            negate_next = false;
            intensifier_next = 1.0;
        }

        // Check urgency markers
        let urgency_count = words
            .iter()
            .filter(|w| {
                self.lexicon
                    .urgency_markers
                    .contains(&w.to_lowercase().as_str())
            })
            .count();
        if urgency_count > 0 {
            total_arousal += 0.3 * urgency_count as f64;
            detected.push(format!("urgency_signals={}", urgency_count));
        }

        // Check frustration markers
        let frustration_count = words
            .iter()
            .filter(|w| {
                self.lexicon
                    .frustration_markers
                    .contains(&w.to_lowercase().as_str())
            })
            .count();
        if frustration_count > 0 {
            total_valence -= 0.2 * frustration_count as f64;
            total_arousal += 0.2 * frustration_count as f64;
            detected.push(format!("frustration_signals={}", frustration_count));
        }

        // Check punctuation patterns
        let exclamation_count = text.chars().filter(|c| *c == '!').count();
        let question_count = text.chars().filter(|c| *c == '?').count();
        let caps_ratio = if !text.is_empty() {
            text.chars().filter(|c| c.is_uppercase()).count() as f64 / text.len() as f64
        } else {
            0.0
        };

        if exclamation_count > 1 {
            total_arousal += 0.2 * exclamation_count.min(3) as f64;
            detected.push(format!("exclamations={}", exclamation_count));
        }
        if question_count > 2 {
            total_arousal += 0.1;
            total_valence -= 0.1; // multiple questions = confusion
            detected.push("multiple_questions".into());
        }
        if caps_ratio > 0.5 && text.len() > 5 {
            total_arousal += 0.3;
            detected.push("CAPS_DETECTED".into());
        }

        // Normalize
        let normalizer = (hit_count as f64).max(1.0);
        let confidence = (hit_count as f64 / words.len().max(1) as f64).clamp(0.0, 1.0);

        SentimentResult {
            valence: (total_valence / normalizer).clamp(-1.0, 1.0),
            arousal: (total_arousal / normalizer).clamp(-1.0, 1.0),
            confidence,
            detected_signals: detected,
        }
    }
}

impl Default for EmotionEngine {
    fn default() -> Self {
        Self::new(EmotionConfig::default())
    }
}

// ═══════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_neutral_state() {
        let state = EmotionalState::neutral();
        assert!(state.intensity() < 0.3);
        assert_eq!(state.classify(), EmotionCategory::Alertness);
    }

    #[test]
    fn test_positive_stimulus() {
        let mut engine = EmotionEngine::default();
        let stimulus = EmotionalStimulus {
            source: StimulusSource::SolutionFound,
            valence_impulse: 0.5,
            arousal_impulse: 0.3,
            persistence: 0.5,
        };
        let result = engine.process_stimulus(&stimulus);
        assert!(result.state.valence > 0.0);
    }

    #[test]
    fn test_negative_stimulus() {
        let mut engine = EmotionEngine::default();
        let stimulus = EmotionalStimulus {
            source: StimulusSource::SolutionFailed,
            valence_impulse: -0.6,
            arousal_impulse: 0.4,
            persistence: 0.5,
        };
        let result = engine.process_stimulus(&stimulus);
        assert!(result.state.valence < 0.0);
    }

    #[test]
    fn test_homeostatic_recovery() {
        let mut engine = EmotionEngine::default();
        // Push to extreme
        let stimulus = EmotionalStimulus {
            source: StimulusSource::SolutionFailed,
            valence_impulse: -0.8,
            arousal_impulse: 0.8,
            persistence: 0.5,
        };
        engine.process_stimulus(&stimulus);

        // Apply neutral stimuli repeatedly — should recover toward baseline
        for _ in 0..50 {
            engine.process_stimulus(&EmotionalStimulus {
                source: StimulusSource::MemoryRecall,
                valence_impulse: 0.0,
                arousal_impulse: 0.0,
                persistence: 0.0,
            });
        }
        // Should be closer to baseline than the initial extreme
        assert!(engine.state.valence.abs() < 0.5);
    }

    #[test]
    fn test_sentiment_positive() {
        let engine = EmotionEngine::default();
        let result = engine.compute_sentiment("This is really great and amazing work!");
        assert!(result.valence > 0.0);
        assert!(result.confidence > 0.0);
    }

    #[test]
    fn test_sentiment_negative() {
        let engine = EmotionEngine::default();
        let result = engine.compute_sentiment("This is terrible and broken, I hate it");
        assert!(result.valence < 0.0);
    }

    #[test]
    fn test_sentiment_negation() {
        let engine = EmotionEngine::default();
        let result = engine.compute_sentiment("This is not good at all");
        assert!(result.valence < 0.0); // "not good" should be negative
    }

    #[test]
    fn test_frustration_boosts_impossible_engine() {
        let mut engine = EmotionEngine::default();
        // Simulate repeated failures
        for _ in 0..5 {
            engine.process_stimulus(&EmotionalStimulus {
                source: StimulusSource::SolutionFailed,
                valence_impulse: -0.4,
                arousal_impulse: 0.3,
                persistence: 0.5,
            });
        }
        let result = engine.process_stimulus(&EmotionalStimulus {
            source: StimulusSource::SolutionFailed,
            valence_impulse: -0.4,
            arousal_impulse: 0.3,
            persistence: 0.5,
        });
        // Frustration should boost the Impossible Engine weight
        assert!(result.strategy_bias.impossible_weight > 1.0);
    }

    #[test]
    fn test_intensity_clamping() {
        let mut engine = EmotionEngine::default();
        // Massive stimulus
        engine.process_stimulus(&EmotionalStimulus {
            source: StimulusSource::NoveltyDetected,
            valence_impulse: 10.0,
            arousal_impulse: 10.0,
            persistence: 1.0,
        });
        assert!(engine.state.intensity() <= 1.0 + 0.01); // within float tolerance
    }

    #[test]
    fn test_affect_memory() {
        let mut engine = EmotionEngine::default();
        engine.record_affect("rust programming", 0.8, 0.6);
        engine.record_affect("rust programming", 0.6, 0.4);
        let recalled = engine.recall_affect("rust programming").unwrap();
        assert!((recalled.0 - 0.7).abs() < 0.01);
        assert!((recalled.1 - 0.5).abs() < 0.01);
    }
}
