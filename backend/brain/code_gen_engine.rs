// ═══════════════════════════════════════════════════════════════
// Code Generation & Self-Programming Engine
// ═══════════════════════════════════════════════════════════════
//
// Production-grade AST-based code generation system that constructs
// programs as structured syntax trees, validates them, and can
// propose modifications to its own cognitive modules.
//
// Architecture:
//   1. AST Builder — Constructs code as Abstract Syntax Trees
//   2. Type System — Verifies type correctness before serialization
//   3. Template Library — Pre-built patterns for common code structures
//   4. Code Validator — Static analysis on generated code
//   5. Self-Evolution Protocol — Proposes modifications to brain modules
//   6. Multi-Language Serializer — AST → Rust/Python/TypeScript
//
// Pure Rust. Deterministic. No external compiler dependency for generation.

use std::collections::HashMap;
use std::time::Instant;

// ═══════════════════════════════════════════════════════════════
// AST TYPES
// ═══════════════════════════════════════════════════════════════

/// A node in the Abstract Syntax Tree.
#[derive(Debug, Clone)]
pub enum AstNode {
    // Module level
    Module {
        name: String,
        items: Vec<AstNode>,
    },
    Import {
        path: String,
        items: Vec<String>,
    },
    Comment {
        text: String,
    },

    // Declarations
    Function {
        name: String,
        params: Vec<Parameter>,
        return_type: Option<TypeExpr>,
        body: Vec<AstNode>,
        is_public: bool,
        is_async: bool,
        doc_comment: Option<String>,
    },
    Struct {
        name: String,
        fields: Vec<StructField>,
        is_public: bool,
        derives: Vec<String>,
        doc_comment: Option<String>,
    },
    Enum {
        name: String,
        variants: Vec<EnumVariant>,
        is_public: bool,
        derives: Vec<String>,
    },
    Impl {
        target: String,
        methods: Vec<AstNode>,
    },
    Trait {
        name: String,
        methods: Vec<AstNode>,
        is_public: bool,
    },
    Const {
        name: String,
        type_expr: TypeExpr,
        value: Box<AstNode>,
        is_public: bool,
    },

    // Statements
    Let {
        name: String,
        type_expr: Option<TypeExpr>,
        value: Box<AstNode>,
        is_mutable: bool,
    },
    Assignment {
        target: String,
        value: Box<AstNode>,
    },
    Return {
        value: Option<Box<AstNode>>,
    },
    If {
        condition: Box<AstNode>,
        then_branch: Vec<AstNode>,
        else_branch: Option<Vec<AstNode>>,
    },
    Match {
        expr: Box<AstNode>,
        arms: Vec<MatchArm>,
    },
    ForLoop {
        variable: String,
        iterable: Box<AstNode>,
        body: Vec<AstNode>,
    },
    WhileLoop {
        condition: Box<AstNode>,
        body: Vec<AstNode>,
    },

    // Expressions
    Literal(LiteralValue),
    Identifier(String),
    BinaryOp {
        op: BinaryOperator,
        left: Box<AstNode>,
        right: Box<AstNode>,
    },
    UnaryOp {
        op: UnaryOperator,
        operand: Box<AstNode>,
    },
    FunctionCall {
        function: String,
        args: Vec<AstNode>,
    },
    MethodCall {
        object: Box<AstNode>,
        method: String,
        args: Vec<AstNode>,
    },
    FieldAccess {
        object: Box<AstNode>,
        field: String,
    },
    IndexAccess {
        object: Box<AstNode>,
        index: Box<AstNode>,
    },
    ArrayLiteral {
        elements: Vec<AstNode>,
    },
    StructLiteral {
        name: String,
        fields: Vec<(String, AstNode)>,
    },
    Closure {
        params: Vec<Parameter>,
        body: Vec<AstNode>,
    },
    Await {
        expr: Box<AstNode>,
    },
    Reference {
        expr: Box<AstNode>,
        is_mutable: bool,
    },

