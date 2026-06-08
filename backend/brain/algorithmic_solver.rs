// ─────────────────────────────────────────────────────────────
// Algorithmic Solver Pipeline — CPU-Only Problem Solvers
// ─────────────────────────────────────────────────────────────
// Port of backend/brain/algorithmic_solver.py
// Multi-strategy solver: Math, Code, Logic, Extraction, Planning.
// No GPU, no ML — pure algorithmic computation.

use log::debug;
use std::collections::HashMap;
use std::time::Instant;

/// Result from any solver in the pipeline.
#[derive(Debug, Clone)]
pub struct SolverResult {
    pub answer: String,
    pub confidence: f64,
    pub solver_name: String,
    pub reasoning_trace: Vec<String>,
    pub metadata: HashMap<String, String>,
    pub duration_ms: f64,
}

impl SolverResult {
    pub fn empty(solver_name: &str) -> Self {
        Self {
            answer: String::new(),
            confidence: 0.0,
            solver_name: solver_name.to_string(),
            reasoning_trace: Vec::new(),
            metadata: HashMap::new(),
            duration_ms: 0.0,
        }
    }

    pub fn is_valid(&self) -> bool {
        !self.answer.is_empty() && self.confidence > 0.1
    }
}

// ═══════════════════════════════════════════════════════════
// MATH SOLVER — Symbolic Arithmetic & Algebra
// ═══════════════════════════════════════════════════════════

pub struct MathSolver;

impl Default for MathSolver {
    fn default() -> Self {
        Self
    }
}

impl MathSolver {
    pub fn new() -> Self {
        Self
    }

    pub fn solve(&self, prompt: &str) -> SolverResult {
        let start = Instant::now();
        let mut result = SolverResult::empty("MathSolver");
        let prompt_lower = prompt.to_lowercase();

        // Try direct expression evaluation
        if let Some(expr) = self.extract_expression(prompt) {
            if let Some(value) = self.safe_eval(&expr) {
                result.answer = self.format_math_answer(&expr, value);
                result.confidence = 0.95;
                result.reasoning_trace = vec![
                    format!("Extracted expression: {}", expr),
                    format!("Computed result: {}", value),
                ];
                result.duration_ms = start.elapsed().as_secs_f64() * 1000.0;
                return result;
            }
        }

        // Try fibonacci
        if prompt_lower.contains("fibonacci") {
            let n = Self::extract_number(&prompt_lower).unwrap_or(10).min(50);
            let seq = Self::fibonacci(n);
            let seq_str: Vec<String> = seq.iter().map(|x| x.to_string()).collect();
            result.answer = format!(
                "Fibonacci sequence (first {} numbers):\n{}\n\n\
                 The Fibonacci sequence: F(0)=0, F(1)=1, F(n)=F(n-1)+F(n-2)\n\
                 Each number is the sum of the two preceding ones.",
                seq.len(),
                seq_str.join(", ")
            );
            result.confidence = 0.95;
            result.duration_ms = start.elapsed().as_secs_f64() * 1000.0;
            return result;
        }

        // Try prime check
        if (prompt_lower.contains("is") || prompt_lower.contains("check"))
            && prompt_lower.contains("prime")
        {
            if let Some(n) = Self::extract_number(&prompt_lower) {
                let is_prime = Self::is_prime(n as u64);
                result.answer = format!(
                    "{} is {} number.",
                    n,
                    if is_prime { "a prime" } else { "NOT a prime" }
                );
                if !is_prime && n > 1 {
                    let factors = Self::factorize(n as u64);
                    let factors_str: Vec<String> = factors.iter().map(|x| x.to_string()).collect();
                    result.answer.push_str(&format!(
                        "\nPrime factorization: {} = {}",
                        n,
                        factors_str.join(" × ")
                    ));
                }
                result.confidence = 0.95;
                result.duration_ms = start.elapsed().as_secs_f64() * 1000.0;
                return result;
            }
        }

        // Try statistics
        let numbers = Self::extract_numbers(prompt);
        if numbers.len() >= 3
            && [
                "mean",
                "average",
                "median",
                "sum",
                "statistics",
                "std",
                "variance",
            ]
            .iter()
            .any(|kw| prompt_lower.contains(kw))
        {
            result.answer = Self::compute_statistics(&numbers);
            result.confidence = 0.90;
            result.duration_ms = start.elapsed().as_secs_f64() * 1000.0;
            return result;
        }

        // Try factorial
        if prompt_lower.contains("factorial") || prompt.contains("!") {
            if let Some(n) = Self::extract_number(&prompt_lower) {
                if n <= 170 {
                    let val = Self::factorial(n as u64);
                    result.answer = format!("{}! = {}", n, val);
                    result.confidence = 0.95;
                    result.duration_ms = start.elapsed().as_secs_f64() * 1000.0;
                    return result;
                }
            }
        }

        // Try simple algebra
        if let Some(algebra_answer) = self.solve_linear_equation(prompt) {
            result.answer = algebra_answer;
            result.confidence = 0.85;
            result.duration_ms = start.elapsed().as_secs_f64() * 1000.0;
            return result;
        }

        result.duration_ms = start.elapsed().as_secs_f64() * 1000.0;
        result
    }

