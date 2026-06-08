// ─────────────────────────────────────────────────────────────
// Program Synthesis Engine — Code Generation from I/O Examples
// ─────────────────────────────────────────────────────────────
// Port of backend/brain/program_synthesis_engine.py

/// I/O example pair for program synthesis.
#[derive(Debug, Clone)]
pub struct IOExample {
    pub input: Vec<String>,
    pub output: String,
}

/// Result of program synthesis.
#[derive(Debug, Clone)]
pub struct SynthesisResult {
    pub program: String,
    pub description: String,
    pub confidence: f64,
    pub verified: bool,
}

/// Program Synthesis Engine — generates code from I/O examples.
pub struct ProgramSynthesisEngine;

impl ProgramSynthesisEngine {
    pub fn new() -> Self {
        Self
    }

    /// Synthesize a program from I/O examples.
    pub fn synthesize(&self, examples: &[IOExample]) -> SynthesisResult {
        if examples.is_empty() {
            return SynthesisResult {
                program: String::new(),
                description: "No examples provided".into(),
                confidence: 0.0,
                verified: false,
            };
        }

        // Try simple pattern matching first
        if let Some(result) = self.try_constant_function(examples) {
            return result;
        }
        if let Some(result) = self.try_identity_function(examples) {
            return result;
        }
        if let Some(result) = self.try_string_transformation(examples) {
            return result;
        }
        if let Some(result) = self.try_arithmetic_function(examples) {
            return result;
        }
        if let Some(result) = self.try_list_operation(examples) {
            return result;
        }

        SynthesisResult {
            program: "// Could not synthesize a matching program".into(),
            description: "No pattern detected in examples".into(),
            confidence: 0.0,
            verified: false,
        }
    }

    fn try_constant_function(&self, examples: &[IOExample]) -> Option<SynthesisResult> {
        let first_output = &examples[0].output;
        if examples.iter().all(|e| &e.output == first_output) {
            Some(SynthesisResult {
                program: format!(
                    "fn f(_input: &str) -> String {{ \"{}\".to_string() }}",
                    first_output
                ),
                description: format!("Constant function returning '{}'", first_output),
                confidence: 0.95,
                verified: true,
            })
        } else {
            None
        }
    }

    fn try_identity_function(&self, examples: &[IOExample]) -> Option<SynthesisResult> {
        if examples
            .iter()
            .all(|e| e.input.len() == 1 && e.input[0] == e.output)
        {
            Some(SynthesisResult {
                program: "fn f(input: &str) -> String { input.to_string() }".into(),
                description: "Identity function".into(),
                confidence: 0.95,
                verified: true,
            })
        } else {
            None
        }
    }

    fn try_string_transformation(&self, examples: &[IOExample]) -> Option<SynthesisResult> {
        // Check: to_uppercase
        if examples
            .iter()
            .all(|e| e.input.len() == 1 && e.input[0].to_uppercase() == e.output)
        {
            return Some(SynthesisResult {
                program: "fn f(input: &str) -> String { input.to_uppercase() }".into(),
                description: "Uppercase transformation".into(),
                confidence: 0.90,
                verified: true,
            });
        }
        // Check: to_lowercase
        if examples
            .iter()
            .all(|e| e.input.len() == 1 && e.input[0].to_lowercase() == e.output)
        {
            return Some(SynthesisResult {
                program: "fn f(input: &str) -> String { input.to_lowercase() }".into(),
                description: "Lowercase transformation".into(),
                confidence: 0.90,
                verified: true,
            });
        }
        // Check: reverse
        if examples
            .iter()
            .all(|e| e.input.len() == 1 && e.input[0].chars().rev().collect::<String>() == e.output)
        {
            return Some(SynthesisResult {
                program: "fn f(input: &str) -> String { input.chars().rev().collect() }".into(),
                description: "String reversal".into(),
                confidence: 0.90,
                verified: true,
            });
        }
        // Check: length
        if examples
            .iter()
            .all(|e| e.input.len() == 1 && e.input[0].len().to_string() == e.output)
        {
            return Some(SynthesisResult {
                program: "fn f(input: &str) -> String { input.len().to_string() }".into(),
                description: "String length".into(),
                confidence: 0.90,
                verified: true,
            });
        }
        // Check: concatenation
        if examples
            .iter()
            .all(|e| e.input.len() == 2 && format!("{}{}", e.input[0], e.input[1]) == e.output)
        {
            return Some(SynthesisResult {
                program: "fn f(a: &str, b: &str) -> String { format!(\"{}{}\", a, b) }".into(),
                description: "String concatenation".into(),
                confidence: 0.90,
                verified: true,
            });
        }
        None
    }