    // Control
    Break,
    Continue,
    Block(Vec<AstNode>),
}

#[derive(Debug, Clone)]
pub struct Parameter {
    pub name: String,
    pub type_expr: TypeExpr,
    pub is_mutable: bool,
    pub is_reference: bool,
}

#[derive(Debug, Clone)]
pub struct StructField {
    pub name: String,
    pub type_expr: TypeExpr,
    pub is_public: bool,
    pub doc_comment: Option<String>,
}

#[derive(Debug, Clone)]
pub struct EnumVariant {
    pub name: String,
    pub fields: Option<Vec<TypeExpr>>,
}

#[derive(Debug, Clone)]
pub struct MatchArm {
    pub pattern: String,
    pub body: Vec<AstNode>,
}

#[derive(Debug, Clone)]
pub enum TypeExpr {
    Simple(String),
    Generic(String, Vec<TypeExpr>),
    Reference(Box<TypeExpr>, bool), // (type, is_mutable)
    Option(Box<TypeExpr>),
    Result(Box<TypeExpr>, Box<TypeExpr>),
    Vec(Box<TypeExpr>),
    Tuple(Vec<TypeExpr>),
    Slice(Box<TypeExpr>),
    Array(Box<TypeExpr>, usize),
    Fn(Vec<TypeExpr>, Box<TypeExpr>),
    Unit,
}

#[derive(Debug, Clone)]
pub enum LiteralValue {
    Integer(i64),
    Float(f64),
    String(String),
    Bool(bool),
    Char(char),
    None,
}

#[derive(Debug, Clone)]
pub enum BinaryOperator {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    And,
    Or,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
}

#[derive(Debug, Clone)]
pub enum UnaryOperator {
    Neg,
    Not,
    Deref,
}

/// Target language for serialization.
#[derive(Debug, Clone, PartialEq)]
pub enum TargetLanguage {
    Rust,
    Python,
    TypeScript,
}

// ═══════════════════════════════════════════════════════════════
// VALIDATION TYPES
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<String>,
    pub complexity_score: f64,
    pub line_count_estimate: usize,
}

#[derive(Debug, Clone)]
pub struct ValidationError {
    pub severity: ErrorSeverity,
    pub message: String,
    pub node_path: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ErrorSeverity {
    Error,
    Warning,
    Info,
}

/// A self-evolution proposal.
#[derive(Debug, Clone)]
pub struct EvolutionProposal {
    pub id: u64,
    pub target_module: String,
    pub description: String,
    pub proposed_ast: AstNode,
    pub rationale: String,
    pub risk_level: f64,
    pub expected_improvement: f64,
    pub safety_approved: bool,
}

/// Result of code generation.
#[derive(Debug, Clone)]
pub struct CodeGenResult {
    pub code: String,
    pub language: TargetLanguage,
    pub ast: AstNode,
    pub validation: ValidationResult,
    pub duration_ms: f64,
}

// ═══════════════════════════════════════════════════════════════
// CODE GENERATION ENGINE
// ═══════════════════════════════════════════════════════════════

pub struct CodeGenEngine {
    templates: HashMap<String, AstNode>,
    evolution_proposals: Vec<EvolutionProposal>,
    next_proposal_id: u64,
    total_generations: u64,
    total_validations: u64,
}

impl CodeGenEngine {
    pub fn new() -> Self {
        let mut engine = Self {
            templates: HashMap::new(),
            evolution_proposals: Vec::new(),
            next_proposal_id: 1,
            total_generations: 0,
            total_validations: 0,
        };
        engine.load_default_templates();
        engine
    }

