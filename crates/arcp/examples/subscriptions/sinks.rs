//! Three Observer sinks. Wire these to structlog / SQLite / OTLP in your tree.

#![allow(
    unreachable_pub,
    clippy::todo,
    clippy::unimplemented,
    dead_code,
    unused_variables
)]

use arcp::Envelope;

pub struct StdoutSink;
pub struct SqliteSink {
    pub path: String,
}
pub struct OtlpSink {
    pub endpoint: String,
}

impl StdoutSink {
    pub async fn handle(&self, _env: Envelope) {
        // structlog-style summarizer
        todo!()
    }
}

impl SqliteSink {
    pub async fn handle(&self, _env: Envelope) {
        // arcp::store::eventlog schema for replay
        todo!()
    }
}

impl OtlpSink {
    pub async fn handle(&self, _env: Envelope) {
        // OTLP metric / trace.span exporter
        todo!()
    }
}
