//! Stream messages (RFC §11).

use serde::{Deserialize, Serialize};

use crate::error::ErrorCode;

/// Stream kind discriminator (RFC §11.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StreamKind {
    /// Plain text.
    Text,
    /// Opaque bytes (base64 in v0.1).
    Binary,
    /// Structured JSON events.
    Event,
    /// Structured log lines.
    Log,
    /// Telemetry samples.
    Metric,
    /// Model reasoning / chain-of-thought.
    Thought,
}

/// Payload for `stream.open`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamOpenPayload {
    /// Stream kind.
    pub kind: StreamKind,
    /// Content type (e.g. `text/plain`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    /// Encoding (e.g. `utf-8`, `base64`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encoding: Option<String>,
}

/// Payload for `stream.chunk`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamChunkPayload {
    /// Per-stream sequence number for ordering.
    pub sequence: u64,
    /// Inline data.
    ///
    /// For `kind: text` this is a string; for `kind: binary` it is base64.
    /// For structured kinds (`event`, `log`, `metric`, `thought`) the value
    /// is a JSON object whose schema is determined by the stream kind.
    pub data: serde_json::Value,
    /// Optional content type override (per-chunk, rare).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    /// Optional integrity hash for binary chunks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    /// True if this is a redacted thought chunk (RFC §11.4).
    #[serde(default, skip_serializing_if = "is_false")]
    pub redacted: bool,
    /// Optional role marker for thought chunks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

#[allow(clippy::trivially_copy_pass_by_ref)]
const fn is_false(b: &bool) -> bool {
    !*b
}

/// Payload for `stream.close`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamClosePayload {
    /// Optional final sequence number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub final_sequence: Option<u64>,
}

/// Payload for `stream.error`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamErrorPayload {
    /// Canonical error code.
    pub code: ErrorCode,
    /// Human-readable message.
    pub message: String,
}