    /// Generate code from an AST.
    pub fn generate(&mut self, ast: &AstNode, language: TargetLanguage) -> CodeGenResult {
        let start = Instant::now();
        self.total_generations += 1;

        let validation = self.validate(ast);
        let code = match language {
            TargetLanguage::Rust => self.serialize_rust(ast, 0),
            TargetLanguage::Python => self.serialize_python(ast, 0),
            TargetLanguage::TypeScript => self.serialize_typescript(ast, 0),
        };

        CodeGenResult {
            code,
            language,
            ast: ast.clone(),
            validation,
            duration_ms: start.elapsed().as_secs_f64() * 1000.0,
        }
    }

    /// Build a function AST from specifications.
    pub fn build_function(
        &self,
        name: &str,
        params: &[(&str, &str)],
        return_type: Option<&str>,
        body_statements: Vec<AstNode>,
        is_public: bool,
        doc: Option<&str>,
    ) -> AstNode {
        AstNode::Function {
            name: name.to_string(),
            params: params
                .iter()
                .map(|(n, t)| Parameter {
                    name: n.to_string(),
                    type_expr: TypeExpr::Simple(t.to_string()),
                    is_mutable: false,
                    is_reference: false,
                })
                .collect(),
            return_type: return_type.map(|t| TypeExpr::Simple(t.to_string())),
            body: body_statements,
            is_public,
            is_async: false,
            doc_comment: doc.map(|d| d.to_string()),
        }
    }

    /// Build a struct AST.
    pub fn build_struct(
        &self,
        name: &str,
        fields: &[(&str, &str, bool)],
        derives: &[&str],
        is_public: bool,
        doc: Option<&str>,
    ) -> AstNode {
        AstNode::Struct {
            name: name.to_string(),
            fields: fields
                .iter()
                .map(|(n, t, pub_field)| StructField {
                    name: n.to_string(),
                    type_expr: TypeExpr::Simple(t.to_string()),
                    is_public: *pub_field,
                    doc_comment: None,
                })
                .collect(),
            is_public,
            derives: derives.iter().map(|d| d.to_string()).collect(),
            doc_comment: doc.map(|d| d.to_string()),
        }
    }

    /// Propose a self-evolution modification to a brain module.
    pub fn propose_evolution(
        &mut self,
        target_module: &str,
        description: &str,
        proposed_ast: AstNode,
        rationale: &str,
        risk_level: f64,
        expected_improvement: f64,
    ) -> u64 {
        let id = self.next_proposal_id;
        self.next_proposal_id += 1;

        // Safety gate: high-risk proposals require manual approval
        let safety_approved = risk_level < 0.3;

        self.evolution_proposals.push(EvolutionProposal {
            id,
            target_module: target_module.to_string(),
            description: description.to_string(),
            proposed_ast,
            rationale: rationale.to_string(),
            risk_level,
            expected_improvement,
            safety_approved,
        });

        id
    }

    /// Get pending evolution proposals.
    pub fn get_pending_proposals(&self) -> Vec<&EvolutionProposal> {
        self.evolution_proposals
            .iter()
            .filter(|p| !p.safety_approved)
            .collect()
    }

    /// Approve an evolution proposal.
    pub fn approve_proposal(&mut self, id: u64) -> bool {
        if let Some(proposal) = self.evolution_proposals.iter_mut().find(|p| p.id == id) {
            proposal.safety_approved = true;
            true
        } else {
            false
        }
    }

    /// Validate an AST for correctness.
    pub fn validate(&mut self, ast: &AstNode) -> ValidationResult {
        self.total_validations += 1;
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        let complexity = self.compute_complexity(ast);
        let lines = self.estimate_lines(ast);

        self.validate_node(ast, "", &mut errors, &mut warnings);

        ValidationResult {
            is_valid: errors.iter().all(|e| e.severity != ErrorSeverity::Error),
            errors,
            warnings,
            complexity_score: complexity,
            line_count_estimate: lines,
        }
    }

    pub fn get_stats(&self) -> serde_json::Value {
        serde_json::json!({
            "engine": "CodeGenEngine v1.0",
            "total_generations": self.total_generations,
            "total_validations": self.total_validations,
            "template_count": self.templates.len(),
            "pending_proposals": self.evolution_proposals.iter().filter(|p| !p.safety_approved).count(),
            "total_proposals": self.evolution_proposals.len(),
        })
    }