    fn try_arithmetic_function(&self, examples: &[IOExample]) -> Option<SynthesisResult> {
        // Check if input/output are numbers
        let numeric: Vec<(Vec<f64>, f64)> = examples
            .iter()
            .filter_map(|e| {
                let inputs: Option<Vec<f64>> = e.input.iter().map(|i| i.parse().ok()).collect();
                let output: f64 = e.output.parse().ok()?;
                Some((inputs?, output))
            })
            .collect();

        if numeric.len() != examples.len() {
            return None;
        }

        // Single input: try common functions
        if numeric[0].0.len() == 1 {
            // Double
            if numeric.iter().all(|(i, o)| (*o - i[0] * 2.0).abs() < 1e-9) {
                return Some(SynthesisResult {
                    program: "fn f(x: f64) -> f64 { x * 2.0 }".into(),
                    description: "Double the input".into(),
                    confidence: 0.90,
                    verified: true,
                });
            }
            // Square
            if numeric.iter().all(|(i, o)| (*o - i[0] * i[0]).abs() < 1e-9) {
                return Some(SynthesisResult {
                    program: "fn f(x: f64) -> f64 { x * x }".into(),
                    description: "Square the input".into(),
                    confidence: 0.90,
                    verified: true,
                });
            }
            // Try linear: y = ax + b
            if numeric.len() >= 2 {
                let (x1, y1) = (numeric[0].0[0], numeric[0].1);
                let (x2, y2) = (numeric[1].0[0], numeric[1].1);
                if (x2 - x1).abs() > 1e-9 {
                    let a = (y2 - y1) / (x2 - x1);
                    let b = y1 - a * x1;
                    if numeric
                        .iter()
                        .all(|(i, o)| (*o - (a * i[0] + b)).abs() < 1e-6)
                    {
                        return Some(SynthesisResult {
                            program: format!("fn f(x: f64) -> f64 {{ {:.4} * x + {:.4} }}", a, b),
                            description: format!("Linear function: y = {:.4}x + {:.4}", a, b),
                            confidence: 0.85,
                            verified: true,
                        });
                    }
                }
            }
        }

        // Two inputs: try addition, multiplication
        if numeric[0].0.len() == 2 {
            if numeric
                .iter()
                .all(|(i, o)| (*o - (i[0] + i[1])).abs() < 1e-9)
            {
                return Some(SynthesisResult {
                    program: "fn f(a: f64, b: f64) -> f64 { a + b }".into(),
                    description: "Sum of two inputs".into(),
                    confidence: 0.90,
                    verified: true,
                });
            }
            if numeric
                .iter()
                .all(|(i, o)| (*o - (i[0] * i[1])).abs() < 1e-9)
            {
                return Some(SynthesisResult {
                    program: "fn f(a: f64, b: f64) -> f64 { a * b }".into(),
                    description: "Product of two inputs".into(),
                    confidence: 0.90,
                    verified: true,
                });
            }
        }

        None
    }

    fn try_list_operation(&self, _examples: &[IOExample]) -> Option<SynthesisResult> {
        // Placeholder for list operations - could detect sort, filter, map patterns
        None
    }
}

impl Default for ProgramSynthesisEngine {
    fn default() -> Self {
        Self::new()
    }
}
