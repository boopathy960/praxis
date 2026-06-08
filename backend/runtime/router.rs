#[derive(Debug, Clone)]
pub struct RoutedMatch {
    pub kind: MatchKind,
    pub name: String,
    pub source_hint: String,
    pub score: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MatchKind {
    Command,
    Tool,
}

#[derive(Debug, Clone)]
pub struct RoutingEntry {
    pub name: String,
    pub source_hint: String,
    pub responsibility: String,
    pub kind: MatchKind,
}

pub struct IntentRouter {
    entries: Vec<RoutingEntry>,
}

impl IntentRouter {
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn register(&mut self, entry: RoutingEntry) {
        self.entries.push(entry);
    }

    #[must_use]
    pub fn route(&self, request_signal: &str, limit: usize) -> Vec<RoutedMatch> {
        let signal_terms = request_signal
            .to_lowercase()
            .replace('/', " ")
            .replace('-', " ")
            .replace('_', " ")
            .split_whitespace()
            .filter(|term| !term.is_empty())
            .map(String::from)
            .collect::<std::collections::HashSet<_>>();

        let mut command_matches = Vec::new();
        let mut tool_matches = Vec::new();

        for entry in &self.entries {
            let score = self.score(&signal_terms, entry);
            if score == 0 {
                continue;
            }

            let matched = RoutedMatch {
                kind: entry.kind.clone(),
                name: entry.name.clone(),
                source_hint: entry.source_hint.clone(),
                score,
            };

            match entry.kind {
                MatchKind::Command => command_matches.push(matched),
                MatchKind::Tool => tool_matches.push(matched),
            }
        }

        command_matches.sort_by(|left, right| right.score.cmp(&left.score));
        tool_matches.sort_by(|left, right| right.score.cmp(&left.score));

        let mut selected = Vec::new();
        if let Some(command) = command_matches.first() {
            selected.push(command.clone());
        }
        if let Some(tool) = tool_matches.first() {
            selected.push(tool.clone());
        }

        let mut remaining = command_matches
            .into_iter()
            .skip(1)
            .chain(tool_matches.into_iter().skip(1))
            .collect::<Vec<_>>();
        remaining.sort_by(|left, right| right.score.cmp(&left.score));

        let slots_left = limit.saturating_sub(selected.len());
        selected.extend(remaining.into_iter().take(slots_left));
        selected.truncate(limit);
        selected
    }

    fn score(
        &self,
        signal_terms: &std::collections::HashSet<String>,
        entry: &RoutingEntry,
    ) -> usize {
        let haystacks = [
            entry.name.to_lowercase(),
            entry.source_hint.to_lowercase(),
            entry.responsibility.to_lowercase(),
        ];

        signal_terms
            .iter()
            .filter(|term| {
                haystacks
                    .iter()
                    .any(|haystack| haystack.contains(term.as_str()))
            })
            .count()
    }
}

impl Default for IntentRouter {
    fn default() -> Self {
        Self::new()
    }
}