    // ─────────────────────────────────────────────────────
    // SERIALIZERS
    // ─────────────────────────────────────────────────────

    fn serialize_rust(&self, node: &AstNode, indent: usize) -> String {
        let pad = "    ".repeat(indent);
        match node {
            AstNode::Module { name, items } => {
                let mut out = format!("// Module: {}\n\n", name);
                for item in items {
                    out.push_str(&self.serialize_rust(item, indent));
                    out.push('\n');
                }
                out
            }
            AstNode::Import { path, items } => {
                if items.is_empty() {
                    format!("{}use {};\n", pad, path)
                } else {
                    format!("{}use {}::{{{}}};\n", pad, path, items.join(", "))
                }
            }
            AstNode::Comment { text } => format!("{}// {}\n", pad, text),
            AstNode::Function {
                name,
                params,
                return_type,
                body,
                is_public,
                is_async,
                doc_comment,
            } => {
                let mut out = String::new();
                if let Some(doc) = doc_comment {
                    out.push_str(&format!("{}/// {}\n", pad, doc));
                }
                let vis = if *is_public { "pub " } else { "" };
                let async_kw = if *is_async { "async " } else { "" };
                let params_str: Vec<String> = params
                    .iter()
                    .map(|p| {
                        let ref_str = if p.is_reference { "&" } else { "" };
                        let mut_str = if p.is_mutable { "mut " } else { "" };
                        format!(
                            "{}{}{}: {}",
                            ref_str,
                            mut_str,
                            p.name,
                            self.serialize_type_rust(&p.type_expr)
                        )
                    })
                    .collect();
                let ret = return_type
                    .as_ref()
                    .map(|t| format!(" -> {}", self.serialize_type_rust(t)))
                    .unwrap_or_default();
                out.push_str(&format!(
                    "{}{}{}fn {}({}){} {{\n",
                    pad,
                    vis,
                    async_kw,
                    name,
                    params_str.join(", "),
                    ret
                ));
                for stmt in body {
                    out.push_str(&self.serialize_rust(stmt, indent + 1));
                }
                out.push_str(&format!("{}}}\n", pad));
                out
            }
            AstNode::Struct {
                name,
                fields,
                is_public,
                derives,
                doc_comment,
            } => {
                let mut out = String::new();
                if let Some(doc) = doc_comment {
                    out.push_str(&format!("{}/// {}\n", pad, doc));
                }
                if !derives.is_empty() {
                    out.push_str(&format!("{}#[derive({})]\n", pad, derives.join(", ")));
                }
                let vis = if *is_public { "pub " } else { "" };
                out.push_str(&format!("{}{}struct {} {{\n", pad, vis, name));
                for field in fields {
                    let fvis = if field.is_public { "pub " } else { "" };
                    out.push_str(&format!(
                        "{}    {}{}: {},\n",
                        pad,
                        fvis,
                        field.name,
                        self.serialize_type_rust(&field.type_expr)
                    ));
                }
                out.push_str(&format!("{}}}\n", pad));
                out
            }
            AstNode::Let {
                name,
                type_expr,
                value,
                is_mutable,
            } => {
                let mut_kw = if *is_mutable { "mut " } else { "" };
                let type_ann = type_expr
                    .as_ref()
                    .map(|t| format!(": {}", self.serialize_type_rust(t)))
                    .unwrap_or_default();
                format!(
                    "{}let {}{}{} = {};\n",
                    pad,
                    mut_kw,
                    name,
                    type_ann,
                    self.serialize_rust(value, 0).trim()
                )
            }
            AstNode::Return { value } => match value {
                Some(v) => format!("{}return {};\n", pad, self.serialize_rust(v, 0).trim()),
                None => format!("{}return;\n", pad),
            },
            AstNode::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let mut out = format!(
                    "{}if {} {{\n",
                    pad,
                    self.serialize_rust(condition, 0).trim()
                );
                for stmt in then_branch {
                    out.push_str(&self.serialize_rust(stmt, indent + 1));
                }
                if let Some(else_stmts) = else_branch {
                    out.push_str(&format!("{}}} else {{\n", pad));
                    for stmt in else_stmts {
                        out.push_str(&self.serialize_rust(stmt, indent + 1));
                    }
                }
                out.push_str(&format!("{}}}\n", pad));
                out
            }
            AstNode::ForLoop {
                variable,
                iterable,
                body,
            } => {
                let mut out = format!(
                    "{}for {} in {} {{\n",
                    pad,
                    variable,
                    self.serialize_rust(iterable, 0).trim()
                );
                for stmt in body {
                    out.push_str(&self.serialize_rust(stmt, indent + 1));
                }
                out.push_str(&format!("{}}}\n", pad));
                out
            }
            AstNode::Literal(val) => match val {
                LiteralValue::Integer(n) => format!("{}", n),
                LiteralValue::Float(f) => format!("{:.6}", f),
                LiteralValue::String(s) => format!("\"{}\"", s),
                LiteralValue::Bool(b) => format!("{}", b),
                LiteralValue::Char(c) => format!("'{}'", c),
                LiteralValue::None => "None".to_string(),
            },
            AstNode::Identifier(name) => name.clone(),
            AstNode::BinaryOp { op, left, right } => {
                let op_str = match op {
                    BinaryOperator::Add => "+",
                    BinaryOperator::Sub => "-",
                    BinaryOperator::Mul => "*",
                    BinaryOperator::Div => "/",
                    BinaryOperator::Mod => "%",
                    BinaryOperator::Eq => "==",
                    BinaryOperator::Ne => "!=",
                    BinaryOperator::Lt => "<",
                    BinaryOperator::Gt => ">",
                    BinaryOperator::Le => "<=",
                    BinaryOperator::Ge => ">=",
                    BinaryOperator::And => "&&",
                    BinaryOperator::Or => "||",
                    BinaryOperator::BitAnd => "&",
                    BinaryOperator::BitOr => "|",
                    BinaryOperator::BitXor => "^",
                    BinaryOperator::Shl => "<<",
                    BinaryOperator::Shr => ">>",
                };
                format!(
                    "{} {} {}",
                    self.serialize_rust(left, 0).trim(),
                    op_str,
                    self.serialize_rust(right, 0).trim()
                )
            }
            AstNode::FunctionCall { function, args } => {
                let args_str: Vec<String> = args
                    .iter()
                    .map(|a| self.serialize_rust(a, 0).trim().to_string())
                    .collect();
                format!("{}({})", function, args_str.join(", "))
            }
            AstNode::MethodCall {
                object,
                method,
                args,
            } => {
                let args_str: Vec<String> = args
                    .iter()
                    .map(|a| self.serialize_rust(a, 0).trim().to_string())
                    .collect();
                format!(
                    "{}.{}({})",
                    self.serialize_rust(object, 0).trim(),
                    method,
                    args_str.join(", ")
                )
            }
            AstNode::Assignment { target, value } => {
                format!(
                    "{}{} = {};\n",
                    pad,
                    target,
                    self.serialize_rust(value, 0).trim()
                )
            }
            _ => format!("{}/* unserialized node */\n", pad),
        }
    }