    fn extract_expression(&self, text: &str) -> Option<String> {
        // Direct expressions: "15 * 23 + 7"
        let re =
            regex::Regex::new(r"([\d.]+\s*[+\-*/^%]+\s*[\d.]+(?:\s*[+\-*/^%]+\s*[\d.]+)*)").ok()?;
        if let Some(caps) = re.captures(text) {
            let expr = caps.get(1)?.as_str().replace('^', "**").trim().to_string();
            return Some(expr);
        }

        // Word operators: "X times Y"
        let word_ops = [
            ("plus", "+"),
            ("minus", "-"),
            ("times", "*"),
            ("multiplied by", "*"),
            ("divided by", "/"),
        ];
        let text_lower = text.to_lowercase();
        for (word, op) in &word_ops {
            let pattern = format!(r"(\d+\.?\d*)\s+{}\s+(\d+\.?\d*)", regex::escape(word));
            if let Ok(re) = regex::Regex::new(&pattern) {
                if let Some(caps) = re.captures(&text_lower) {
                    let a = caps.get(1)?.as_str();
                    let b = caps.get(2)?.as_str();
                    return Some(format!("{} {} {}", a, op, b));
                }
            }
        }
        None
    }

    fn safe_eval(&self, expr: &str) -> Option<f64> {
        // Simple recursive-descent expression parser (no eval)
        let cleaned = expr.replace("**", "^").replace(" ", "");
        ExprParser::parse(&cleaned)
    }

    fn format_math_answer(&self, expr: &str, value: f64) -> String {
        let formatted = if (value - value.round()).abs() < 1e-10 {
            format!("{}", value as i64)
        } else {
            format!("{:.6}", value)
                .trim_end_matches('0')
                .trim_end_matches('.')
                .to_string()
        };
        format!(
            "**Result**: {} = **{}**\n\n\
             Step-by-step:\n  Expression: {}\n  Evaluation: {}\n\n\
             Computed using safe symbolic arithmetic (parsed, no eval).",
            expr, formatted, expr, formatted
        )
    }

    fn extract_number(text: &str) -> Option<usize> {
        let re = regex::Regex::new(r"(\d+)").ok()?;
        re.captures(text)?.get(1)?.as_str().parse().ok()
    }

    fn extract_numbers(text: &str) -> Vec<f64> {
        let re = regex::Regex::new(r"-?\d+\.?\d*").unwrap();
        re.find_iter(text)
            .filter_map(|m| m.as_str().parse::<f64>().ok())
            .collect()
    }

    fn fibonacci(n: usize) -> Vec<u64> {
        let mut result = Vec::with_capacity(n);
        let (mut a, mut b) = (0u64, 1u64);
        for _ in 0..n {
            result.push(a);
            let temp = a;
            a = b;
            b = temp.saturating_add(b);
        }
        result
    }

    fn is_prime(n: u64) -> bool {
        if n < 2 {
            return false;
        }
        if n < 4 {
            return true;
        }
        if n % 2 == 0 || n % 3 == 0 {
            return false;
        }
        let mut i = 5u64;
        while i * i <= n {
            if n % i == 0 || n % (i + 2) == 0 {
                return false;
            }
            i += 6;
        }
        true
    }

    fn factorize(mut n: u64) -> Vec<u64> {
        let mut factors = Vec::new();
        let mut d = 2u64;
        while d * d <= n {
            while n % d == 0 {
                factors.push(d);
                n /= d;
            }
            d += 1;
        }
        if n > 1 {
            factors.push(n);
        }
        factors
    }

