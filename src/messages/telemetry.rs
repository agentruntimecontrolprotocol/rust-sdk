//! Telemetry messages (RFC §17).
//!
//! Re-exports the [`EventEmitPayload`], [`LogPayload`], and [`MetricPayload`]
//! types defined in the parent [`crate::messages`] module so the dedicated
//! telemetry submodule can carry the §17.1 trace span payload alongside.

pub use crate::messages::{EventEmitPayload, LogLevel, LogPayload, MetricPayload};
use serde::{Deserialize, Serialize};

use crate::ids::{SpanId, TraceId};

/// Payload for `trace.span` — propagates an OpenTelemetry-style span event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceSpanPayload {
    /// Span name (e.g. operation identifier).
    pub name: String,
    /// Trace id (must match envelope's `trace_id`).
    pub trace_id: TraceId,
    /// Span id.
    pub span_id: SpanId,
    /// Optional parent span id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_span_id: Option<SpanId>,
    /// Span start time.
    pub start_time: chrono::DateTime<chrono::Utc>,
    /// Span end time.
    pub end_time: chrono::DateTime<chrono::Utc>,
    /// Optional attributes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attributes: Option<serde_json::Value>,
}