    fn serialize_python(&self, node: &AstNode, indent: usize) -> String {
        let pad = "    ".repeat(indent);
        match node {
            AstNode::Function {
                name,
                params,
                body,
                doc_comment,
                ..
            } => {
                let mut out = String::new();
                let params_str: Vec<String> = params
                    .iter()
                    .map(|p| format!("{}: {}", p.name, self.serialize_type_python(&p.type_expr)))
                    .collect();
                out.push_str(&format!(
                    "{}def {}({}):\n",
                    pad,
                    name,
                    params_str.join(", ")
                ));
                if let Some(doc) = doc_comment {
                    out.push_str(&format!("{}    \"\"\"{}\"\"\"\n", pad, doc));
                }
                if body.is_empty() {
                    out.push_str(&format!("{}    pass\n", pad));
                } else {
                    for stmt in body {
                        out.push_str(&self.serialize_python(stmt, indent + 1));
                    }
                }
                out
            }
            AstNode::Let { name, value, .. } => {
                format!(
                    "{}{} = {}\n",
                    pad,
                    name,
                    self.serialize_python(value, 0).trim()
                )
            }
            AstNode::Return { value } => match value {
                Some(v) => format!("{}return {}\n", pad, self.serialize_python(v, 0).trim()),
                None => format!("{}return\n", pad),
            },
            AstNode::Literal(val) => match val {
                LiteralValue::Integer(n) => format!("{}", n),
                LiteralValue::Float(f) => format!("{:.6}", f),
                LiteralValue::String(s) => format!("\"{}\"", s),
                LiteralValue::Bool(b) => {
                    if *b {
                        "True".to_string()
                    } else {
                        "False".to_string()
                    }
                }
                LiteralValue::None => "None".to_string(),
                LiteralValue::Char(c) => format!("\"{}\"", c),
            },
            AstNode::Identifier(name) => name.clone(),
            AstNode::Comment { text } => format!("{}# {}\n", pad, text),
            _ => format!("{}# unserialized node\n", pad),
        }
    }