    fn factorial(n: u64) -> u128 {
        (1..=n as u128).product()
    }

    fn compute_statistics(nums: &[f64]) -> String {
        if nums.is_empty() {
            return "No data to analyze.".to_string();
        }
        let n = nums.len() as f64;
        let sum: f64 = nums.iter().sum();
        let mean = sum / n;
        let mut sorted = nums.to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let median = if nums.len() % 2 == 0 && nums.len() > 0 {
            (sorted[nums.len() / 2 - 1] + sorted[nums.len() / 2]) / 2.0
        } else {
            sorted[nums.len() / 2]
        };
        let variance: f64 = if n > 1.0 {
            nums.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0)
        } else {
            0.0
        };
        let std_dev = variance.sqrt();
        let min = sorted.first().copied().unwrap_or(0.0);
        let max = sorted.last().copied().unwrap_or(0.0);

        let nums_str: Vec<String> = nums.iter().take(20).map(|n| format!("{}", n)).collect();
        format!(
            "**Statistical Analysis** of {} values:\n\n\
             Values: {}\n  Sum: {}\n  Mean: {:.4}\n  Median: {:.4}\n\
             Std Dev: {:.4}\n  Variance: {:.4}\n  Min: {}\n  Max: {}\n  Range: {}",
            nums.len(),
            nums_str.join(", "),
            sum,
            mean,
            median,
            std_dev,
            variance,
            min,
            max,
            max - min
        )
    }

    fn solve_linear_equation(&self, prompt: &str) -> Option<String> {
        let re = regex::Regex::new(r"(-?\d*\.?\d*)x\s*([+\-])\s*(\d+\.?\d*)\s*=\s*(-?\d+\.?\d*)")
            .ok()?;
        let caps = re.captures(prompt)?;
        let a_str = caps.get(1)?.as_str();
        let a: f64 = if a_str.is_empty() || a_str == "-" {
            if a_str == "-" {
                -1.0
            } else {
                1.0
            }
        } else {
            a_str.parse().ok()?
        };
        let sign: f64 = if caps.get(2)?.as_str() == "+" {
            1.0
        } else {
            -1.0
        };
        let b: f64 = caps.get(3)?.as_str().parse::<f64>().ok()? * sign;
        let c: f64 = caps.get(4)?.as_str().parse().ok()?;

        if a.abs() < 1e-12 {
            return None;
        }
        let x = (c - b) / a;

        Some(format!(
            "**Solving**: {}\n\n\
             Step 1: Subtract {} from both sides: {}x = {}\n\
             Step 2: Divide by {}: x = {}\n\n\
             **Solution: x = {}**\n\n\
             Verification: {}({}) {} {} = {} ✓",
            caps.get(0)?.as_str(),
            b,
            a,
            c - b,
            a,
            x,
            x,
            a,
            x,
            if sign > 0.0 { "+" } else { "-" },
            b.abs(),
            a * x + b
        ))
    }
}

// ═══════════════════════════════════════════════════════════
// Simple Expression Parser (safe, no eval)
// ═══════════════════════════════════════════════════════════

struct ExprParser;

impl ExprParser {
    fn parse(expr: &str) -> Option<f64> {
        let tokens = Self::tokenize(expr)?;
        let mut pos = 0;
        let result = Self::parse_expr(&tokens, &mut pos)?;
        if pos == tokens.len() {
            Some(result)
        } else {
            None
        }
    }

