// ─────────────────────────────────────────────────────────────
// Tool Forge — Dynamic Tool Creation
// ─────────────────────────────────────────────────────────────
// Port of backend/agents/tools/tool_forge.py

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: Vec<ToolParam>,
    pub output_type: String,
    pub primitives: Vec<String>,
    pub composition_rule: CompositionRule,
}

#[derive(Debug, Clone)]
pub struct ToolParam {
    pub name: String,
    pub param_type: String,
    pub required: bool,
    pub default: Option<String>,
}

#[derive(Debug, Clone)]
pub enum CompositionRule {
    Sequential,  // Run primitives in order
    Parallel,    // Run all, merge results
    Conditional, // Run based on input conditions
    Pipeline,    // Output of one feeds into next
}

/// Dynamically creates tools from primitives and specifications.
pub struct ToolForge {
    templates: HashMap<String, ToolSpec>,
    forged_count: u64,
}

impl ToolForge {
    pub fn new() -> Self {
        Self {
            templates: HashMap::new(),
            forged_count: 0,
        }
    }

    /// Register a tool template for later instantiation.
    pub fn register_template(&mut self, spec: ToolSpec) {
        self.templates.insert(spec.name.clone(), spec);
    }

    /// Forge a new tool from a description and available primitives.
    pub fn forge(&mut self, name: &str, description: &str, primitives: Vec<String>) -> ToolSpec {
        self.forged_count += 1;

        // Auto-detect composition rule based on primitives
        let rule = if primitives.len() == 1 {
            CompositionRule::Sequential
        } else if primitives
            .iter()
            .any(|p| p.contains("filter") || p.contains("if"))
        {
            CompositionRule::Conditional
        } else if primitives
            .iter()
            .any(|p| p.contains("transform") || p.contains("map"))
        {
            CompositionRule::Pipeline
        } else {
            CompositionRule::Sequential
        };

        let spec = ToolSpec {
            name: name.to_string(),
            description: description.to_string(),
            input_schema: vec![ToolParam {
                name: "input".to_string(),
                param_type: "String".to_string(),
                required: true,
                default: None,
            }],
            output_type: "String".to_string(),
            primitives,
            composition_rule: rule,
        };

        self.templates.insert(name.to_string(), spec.clone());
        spec
    }

    /// Compose primitives according to a composition rule.
    pub fn compose(&self, spec: &ToolSpec, inputs: &HashMap<String, String>) -> String {
        match &spec.composition_rule {
            CompositionRule::Sequential => spec
                .primitives
                .iter()
                .map(|p| format!("[{}]({})", p, inputs.get("input").unwrap_or(&String::new())))
                .collect::<Vec<_>>()
                .join(" → "),
            CompositionRule::Pipeline => {
                let input = inputs.get("input").cloned().unwrap_or_default();
                spec.primitives
                    .iter()
                    .fold(input, |acc, p| format!("{}({})", p, acc))
            }
            CompositionRule::Parallel => spec
                .primitives
                .iter()
                .map(|p| format!("{}(parallel)", p))
                .collect::<Vec<_>>()
                .join(" | "),
            CompositionRule::Conditional => {
                format!(
                    "if condition {{ {} }} else {{ {} }}",
                    spec.primitives.first().unwrap_or(&String::new()),
                    spec.primitives.last().unwrap_or(&String::new())
                )
            }
        }
    }

    pub fn forged_count(&self) -> u64 {
        self.forged_count
    }
    pub fn templates(&self) -> &HashMap<String, ToolSpec> {
        &self.templates
    }
}

impl Default for ToolForge {
    fn default() -> Self {
        Self::new()
    }
}