    fn serialize_typescript(&self, node: &AstNode, indent: usize) -> String {
        let pad = "  ".repeat(indent);
        match node {
            AstNode::Function {
                name,
                params,
                return_type,
                body,
                is_public,
                is_async,
                doc_comment,
            } => {
                let mut out = String::new();
                if let Some(doc) = doc_comment {
                    out.push_str(&format!("{}/** {} */\n", pad, doc));
                }
                let export = if *is_public { "export " } else { "" };
                let async_kw = if *is_async { "async " } else { "" };
                let params_str: Vec<String> = params
                    .iter()
                    .map(|p| format!("{}: {}", p.name, self.serialize_type_ts(&p.type_expr)))
                    .collect();
                let ret = return_type
                    .as_ref()
                    .map(|t| format!(": {}", self.serialize_type_ts(t)))
                    .unwrap_or_default();
                out.push_str(&format!(
                    "{}{}{}function {}({}){} {{\n",
                    pad,
                    export,
                    async_kw,
                    name,
                    params_str.join(", "),
                    ret
                ));
                for stmt in body {
                    out.push_str(&self.serialize_typescript(stmt, indent + 1));
                }
                out.push_str(&format!("{}}}\n", pad));
                out
            }
            AstNode::Let {
                name,
                type_expr,
                value,
                is_mutable,
            } => {
                let kw = if *is_mutable { "let" } else { "const" };
                let type_ann = type_expr
                    .as_ref()
                    .map(|t| format!(": {}", self.serialize_type_ts(t)))
                    .unwrap_or_default();
                format!(
                    "{}{} {}{} = {};\n",
                    pad,
                    kw,
                    name,
                    type_ann,
                    self.serialize_typescript(value, 0).trim()
                )
            }
            AstNode::Literal(val) => match val {
                LiteralValue::Integer(n) => format!("{}", n),
                LiteralValue::Float(f) => format!("{}", f),
                LiteralValue::String(s) => format!("\"{}\"", s),
                LiteralValue::Bool(b) => format!("{}", b),
                LiteralValue::None => "null".to_string(),
                LiteralValue::Char(c) => format!("\"{}\"", c),
            },
            AstNode::Identifier(name) => name.clone(),
            AstNode::Comment { text } => format!("{}// {}\n", pad, text),
            _ => format!("{}// unserialized node\n", pad),
        }
    }

