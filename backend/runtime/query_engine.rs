use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct QueryConfig {
    pub max_turns: usize,
    pub max_budget_tokens: u64,
    pub compact_after_turns: usize,
    pub structured_output: bool,
}

impl Default for QueryConfig {
    fn default() -> Self {
        Self {
            max_turns: 8,
            max_budget_tokens: 2_000,
            compact_after_turns: 12,
            structured_output: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TurnResult {
    pub request_signal: String,
    pub output: String,
    pub matched_commands: Vec<String>,
    pub matched_tools: Vec<String>,
    pub usage_input_tokens: u64,
    pub usage_output_tokens: u64,
    pub stop_reason: StopReason,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StopReason {
    Completed,
    MaxTurnsReached,
    MaxBudgetReached,
    Error(String),
}

#[derive(Debug, Clone)]
pub struct StreamEvent {
    pub event_type: String,
    pub data: HashMap<String, String>,
}

pub struct ConnectionEngine {
    config: QueryConfig,
    session_id: String,
    signals: Vec<String>,
    total_input_tokens: u64,
    total_output_tokens: u64,
    turn_count: usize,
}

impl ConnectionEngine {
    #[must_use]
    pub fn new(session_id: &str, config: QueryConfig) -> Self {
        Self {
            config,
            session_id: session_id.to_string(),
            signals: Vec::new(),
            total_input_tokens: 0,
            total_output_tokens: 0,
            turn_count: 0,
        }
    }

    #[must_use]
    pub fn submit(
        &mut self,
        request_signal: &str,
        commands: &[String],
        tools: &[String],
    ) -> TurnResult {
        self.turn_count += 1;

        if self.signals.len() >= self.config.max_turns {
            return TurnResult {
                request_signal: request_signal.to_string(),
                output: format!(
                    "Maximum coordinated turns ({}) reached before processing {}",
                    self.config.max_turns, request_signal
                ),
                matched_commands: commands.to_vec(),
                matched_tools: tools.to_vec(),
                usage_input_tokens: self.total_input_tokens,
                usage_output_tokens: self.total_output_tokens,
                stop_reason: StopReason::MaxTurnsReached,
            };
        }

        let input_tokens = (request_signal.len() / 4) as u64;
        let output_estimate = input_tokens / 2;
        self.total_input_tokens += input_tokens;
        self.total_output_tokens += output_estimate;

        let total_budget = self.total_input_tokens + self.total_output_tokens;
        let stop_reason = if total_budget > self.config.max_budget_tokens {
            StopReason::MaxBudgetReached
        } else {
            StopReason::Completed
        };

        let output = format!(
            "Signal: {}\nMatched commands: {}\nMatched tools: {}",
            request_signal,
            if commands.is_empty() {
                "none".to_string()
            } else {
                commands.join(", ")
            },
            if tools.is_empty() {
                "none".to_string()
            } else {
                tools.join(", ")
            },
        );

        self.signals.push(request_signal.to_string());
        self.compact_if_needed();

        TurnResult {
            request_signal: request_signal.to_string(),
            output,
            matched_commands: commands.to_vec(),
            matched_tools: tools.to_vec(),
            usage_input_tokens: self.total_input_tokens,
            usage_output_tokens: self.total_output_tokens,
            stop_reason,
        }
    }

    #[must_use]
    pub fn stream_submit(
        &mut self,
        request_signal: &str,
        commands: &[String],
        tools: &[String],
    ) -> Vec<StreamEvent> {
        let mut events = Vec::new();
        events.push(StreamEvent {
            event_type: "signal_start".into(),
            data: [
                ("session_id".into(), self.session_id.clone()),
                ("signal".into(), request_signal.to_string()),
            ]
            .into_iter()
            .collect(),
        });

        if !commands.is_empty() {
            events.push(StreamEvent {
                event_type: "command_match".into(),
                data: [("commands".into(), commands.join(","))]
                    .into_iter()
                    .collect(),
            });
        }

        if !tools.is_empty() {
            events.push(StreamEvent {
                event_type: "tool_match".into(),
                data: [("tools".into(), tools.join(","))].into_iter().collect(),
            });
        }

        let result = self.submit(request_signal, commands, tools);
        events.push(StreamEvent {
            event_type: "signal_delta".into(),
            data: [("text".into(), result.output.clone())]
                .into_iter()
                .collect(),
        });
        events.push(StreamEvent {
            event_type: "signal_stop".into(),
            data: [
                ("stop_reason".into(), format!("{:?}", result.stop_reason)),
                ("input_tokens".into(), result.usage_input_tokens.to_string()),
                (
                    "output_tokens".into(),
                    result.usage_output_tokens.to_string(),
                ),
            ]
            .into_iter()
            .collect(),
        });
        events
    }

    #[must_use]
    pub fn run_turn_loop(
        &mut self,
        request_signal: &str,
        commands: &[String],
        tools: &[String],
        max_turns: usize,
    ) -> Vec<TurnResult> {
        let mut results = Vec::new();
        for turn in 0..max_turns {
            let turn_signal = if turn == 0 {
                request_signal.to_string()
            } else {
                format!("{request_signal} [turn {}]", turn + 1)
            };
            let result = self.submit(&turn_signal, commands, tools);
            let should_stop = result.stop_reason != StopReason::Completed;
            results.push(result);
            if should_stop {
                break;
            }
        }
        results
    }

    fn compact_if_needed(&mut self) {
        if self.signals.len() > self.config.compact_after_turns {
            let drain = self.signals.len() - self.config.compact_after_turns;
            self.signals.drain(0..drain);
        }
    }

    #[must_use]
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    #[must_use]
    pub fn turn_count(&self) -> usize {
        self.turn_count
    }
}
