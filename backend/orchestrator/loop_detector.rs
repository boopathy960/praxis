// Loop Detector — Port of backend/agents/loop_detector.py
use std::collections::VecDeque;

pub struct LoopDetector {
    call_history: VecDeque<String>,
    window_size: usize,
    max_repeats: usize,
    total_loops_detected: u64,
}
impl LoopDetector {
    pub fn new(window_size: usize, max_repeats: usize) -> Self {
        Self {
            call_history: VecDeque::new(),
            window_size,
            max_repeats,
            total_loops_detected: 0,
        }
    }
    pub fn record_call(&mut self, signature: &str) -> bool {
        self.call_history.push_back(signature.to_string());
        if self.call_history.len() > self.window_size {
            self.call_history.pop_front();
        }
        self.detect_loop(signature)
    }
    fn detect_loop(&mut self, signature: &str) -> bool {
        let count = self
            .call_history
            .iter()
            .filter(|s| s.as_str() == signature)
            .count();
        if count >= self.max_repeats {
            self.total_loops_detected += 1;
            true
        } else {
            // Check for pattern loops (A->B->A->B)
            if self.call_history.len() >= 4 {
                let len = self.call_history.len();
                let a = &self.call_history[len - 4];
                let b = &self.call_history[len - 3];
                let c = &self.call_history[len - 2];
                let d = &self.call_history[len - 1];
                if a == c && b == d {
                    self.total_loops_detected += 1;
                    return true;
                }
            }
            false
        }
    }
    pub fn clear(&mut self) {
        self.call_history.clear();
    }
    pub fn total_detected(&self) -> u64 {
        self.total_loops_detected
    }
}
impl Default for LoopDetector {
    fn default() -> Self {
        Self::new(20, 3)
    }
}