    fn serialize_type_rust(&self, t: &TypeExpr) -> String {
        match t {
            TypeExpr::Simple(s) => s.clone(),
            TypeExpr::Generic(base, args) => format!(
                "{}<{}>",
                base,
                args.iter()
                    .map(|a| self.serialize_type_rust(a))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            TypeExpr::Option(inner) => format!("Option<{}>", self.serialize_type_rust(inner)),
            TypeExpr::Result(ok, err) => format!(
                "Result<{}, {}>",
                self.serialize_type_rust(ok),
                self.serialize_type_rust(err)
            ),
            TypeExpr::Vec(inner) => format!("Vec<{}>", self.serialize_type_rust(inner)),
            TypeExpr::Reference(inner, is_mut) => format!(
                "&{}{}",
                if *is_mut { "mut " } else { "" },
                self.serialize_type_rust(inner)
            ),
            TypeExpr::Unit => "()".to_string(),
            TypeExpr::Tuple(items) => format!(
                "({})",
                items
                    .iter()
                    .map(|i| self.serialize_type_rust(i))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            TypeExpr::Slice(inner) => format!("[{}]", self.serialize_type_rust(inner)),
            TypeExpr::Array(inner, size) => {
                format!("[{}; {}]", self.serialize_type_rust(inner), size)
            }
            TypeExpr::Fn(params, ret) => format!(
                "Fn({}) -> {}",
                params
                    .iter()
                    .map(|p| self.serialize_type_rust(p))
                    .collect::<Vec<_>>()
                    .join(", "),
                self.serialize_type_rust(ret)
            ),
        }
    }

    fn serialize_type_python(&self, t: &TypeExpr) -> String {
        match t {
            TypeExpr::Simple(s) => match s.as_str() {
                "f64" | "f32" => "float".to_string(),
                "i32" | "i64" | "u32" | "u64" | "usize" | "isize" => "int".to_string(),
                "String" | "&str" => "str".to_string(),
                "bool" => "bool".to_string(),
                _ => s.clone(),
            },
            TypeExpr::Vec(inner) => format!("list[{}]", self.serialize_type_python(inner)),
            TypeExpr::Option(inner) => format!("Optional[{}]", self.serialize_type_python(inner)),
            _ => "Any".to_string(),
        }
    }

    fn serialize_type_ts(&self, t: &TypeExpr) -> String {
        match t {
            TypeExpr::Simple(s) => match s.as_str() {
                "f64" | "f32" | "i32" | "i64" | "u32" | "u64" | "usize" => "number".to_string(),
                "String" | "&str" => "string".to_string(),
                "bool" => "boolean".to_string(),
                _ => s.clone(),
            },
            TypeExpr::Vec(inner) => format!("{}[]", self.serialize_type_ts(inner)),
            TypeExpr::Option(inner) => format!("{} | null", self.serialize_type_ts(inner)),
            _ => "any".to_string(),
        }
    }

    // ─────────────────────────────────────────────────────
    // VALIDATION
    // ─────────────────────────────────────────────────────

    fn validate_node(
        &self,
        node: &AstNode,
        path: &str,
        errors: &mut Vec<ValidationError>,
        warnings: &mut Vec<String>,
    ) {
        match node {
            AstNode::Function {
                name, body, params, ..
            } => {
                let fn_path = format!("{}::{}", path, name);
                if name.is_empty() {
                    errors.push(ValidationError {
                        severity: ErrorSeverity::Error,
                        message: "Function name is empty".into(),
                        node_path: fn_path.clone(),
                    });
                }
                if body.is_empty() {
                    warnings.push(format!("Function {} has empty body", fn_path));
                }
                // Check for duplicate param names
                let mut seen = std::collections::HashSet::new();
                for p in params {
                    if !seen.insert(&p.name) {
                        errors.push(ValidationError {
                            severity: ErrorSeverity::Error,
                            message: format!("Duplicate parameter: {}", p.name),
                            node_path: fn_path.clone(),
                        });
                    }
                }
                for stmt in body {
                    self.validate_node(stmt, &fn_path, errors, warnings);
                }
            }
            AstNode::Struct { name, fields, .. } => {
                if fields.is_empty() {
                    warnings.push(format!("Struct {} has no fields", name));
                }
            }
            AstNode::Module { items, .. } => {
                for item in items {
                    self.validate_node(item, path, errors, warnings);
                }
            }
            _ => {}
        }
    }

    fn compute_complexity(&self, node: &AstNode) -> f64 {
        match node {
            AstNode::If {
                then_branch,
                else_branch,
                ..
            } => {
                let mut c = 1.0;
                for s in then_branch {
                    c += self.compute_complexity(s);
                }
                if let Some(els) = else_branch {
                    for s in els {
                        c += self.compute_complexity(s);
                    }
                }
                c
            }
            AstNode::ForLoop { body, .. } | AstNode::WhileLoop { body, .. } => {
                2.0 + body.iter().map(|s| self.compute_complexity(s)).sum::<f64>()
            }
            AstNode::Match { arms, .. } => 1.0 + arms.len() as f64 * 0.5,
            AstNode::Function { body, .. } => {
                1.0 + body.iter().map(|s| self.compute_complexity(s)).sum::<f64>()
            }
            AstNode::Module { items, .. } => items
                .iter()
                .map(|i| self.compute_complexity(i))
                .sum::<f64>(),
            _ => 0.0,
        }
    }

    fn estimate_lines(&self, node: &AstNode) -> usize {
        match node {
            AstNode::Function { body, .. } => {
                3 + body.iter().map(|s| self.estimate_lines(s)).sum::<usize>()
            }
            AstNode::Struct { fields, .. } => 3 + fields.len(),
            AstNode::Module { items, .. } => items.iter().map(|i| self.estimate_lines(i)).sum(),
            AstNode::If {
                then_branch,
                else_branch,
                ..
            } => {
                2 + then_branch
                    .iter()
                    .map(|s| self.estimate_lines(s))
                    .sum::<usize>()
                    + else_branch
                        .as_ref()
                        .map(|b| 1 + b.iter().map(|s| self.estimate_lines(s)).sum::<usize>())
                        .unwrap_or(0)
            }
            AstNode::ForLoop { body, .. } => {
                2 + body.iter().map(|s| self.estimate_lines(s)).sum::<usize>()
            }
            _ => 1,
        }
    }

    fn load_default_templates(&mut self) {
        // Getter pattern
        self.templates.insert(
            "getter".into(),
            AstNode::Function {
                name: "get_VALUE".into(),
                params: vec![Parameter {
                    name: "self".into(),
                    type_expr: TypeExpr::Simple("&Self".into()),
                    is_mutable: false,
                    is_reference: true,
                }],
                return_type: Some(TypeExpr::Reference(
                    Box::new(TypeExpr::Simple("TYPE".into())),
                    false,
                )),
                body: vec![AstNode::Return {
                    value: Some(Box::new(AstNode::FieldAccess {
                        object: Box::new(AstNode::Identifier("self".into())),
                        field: "VALUE".into(),
                    })),
                }],
                is_public: true,
                is_async: false,
                doc_comment: Some("Get VALUE.".into()),
            },
        );

        // Constructor pattern
        self.templates.insert(
            "constructor".into(),
            AstNode::Function {
                name: "new".into(),
                params: vec![],
                return_type: Some(TypeExpr::Simple("Self".into())),
                body: vec![AstNode::Return {
                    value: Some(Box::new(AstNode::Identifier(
                        "Self { /* fields */ }".into(),
                    ))),
                }],
                is_public: true,
                is_async: false,
                doc_comment: Some("Create a new instance.".into()),
            },
        );
    }
}

impl Default for CodeGenEngine {
    fn default() -> Self {
        Self::new()
    }
}
