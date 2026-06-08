// ─────────────────────────────────────────────────────────────
// Tool Fabricator — On-the-Fly Tool Synthesis from Primitives
// ─────────────────────────────────────────────────────────────
// Port of backend/brain/tool_fabricator.py

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct FabricatedTool {
    pub name: String,
    pub description: String,
    pub primitives_used: Vec<String>,
    pub code: String,
    pub confidence: f64,
}

/// Primitive operations that can be composed into tools.
#[derive(Debug, Clone)]
pub struct Primitive {
    pub name: String,
    pub category: String,
    pub signature: String,
    pub description: String,
}

/// Tool Fabricator — synthesizes tools from primitive operations.
pub struct ToolFabricator {
    primitives: Vec<Primitive>,
    fabricated_cache: HashMap<String, FabricatedTool>,
}

impl ToolFabricator {
    pub fn new() -> Self {
        let primitives = vec![
            Primitive {
                name: "read_file".into(),
                category: "io".into(),
                signature: "fn(path: &str) -> String".into(),
                description: "Read file contents".into(),
            },
            Primitive {
                name: "write_file".into(),
                category: "io".into(),
                signature: "fn(path: &str, content: &str)".into(),
                description: "Write content to file".into(),
            },
            Primitive {
                name: "http_get".into(),
                category: "network".into(),
                signature: "fn(url: &str) -> String".into(),
                description: "HTTP GET request".into(),
            },
            Primitive {
                name: "http_post".into(),
                category: "network".into(),
                signature: "fn(url: &str, body: &str) -> String".into(),
                description: "HTTP POST request".into(),
            },
            Primitive {
                name: "parse_json".into(),
                category: "data".into(),
                signature: "fn(json: &str) -> Value".into(),
                description: "Parse JSON string".into(),
            },
            Primitive {
                name: "regex_match".into(),
                category: "text".into(),
                signature: "fn(pattern: &str, text: &str) -> Vec<String>".into(),
                description: "Regex pattern matching".into(),
            },
            Primitive {
                name: "hash_sha256".into(),
                category: "crypto".into(),
                signature: "fn(data: &[u8]) -> String".into(),
                description: "SHA-256 hash".into(),
            },
            Primitive {
                name: "sort_items".into(),
                category: "algorithm".into(),
                signature: "fn(items: &mut [T])".into(),
                description: "Sort items".into(),
            },
            Primitive {
                name: "filter_items".into(),
                category: "algorithm".into(),
                signature: "fn(items: &[T], pred: fn(&T) -> bool) -> Vec<T>".into(),
                description: "Filter items by predicate".into(),
            },
            Primitive {
                name: "map_transform".into(),
                category: "algorithm".into(),
                signature: "fn(items: &[T], f: fn(&T) -> U) -> Vec<U>".into(),
                description: "Transform each item".into(),
            },
        ];

        Self {
            primitives,
            fabricated_cache: HashMap::new(),
        }
    }

    /// Fabricate a tool from a natural language description.
    pub fn fabricate(&mut self, description: &str) -> FabricatedTool {
        // Check cache
        if let Some(cached) = self.fabricated_cache.get(description) {
            return cached.clone();
        }

        let desc_lower = description.to_lowercase();
        let mut selected_primitives = Vec::new();

        // Match primitives by relevance
        for prim in &self.primitives {
            let desc_lc = prim.description.to_lowercase();
            let keywords: Vec<&str> = desc_lc.split_whitespace().collect();
            let relevance = keywords
                .iter()
                .filter(|kw| desc_lower.contains(*kw))
                .count();
            if relevance > 0 {
                selected_primitives.push((prim.clone(), relevance));
            }
        }

        selected_primitives.sort_by(|a, b| b.1.cmp(&a.1));
        let top_primitives: Vec<Primitive> = selected_primitives
            .iter()
            .take(3)
            .map(|(p, _)| p.clone())
            .collect();

        // Generate composite tool
        let name = Self::generate_tool_name(description);
        let prim_names: Vec<String> = top_primitives.iter().map(|p| p.name.clone()).collect();

        let code = self.generate_tool_code(&name, &top_primitives);

        let tool = FabricatedTool {
            name: name.clone(),
            description: description.to_string(),
            primitives_used: prim_names,
            code,
            confidence: if top_primitives.is_empty() { 0.3 } else { 0.7 },
        };

        self.fabricated_cache
            .insert(description.to_string(), tool.clone());
        tool
    }

    fn generate_tool_name(description: &str) -> String {
        let words: Vec<&str> = description.split_whitespace().take(3).collect();
        words
            .join("_")
            .to_lowercase()
            .replace(|c: char| !c.is_alphanumeric() && c != '_', "")
    }

    fn generate_tool_code(&self, name: &str, primitives: &[Primitive]) -> String {
        let mut code = format!("// Auto-fabricated tool: {}\n", name);
        code.push_str(&format!(
            "pub fn {}(input: &str) -> Result<String, Box<dyn std::error::Error>> {{\n",
            name
        ));

        for (i, prim) in primitives.iter().enumerate() {
            code.push_str(&format!("    // Step {}: {}\n", i + 1, prim.description));
            code.push_str(&format!(
                "    let step{}_result = {}(input)?;\n",
                i + 1,
                prim.name
            ));
        }

        code.push_str("    Ok(step1_result)\n");
        code.push_str("}\n");
        code
    }

    pub fn list_primitives(&self) -> &[Primitive] {
        &self.primitives
    }

    pub fn cache_size(&self) -> usize {
        self.fabricated_cache.len()
    }
}

impl Default for ToolFabricator {
    fn default() -> Self {
        Self::new()
    }
}
