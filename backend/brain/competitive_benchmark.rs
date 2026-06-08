// ─────────────────────────────────────────────────────────────
// Competitive Benchmark Engine — Self-Benchmarking
// ─────────────────────────────────────────────────────────────
// Port of backend/brain/competitive_benchmark_engine.py

use std::collections::HashMap;
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct BenchmarkResult {
    pub test_name: String,
    pub score: f64,
    pub duration_ms: f64,
    pub passed: bool,
    pub details: String,
}

#[derive(Debug, Clone)]
pub struct BenchmarkSuite {
    pub name: String,
    pub results: Vec<BenchmarkResult>,
    pub total_score: f64,
    pub pass_rate: f64,
    pub total_duration_ms: f64,
    pub regressions: Vec<String>,
}

/// Test case for benchmarking.
#[derive(Debug, Clone)]
pub struct TestCase {
    pub name: String,
    pub input: String,
    pub expected: String,
    pub domain: String,
    pub difficulty: f64,
}

/// Competitive Benchmark Engine — self-benchmarking with regression detection.
pub struct CompetitiveBenchmarkEngine {
    test_suites: HashMap<String, Vec<TestCase>>,
    history: Vec<BenchmarkSuite>,
    baseline_scores: HashMap<String, f64>,
}

impl CompetitiveBenchmarkEngine {
    pub fn new() -> Self {
        let mut engine = Self {
            test_suites: HashMap::new(),
            history: Vec::new(),
            baseline_scores: HashMap::new(),
        };
        engine.init_default_suites();
        engine
    }

    fn init_default_suites(&mut self) {
        // Math benchmark suite
        let math_tests = vec![
            TestCase {
                name: "addition".into(),
                input: "15 + 23".into(),
                expected: "38".into(),
                domain: "math".into(),
                difficulty: 0.1,
            },
            TestCase {
                name: "multiplication".into(),
                input: "12 * 15".into(),
                expected: "180".into(),
                domain: "math".into(),
                difficulty: 0.2,
            },
            TestCase {
                name: "prime_check".into(),
                input: "is 17 prime".into(),
                expected: "prime".into(),
                domain: "math".into(),
                difficulty: 0.3,
            },
            TestCase {
                name: "fibonacci".into(),
                input: "fibonacci 5".into(),
                expected: "0, 1, 1, 2, 3".into(),
                domain: "math".into(),
                difficulty: 0.3,
            },
            TestCase {
                name: "factorial".into(),
                input: "factorial 5".into(),
                expected: "120".into(),
                domain: "math".into(),
                difficulty: 0.2,
            },
        ];
        self.test_suites.insert("math".into(), math_tests);

        // Logic benchmark suite
        let logic_tests = vec![
            TestCase {
                name: "syllogism".into(),
                input: "all humans are mortal. all greeks are humans.".into(),
                expected: "all greeks are mortal".into(),
                domain: "logic".into(),
                difficulty: 0.4,
            },
            TestCase {
                name: "boolean_and".into(),
                input: "true and false".into(),
                expected: "false".into(),
                domain: "logic".into(),
                difficulty: 0.1,
            },
        ];
        self.test_suites.insert("logic".into(), logic_tests);
    }

    /// Run a benchmark suite.
    pub fn run_suite<F>(&mut self, suite_name: &str, solver: F) -> BenchmarkSuite
    where
        F: Fn(&str) -> (String, f64),
    {
        let start = Instant::now();
        let tests = self
            .test_suites
            .get(suite_name)
            .cloned()
            .unwrap_or_default();
        let mut results = Vec::new();

        for test in &tests {
            let test_start = Instant::now();
            let (answer, confidence) = solver(&test.input);

            let passed = answer
                .to_lowercase()
                .contains(&test.expected.to_lowercase());
            let score = if passed {
                confidence * (1.0 + test.difficulty)
            } else {
                0.0
            };

            results.push(BenchmarkResult {
                test_name: test.name.clone(),
                score,
                duration_ms: test_start.elapsed().as_secs_f64() * 1000.0,
                passed,
                details: format!(
                    "Expected '{}', got '{}'",
                    test.expected,
                    &answer.chars().take(100).collect::<String>()
                ),
            });
        }

        let total_score: f64 =
            results.iter().map(|r| r.score).sum::<f64>() / results.len().max(1) as f64;
        let pass_rate =
            results.iter().filter(|r| r.passed).count() as f64 / results.len().max(1) as f64;

        // Detect regressions
        let mut regressions = Vec::new();
        if let Some(baseline) = self.baseline_scores.get(suite_name) {
            if total_score < *baseline * 0.9 {
                regressions.push(format!(
                    "Regression in '{}': baseline={:.3} current={:.3} (Δ={:.3})",
                    suite_name,
                    baseline,
                    total_score,
                    total_score - baseline
                ));
            }
        }

        // Update baseline
        self.baseline_scores
            .insert(suite_name.to_string(), total_score);

        let suite = BenchmarkSuite {
            name: suite_name.to_string(),
            results,
            total_score,
            pass_rate,
            total_duration_ms: start.elapsed().as_secs_f64() * 1000.0,
            regressions,
        };

        self.history.push(suite.clone());
        suite
    }

    /// Add a custom test case to a suite.
    pub fn add_test(&mut self, suite: &str, test: TestCase) {
        self.test_suites
            .entry(suite.to_string())
            .or_default()
            .push(test);
    }

    /// Get performance trend for a suite.
    pub fn trend(&self, suite_name: &str) -> Vec<f64> {
        self.history
            .iter()
            .filter(|s| s.name == suite_name)
            .map(|s| s.total_score)
            .collect()
    }

    pub fn available_suites(&self) -> Vec<&String> {
        self.test_suites.keys().collect()
    }
}

impl Default for CompetitiveBenchmarkEngine {
    fn default() -> Self {
        Self::new()
    }
}