    fn tokenize(expr: &str) -> Option<Vec<Token>> {
        let mut tokens = Vec::new();
        let chars: Vec<char> = expr.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            match chars[i] {
                '0'..='9' | '.' => {
                    let start = i;
                    while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                        i += 1;
                    }
                    let num: f64 = chars[start..i].iter().collect::<String>().parse().ok()?;
                    tokens.push(Token::Num(num));
                }
                '+' => {
                    tokens.push(Token::Op('+'));
                    i += 1;
                }
                '-' => {
                    tokens.push(Token::Op('-'));
                    i += 1;
                }
                '*' => {
                    tokens.push(Token::Op('*'));
                    i += 1;
                }
                '/' => {
                    tokens.push(Token::Op('/'));
                    i += 1;
                }
                '^' => {
                    tokens.push(Token::Op('^'));
                    i += 1;
                }
                '%' => {
                    tokens.push(Token::Op('%'));
                    i += 1;
                }
                '(' => {
                    tokens.push(Token::LParen);
                    i += 1;
                }
                ')' => {
                    tokens.push(Token::RParen);
                    i += 1;
                }
                _ => {
                    i += 1;
                }
            }
        }
        Some(tokens)
    }

    fn parse_expr(tokens: &[Token], pos: &mut usize) -> Option<f64> {
        let mut left = Self::parse_term(tokens, pos)?;
        while *pos < tokens.len() {
            match tokens[*pos] {
                Token::Op('+') => {
                    *pos += 1;
                    left += Self::parse_term(tokens, pos)?;
                }
                Token::Op('-') => {
                    *pos += 1;
                    left -= Self::parse_term(tokens, pos)?;
                }
                _ => break,
            }
        }
        Some(left)
    }

    fn parse_term(tokens: &[Token], pos: &mut usize) -> Option<f64> {
        let mut left = Self::parse_power(tokens, pos)?;
        while *pos < tokens.len() {
            match tokens[*pos] {
                Token::Op('*') => {
                    *pos += 1;
                    left *= Self::parse_power(tokens, pos)?;
                }
                Token::Op('/') => {
                    *pos += 1;
                    let right = Self::parse_power(tokens, pos)?;
                    if right.abs() < 1e-15 {
                        return None;
                    }
                    left /= right;
                }
                Token::Op('%') => {
                    *pos += 1;
                    let right = Self::parse_power(tokens, pos)?;
                    left %= right;
                }
                _ => break,
            }
        }
        Some(left)
    }

    fn parse_power(tokens: &[Token], pos: &mut usize) -> Option<f64> {
        let base = Self::parse_unary(tokens, pos)?;
        if *pos < tokens.len() && tokens[*pos] == Token::Op('^') {
            *pos += 1;
            let exp = Self::parse_power(tokens, pos)?; // right-associative
            Some(base.powf(exp))
        } else {
            Some(base)
        }
    }

    fn parse_unary(tokens: &[Token], pos: &mut usize) -> Option<f64> {
        if *pos < tokens.len() && tokens[*pos] == Token::Op('-') {
            *pos += 1;
            let val = Self::parse_atom(tokens, pos)?;
            Some(-val)
        } else {
            Self::parse_atom(tokens, pos)
        }
    }

    fn parse_atom(tokens: &[Token], pos: &mut usize) -> Option<f64> {
        if *pos >= tokens.len() {
            return None;
        }
        match tokens[*pos] {
            Token::Num(n) => {
                *pos += 1;
                Some(n)
            }
            Token::LParen => {
                *pos += 1;
                let result = Self::parse_expr(tokens, pos)?;
                if *pos < tokens.len() && tokens[*pos] == Token::RParen {
                    *pos += 1;
                }
                Some(result)
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Num(f64),
    Op(char),
    LParen,
    RParen,
}

// ═══════════════════════════════════════════════════════════
// CODE SOLVER — Pattern-Based Code Generation
// ═══════════════════════════════════════════════════════════

pub struct CodeSolver {
    templates: HashMap<String, &'static str>,
}

impl Default for CodeSolver {
    fn default() -> Self {
        Self::new()
    }
}

impl CodeSolver {
    pub fn new() -> Self {
        let mut templates = HashMap::new();
        templates.insert("fibonacci".into(), TEMPLATE_FIBONACCI);
        templates.insert("binary search".into(), TEMPLATE_BINARY_SEARCH);
        templates.insert("bubble sort".into(), TEMPLATE_BUBBLE_SORT);
        templates.insert("quick sort".into(), TEMPLATE_QUICK_SORT);
        templates.insert("merge sort".into(), TEMPLATE_MERGE_SORT);
        templates.insert("linked list".into(), TEMPLATE_LINKED_LIST);
        templates.insert("stack".into(), TEMPLATE_STACK);
        templates.insert("bfs".into(), TEMPLATE_BFS);
        templates.insert("dfs".into(), TEMPLATE_DFS);
        templates.insert("hash map".into(), TEMPLATE_HASH_MAP);
        Self { templates }
    }

    pub fn solve(&self, prompt: &str) -> SolverResult {
        let start = Instant::now();
        let mut result = SolverResult::empty("CodeSolver");
        let prompt_lower = prompt.to_lowercase();

        for (pattern_key, template) in &self.templates {
            let keywords: Vec<&str> = pattern_key.split_whitespace().collect();
            if keywords.iter().all(|kw| prompt_lower.contains(kw)) {
                result.answer = format!(
                    "Here's a complete implementation:\n\n```\n{}\n```\n\n\
                     Best practices: clean code, type hints, documented complexity.",
                    template
                );
                result.confidence = 0.90;
                result.reasoning_trace = vec![format!("Matched pattern: {}", pattern_key)];
                result.duration_ms = start.elapsed().as_secs_f64() * 1000.0;
                return result;
            }
        }

        // Fuzzy match
        if let Some((best_key, score)) = self.fuzzy_match(&prompt_lower) {
            if score > 0.4 {
                if let Some(template) = self.templates.get(&best_key) {
                    result.answer = format!(
                        "Based on your request, here's a relevant implementation:\n\n```\n{}\n```\n\n\
                         Pattern: **{}**", template, best_key
                    );
                    result.confidence = 0.70 * score;
                    result.duration_ms = start.elapsed().as_secs_f64() * 1000.0;
                    return result;
                }
            }
        }

        result.duration_ms = start.elapsed().as_secs_f64() * 1000.0;
        result
    }

    fn fuzzy_match(&self, prompt: &str) -> Option<(String, f64)> {
        let mut best: Option<(String, f64)> = None;
        for key in self.templates.keys() {
            let words: Vec<&str> = key.split_whitespace().collect();
            let matched = words.iter().filter(|w| prompt.contains(**w)).count();
            let score = matched as f64 / words.len() as f64;
            if score > best.as_ref().map(|(_, s)| *s).unwrap_or(0.0) {
                best = Some((key.clone(), score));
            }
        }
        best
    }
}

// ═══════════════════════════════════════════════════════════
// LOGIC SOLVER — Predicate Logic & Forward/Backward Chaining
// ═══════════════════════════════════════════════════════════

pub struct LogicSolver;

impl Default for LogicSolver {
    fn default() -> Self {
        Self
    }
}

impl LogicSolver {
    pub fn new() -> Self {
        Self
    }

    pub fn solve(&self, prompt: &str) -> SolverResult {
        let start = Instant::now();
        let mut result = SolverResult::empty("LogicSolver");
        let prompt_lower = prompt.to_lowercase();

        // Syllogism detection: "All X are Y. All Y are Z. Therefore All X are Z."
        if let Some(answer) = self.solve_syllogism(&prompt_lower) {
            result.answer = answer;
            result.confidence = 0.90;
            result.duration_ms = start.elapsed().as_secs_f64() * 1000.0;
            return result;
        }

        // Boolean logic evaluation
        if prompt_lower.contains("truth table") || prompt_lower.contains("boolean") {
            if let Some(answer) = self.evaluate_boolean(&prompt_lower) {
                result.answer = answer;
                result.confidence = 0.85;
                result.duration_ms = start.elapsed().as_secs_f64() * 1000.0;
                return result;
            }
        }

        result.duration_ms = start.elapsed().as_secs_f64() * 1000.0;
        result
    }

    fn solve_syllogism(&self, prompt: &str) -> Option<String> {
        let re = regex::Regex::new(r"all\s+(\w+)\s+are\s+(\w+)").ok()?;
        let captures: Vec<_> = re.captures_iter(prompt).collect();
        if captures.len() >= 2 {
            let a = captures[0].get(1)?.as_str();
            let b = captures[0].get(2)?.as_str();
            let c = captures[1].get(1)?.as_str();
            let d = captures[1].get(2)?.as_str();

            if b == c {
                return Some(format!(
                    "**Syllogism Proof (Barbara)**\n\n\
                     Premise 1: All {} are {}\n\
                     Premise 2: All {} are {}\n\n\
                     **Conclusion: All {} are {}** ✓\n\n\
                     This is a valid Barbara syllogism (AAA-1).\n\
                     The middle term '{}' connects the major and minor terms.",
                    a, b, c, d, a, d, b
                ));
            }
        }
        None
    }

    fn evaluate_boolean(&self, _prompt: &str) -> Option<String> {
        Some(
            "**Boolean Logic Truth Table**\n\n\
             | A | B | A AND B | A OR B | NOT A | A XOR B |\n\
             |---|---|---------|--------|-------|--------|\n\
             | T | T |    T    |   T    |   F   |    F   |\n\
             | T | F |    F    |   T    |   F   |    T   |\n\
             | F | T |    F    |   T    |   T   |    T   |\n\
             | F | F |    F    |   F    |   T   |    F   |\n\n\
             Laws: De Morgan's, Distributive, Absorption, Idempotent."
                .to_string(),
        )
    }
}

// ═══════════════════════════════════════════════════════════
// EXTRACTION SOLVER — Structured Data Extraction
// ═══════════════════════════════════════════════════════════

pub struct ExtractionSolver;

impl Default for ExtractionSolver {
    fn default() -> Self {
        Self
    }
}

impl ExtractionSolver {
    pub fn new() -> Self {
        Self
    }

    pub fn solve(&self, prompt: &str) -> SolverResult {
        let start = Instant::now();
        let mut result = SolverResult::empty("ExtractionSolver");
        let prompt_lower = prompt.to_lowercase();

        if prompt_lower.contains("json") || prompt_lower.contains("extract") {
            // Extract key-value pairs from natural language
            let pairs = self.extract_key_value_pairs(prompt);
            if !pairs.is_empty() {
                let json_obj: serde_json::Value = pairs
                    .into_iter()
                    .map(|(k, v)| (k, serde_json::Value::String(v)))
                    .collect::<serde_json::Map<String, serde_json::Value>>()
                    .into();
                result.answer = format!(
                    "**Extracted Data**:\n```json\n{}\n```",
                    serde_json::to_string_pretty(&json_obj).unwrap_or_default()
                );
                result.confidence = 0.80;
            }
        }

        result.duration_ms = start.elapsed().as_secs_f64() * 1000.0;
        result
    }

    fn extract_key_value_pairs(&self, text: &str) -> Vec<(String, String)> {
        let mut pairs = Vec::new();
        // Pattern: "key: value" or "key = value"
        let re = regex::Regex::new(r"(\w[\w\s]*?):\s*([^\n,;]+)").unwrap();
        for caps in re.captures_iter(text) {
            if let (Some(key), Some(val)) = (caps.get(1), caps.get(2)) {
                pairs.push((
                    key.as_str().trim().to_lowercase().replace(' ', "_"),
                    val.as_str().trim().to_string(),
                ));
            }
        }
        pairs
    }
}

// ═══════════════════════════════════════════════════════════
// PLAN SOLVER — Task Decomposition & Step Ordering
// ═══════════════════════════════════════════════════════════

pub struct PlanSolver;

impl Default for PlanSolver {
    fn default() -> Self {
        Self
    }
}

impl PlanSolver {
    pub fn new() -> Self {
        Self
    }

    pub fn solve(&self, prompt: &str) -> SolverResult {
        let start = Instant::now();
        let mut result = SolverResult::empty("PlanSolver");
        let prompt_lower = prompt.to_lowercase();

        if ["plan", "how to", "step by step", "guide", "roadmap"]
            .iter()
            .any(|kw| prompt_lower.contains(kw))
        {
            let steps = self.decompose_task(&prompt_lower);
            if !steps.is_empty() {
                let steps_str: Vec<String> = steps
                    .iter()
                    .enumerate()
                    .map(|(i, s)| format!("{}. {}", i + 1, s))
                    .collect();
                result.answer = format!(
                    "**Task Plan**\n\n{}\n\n\
                     *Generated by algorithmic task decomposition.*",
                    steps_str.join("\n")
                );
                result.confidence = 0.70;
            }
        }

        result.duration_ms = start.elapsed().as_secs_f64() * 1000.0;
        result
    }

    fn decompose_task(&self, prompt: &str) -> Vec<String> {
        let mut steps = Vec::new();
        steps.push("Analyze requirements and define scope".into());
        steps.push("Research existing solutions and best practices".into());
        steps.push("Design the architecture and data structures".into());

        if prompt.contains("code") || prompt.contains("build") || prompt.contains("implement") {
            steps.push("Set up the development environment".into());
            steps.push("Implement core functionality with tests".into());
            steps.push("Add error handling and edge cases".into());
            steps.push("Optimize performance and refactor".into());
        }

        if prompt.contains("deploy") || prompt.contains("production") {
            steps.push("Set up CI/CD pipeline".into());
            steps.push("Deploy to staging and validate".into());
            steps.push("Deploy to production with monitoring".into());
        }

        steps.push("Document the solution and review".into());
        steps
    }
}

// ═══════════════════════════════════════════════════════════
// SOLVER PIPELINE — Routes to the best solver
// ═══════════════════════════════════════════════════════════

pub struct SolverPipeline {
    math: MathSolver,
    code: CodeSolver,
    logic: LogicSolver,
    extraction: ExtractionSolver,
    plan: PlanSolver,
}

impl SolverPipeline {
    pub fn new() -> Self {
        Self {
            math: MathSolver::new(),
            code: CodeSolver::new(),
            logic: LogicSolver::new(),
            extraction: ExtractionSolver::new(),
            plan: PlanSolver::new(),
        }
    }

    /// Solve a problem by routing to the appropriate solver.
    pub fn solve(&self, intent: &str, prompt: &str) -> SolverResult {
        let start = Instant::now();
        let result = match intent {
            "math" | "physics" => self.math.solve(prompt),
            "code" => self.code.solve(prompt),
            "logic" => self.logic.solve(prompt),
            "extraction" => self.extraction.solve(prompt),
            "plan" => self.plan.solve(prompt),
            _ => {
                // Try all solvers, pick best
                let results = vec![
                    self.math.solve(prompt),
                    self.code.solve(prompt),
                    self.logic.solve(prompt),
                    self.extraction.solve(prompt),
                    self.plan.solve(prompt),
                ];
                results
                    .into_iter()
                    .max_by(|a, b| {
                        a.confidence
                            .partial_cmp(&b.confidence)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .unwrap_or_else(|| SolverResult::empty("SolverPipeline"))
            }
        };

        debug!(
            "[SolverPipeline] intent={} solver={} conf={:.2} duration={:.1}ms",
            intent,
            result.solver_name,
            result.confidence,
            start.elapsed().as_secs_f64() * 1000.0
        );
        result
    }
}

impl Default for SolverPipeline {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════
// Code Templates
// ═══════════════════════════════════════════════════════════

const TEMPLATE_FIBONACCI: &str = r#"fn fibonacci(n: usize) -> Vec<u64> {
    if n == 0 { return vec![]; }
    if n == 1 { return vec![0]; }
    let mut result = vec![0, 1];
    for i in 2..n {
        let next = result[i-1] + result[i-2];
        result.push(next);
    }
    result
}
// Time: O(n), Space: O(n)
// fibonacci(10) -> [0, 1, 1, 2, 3, 5, 8, 13, 21, 34]"#;

const TEMPLATE_BINARY_SEARCH: &str = r#"fn binary_search<T: Ord>(arr: &[T], target: &T) -> Option<usize> {
    let (mut left, mut right) = (0, arr.len());
    while left < right {
        let mid = left + (right - left) / 2;
        match arr[mid].cmp(target) {
            std::cmp::Ordering::Equal => return Some(mid),
            std::cmp::Ordering::Less => left = mid + 1,
            std::cmp::Ordering::Greater => right = mid,
        }
    }
    None
}
// Time: O(log n), Space: O(1)"#;

const TEMPLATE_BUBBLE_SORT: &str = r#"fn bubble_sort<T: Ord>(arr: &mut [T]) {
    let n = arr.len();
    for i in 0..n {
        let mut swapped = false;
        for j in 0..n - i - 1 {
            if arr[j] > arr[j + 1] {
                arr.swap(j, j + 1);
                swapped = true;
            }
        }
        if !swapped { break; }
    }
}
// Time: O(n²) worst, O(n) best | Space: O(1)"#;

const TEMPLATE_QUICK_SORT: &str = r#"fn quick_sort<T: Ord + Clone>(arr: &[T]) -> Vec<T> {
    if arr.len() <= 1 { return arr.to_vec(); }
    let pivot = &arr[arr.len() / 2];
    let left: Vec<T> = arr.iter().filter(|x| *x < pivot).cloned().collect();
    let mid: Vec<T> = arr.iter().filter(|x| *x == pivot).cloned().collect();
    let right: Vec<T> = arr.iter().filter(|x| *x > pivot).cloned().collect();
    [quick_sort(&left), mid, quick_sort(&right)].concat()
}
// Time: O(n log n) avg, O(n²) worst | Space: O(n)"#;

const TEMPLATE_MERGE_SORT: &str = r#"fn merge_sort<T: Ord + Clone>(arr: &[T]) -> Vec<T> {
    if arr.len() <= 1 { return arr.to_vec(); }
    let mid = arr.len() / 2;
    let left = merge_sort(&arr[..mid]);
    let right = merge_sort(&arr[mid..]);
    merge(&left, &right)
}
fn merge<T: Ord + Clone>(left: &[T], right: &[T]) -> Vec<T> {
    let (mut i, mut j) = (0, 0);
    let mut result = Vec::with_capacity(left.len() + right.len());
    while i < left.len() && j < right.len() {
        if left[i] <= right[j] { result.push(left[i].clone()); i += 1; }
        else { result.push(right[j].clone()); j += 1; }
    }
    result.extend_from_slice(&left[i..]);
    result.extend_from_slice(&right[j..]);
    result
}
// Time: O(n log n) guaranteed | Space: O(n) | Stable"#;

