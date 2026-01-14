//! End-to-end latency tracing inspired by Google Dapper
//!
//! Provides request tracing through the nvim-web stack for debugging latency.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

static TRACE_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

/// A span represents a single operation within a trace
#[derive(Debug, Clone)]
pub struct Span {
    pub name: String,
    pub start_us: u64,
    pub duration_us: Option<u64>,
}

/// A trace represents a complete request lifecycle
#[derive(Debug, Clone)]
pub struct Trace {
    pub id: u64,
    pub spans: Vec<Span>,
    pub start: Instant,
    pub metadata: HashMap<String, String>,
}

impl Trace {
    /// Start a new trace with a unique ID
    pub fn new() -> Self {
        Self {
            id: TRACE_ID_COUNTER.fetch_add(1, Ordering::SeqCst),
            spans: Vec::new(),
            start: Instant::now(),
            metadata: HashMap::new(),
        }
    }

    /// Start a named span
    pub fn start_span(&mut self, name: &str) -> usize {
        let span = Span {
            name: name.to_string(),
            start_us: self.start.elapsed().as_micros() as u64,
            duration_us: None,
        };
        self.spans.push(span);
        self.spans.len() - 1
    }

    /// End a span by index
    pub fn end_span(&mut self, idx: usize) {
        if let Some(span) = self.spans.get_mut(idx) {
            let now_us = self.start.elapsed().as_micros() as u64;
            span.duration_us = Some(now_us - span.start_us);
        }
    }

    /// Add metadata to the trace
    pub fn add_metadata(&mut self, key: &str, value: &str) {
        self.metadata.insert(key.to_string(), value.to_string());
    }

    /// Total trace duration in microseconds
    pub fn total_us(&self) -> u64 {
        self.start.elapsed().as_micros() as u64
    }

    /// Format trace for logging
    pub fn format_log(&self) -> String {
        let mut lines = vec![format!("Trace #{} ({}us total)", self.id, self.total_us())];

        for span in &self.spans {
            let duration = span.duration_us.unwrap_or(0);
            lines.push(format!("  {} {}us", span.name, duration));
        }

        if !self.metadata.is_empty() {
            lines.push("  metadata:".to_string());
            for (k, v) in &self.metadata {
                lines.push(format!("    {}: {}", k, v));
            }
        }

        lines.join("\n")
    }
}

impl Default for Trace {
    fn default() -> Self {
        Self::new()
    }
}

/// Macro for tracing a code block
#[macro_export]
macro_rules! trace_span {
    ($trace:expr, $name:expr, $body:block) => {{
        let span_idx = $trace.start_span($name);
        let result = $body;
        $trace.end_span(span_idx);
        result
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_trace_spans() {
        let mut trace = Trace::new();

        let idx = trace.start_span("websocket");
        thread::sleep(Duration::from_millis(1));
        trace.end_span(idx);

        assert_eq!(trace.spans.len(), 1);
        assert!(trace.spans[0].duration_us.unwrap() >= 1000);
    }
}
