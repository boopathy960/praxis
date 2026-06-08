// ─────────────────────────────────────────────────────────────
// Calculator — Mathematical Computation Tool
// ─────────────────────────────────────────────────────────────
// Port of backend/agents/tools/calculator.py

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub enum MathOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    Sqrt,
    Log,
    Ln,
    Sin,
    Cos,
    Tan,
    Abs,
    Floor,
    Ceil,
    Round,
    Mod,
    Factorial,
    Gcd,
    Lcm,
}

#[derive(Debug, Clone)]
pub struct CalcResult {
    pub expression: String,
    pub result: f64,
    pub steps: Vec<String>,
    pub error: Option<String>,
}

/// Mathematical computation tool with expression evaluation and step tracking.
pub struct Calculator {
    constants: HashMap<String, f64>,
    memory: HashMap<String, f64>,
    history: Vec<CalcResult>,
}

impl Calculator {
    pub fn new() -> Self {
        let mut constants = HashMap::new();
        constants.insert("pi".into(), std::f64::consts::PI);
        constants.insert("e".into(), std::f64::consts::E);
        constants.insert("tau".into(), std::f64::consts::TAU);
        constants.insert("phi".into(), 1.618033988749895);
        constants.insert("sqrt2".into(), std::f64::consts::SQRT_2);

        Self {
            constants,
            memory: HashMap::new(),
            history: Vec::new(),
        }
    }

    /// Evaluate a mathematical operation.
    pub fn compute(&mut self, op: MathOp, args: &[f64]) -> CalcResult {
        let (expr, result, steps) = match op {
            MathOp::Add => {
                let r: f64 = args.iter().sum();
                (
                    format!("sum({:?})", args),
                    r,
                    vec![format!("{:?} = {}", args, r)],
                )
            }
            MathOp::Sub => {
                let r = args.get(0).unwrap_or(&0.0) - args.get(1).unwrap_or(&0.0);
                (
                    format!(
                        "{} - {}",
                        args.get(0).unwrap_or(&0.0),
                        args.get(1).unwrap_or(&0.0)
                    ),
                    r,
                    vec![],
                )
            }
            MathOp::Mul => {
                let r: f64 = args.iter().product();
                (format!("product({:?})", args), r, vec![])
            }
            MathOp::Div => {
                let a = *args.get(0).unwrap_or(&0.0);
                let b = *args.get(1).unwrap_or(&1.0);
                if b == 0.0 {
                    let res = CalcResult {
                        expression: format!("{} / {}", a, b),
                        result: f64::NAN,
                        steps: vec![],
                        error: Some("Division by zero".into()),
                    };
                    self.history.push(res.clone());
                    return res;
                }
                (format!("{} / {}", a, b), a / b, vec![])
            }
            MathOp::Pow => {
                let base = *args.get(0).unwrap_or(&0.0);
                let exp = *args.get(1).unwrap_or(&1.0);
                (format!("{}^{}", base, exp), base.powf(exp), vec![])
            }
            MathOp::Sqrt => {
                let a = *args.get(0).unwrap_or(&0.0);
                (format!("√{}", a), a.sqrt(), vec![])
            }
            MathOp::Log => {
                let a = *args.get(0).unwrap_or(&1.0);
                (format!("log10({})", a), a.log10(), vec![])
            }
            MathOp::Ln => {
                let a = *args.get(0).unwrap_or(&1.0);
                (format!("ln({})", a), a.ln(), vec![])
            }
            MathOp::Sin => {
                let a = *args.get(0).unwrap_or(&0.0);
                (format!("sin({})", a), a.sin(), vec![])
            }
            MathOp::Cos => {
                let a = *args.get(0).unwrap_or(&0.0);
                (format!("cos({})", a), a.cos(), vec![])
            }
            MathOp::Tan => {
                let a = *args.get(0).unwrap_or(&0.0);
                (format!("tan({})", a), a.tan(), vec![])
            }
            MathOp::Abs => {
                let a = *args.get(0).unwrap_or(&0.0);
                (format!("|{}|", a), a.abs(), vec![])
            }
            MathOp::Floor => {
                let a = *args.get(0).unwrap_or(&0.0);
                (format!("⌊{}⌋", a), a.floor(), vec![])
            }
            MathOp::Ceil => {
                let a = *args.get(0).unwrap_or(&0.0);
                (format!("⌈{}⌉", a), a.ceil(), vec![])
            }
            MathOp::Round => {
                let a = *args.get(0).unwrap_or(&0.0);
                (format!("round({})", a), a.round(), vec![])
            }
            MathOp::Mod => {
                let a = *args.get(0).unwrap_or(&0.0);
                let b = *args.get(1).unwrap_or(&1.0);
                (format!("{} mod {}", a, b), a % b, vec![])
            }
            MathOp::Factorial => {
                let n = *args.get(0).unwrap_or(&0.0) as u64;
                let r = (1..=n).product::<u64>() as f64;
                (format!("{}!", n), r, vec![])
            }
            MathOp::Gcd => {
                let a = *args.get(0).unwrap_or(&0.0) as u64;
                let b = *args.get(1).unwrap_or(&0.0) as u64;
                let r = self.gcd(a, b) as f64;
                (format!("gcd({}, {})", a, b), r, vec![])
            }
            MathOp::Lcm => {
                let a = *args.get(0).unwrap_or(&0.0) as u64;
                let b = *args.get(1).unwrap_or(&0.0) as u64;
                let g = self.gcd(a, b);
                let r = if g == 0 { 0 } else { a * b / g } as f64;
                (format!("lcm({}, {})", a, b), r, vec![])
            }
        };

        let res = CalcResult {
            expression: expr,
            result,
            steps,
            error: None,
        };
        self.history.push(res.clone());
        res
    }

    fn gcd(&self, a: u64, b: u64) -> u64 {
        if b == 0 {
            a
        } else {
            self.gcd(b, a % b)
        }
    }

    /// Store a value in memory.
    pub fn store(&mut self, key: &str, value: f64) {
        self.memory.insert(key.to_string(), value);
    }

    /// Recall a value from memory.
    pub fn recall(&self, key: &str) -> Option<f64> {
        self.memory.get(key).copied()
    }

    /// Get a constant by name.
    pub fn constant(&self, name: &str) -> Option<f64> {
        self.constants.get(name).copied()
    }

    pub fn history(&self) -> &[CalcResult] {
        &self.history
    }
}

impl Default for Calculator {
    fn default() -> Self {
        Self::new()
    }
}