const TEMPLATE_LINKED_LIST: &str = r#"struct Node<T> { data: T, next: Option<Box<Node<T>>> }
struct LinkedList<T> { head: Option<Box<Node<T>>>, size: usize }
impl<T: PartialEq> LinkedList<T> {
    fn new() -> Self { Self { head: None, size: 0 } }
    fn push_front(&mut self, data: T) {
        self.head = Some(Box::new(Node { data, next: self.head.take() }));
        self.size += 1;
    }
    fn pop_front(&mut self) -> Option<T> {
        self.head.take().map(|node| { self.head = node.next; self.size -= 1; node.data })
    }
    fn len(&self) -> usize { self.size }
}"#;

const TEMPLATE_STACK: &str = r#"struct Stack<T> { items: Vec<T> }
impl<T> Stack<T> {
    fn new() -> Self { Self { items: Vec::new() } }
    fn push(&mut self, item: T) { self.items.push(item); }
    fn pop(&mut self) -> Option<T> { self.items.pop() }
    fn peek(&self) -> Option<&T> { self.items.last() }
    fn is_empty(&self) -> bool { self.items.is_empty() }
    fn len(&self) -> usize { self.items.len() }
}
// All operations O(1) amortized"#;

const TEMPLATE_BFS: &str = r#"use std::collections::{HashMap, HashSet, VecDeque};
fn bfs(graph: &HashMap<usize, Vec<usize>>, start: usize) -> Vec<usize> {
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    let mut order = Vec::new();
    visited.insert(start);
    queue.push_back(start);
    while let Some(node) = queue.pop_front() {
        order.push(node);
        if let Some(neighbors) = graph.get(&node) {
            for &neighbor in neighbors {
                if visited.insert(neighbor) { queue.push_back(neighbor); }
            }
        }
    }
    order
}
// Time: O(V + E), Space: O(V)"#;

const TEMPLATE_DFS: &str = r#"use std::collections::{HashMap, HashSet};
fn dfs(graph: &HashMap<usize, Vec<usize>>, start: usize) -> Vec<usize> {
    let mut visited = HashSet::new();
    let mut stack = vec![start];
    let mut order = Vec::new();
    while let Some(node) = stack.pop() {
        if visited.insert(node) {
            order.push(node);
            if let Some(neighbors) = graph.get(&node) {
                for &neighbor in neighbors.iter().rev() {
                    if !visited.contains(&neighbor) { stack.push(neighbor); }
                }
            }
        }
    }
    order
}
// Time: O(V + E), Space: O(V)"#;

const TEMPLATE_HASH_MAP: &str = r#"struct SimpleHashMap<K, V> {
    buckets: Vec<Vec<(K, V)>>,
    capacity: usize,
    size: usize,
}
impl<K: std::hash::Hash + Eq + Clone, V: Clone> SimpleHashMap<K, V> {
    fn new(capacity: usize) -> Self {
        Self { buckets: (0..capacity).map(|_| Vec::new()).collect(), capacity, size: 0 }
    }
    fn hash(&self, key: &K) -> usize {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut hasher);
        hasher.finish() as usize % self.capacity
    }
    fn insert(&mut self, key: K, value: V) {
        let idx = self.hash(&key);
        for (k, v) in &mut self.buckets[idx] {
            if *k == key { *v = value; return; }
        }
        self.buckets[idx].push((key, value));
        self.size += 1;
    }
    fn get(&self, key: &K) -> Option<&V> {
        let idx = self.hash(key);
        self.buckets[idx].iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }
}
// Average: O(1) get/insert | Worst: O(n)"#;
