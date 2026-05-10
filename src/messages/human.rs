//! Human-in-the-loop messages (RFC §12).

use serde::{Deserialize, Serialize};

use crate::error::ErrorCode;

/// Payload for `human.input.request`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanInputRequestPayload {
    /// Prompt shown to the operator.
    pub prompt: String,
    /// JSON Schema (draft 2020-12) the response must validate against.
    pub response_schema: serde_json::Value,
    /// Default response if the deadline elapses (PLAN.md §A4 choice).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
    /// Absolute deadline.
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

/// Payload for `human.input.response`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanInputResponsePayload {
    /// Operator-supplied value (validated against the request schema).
    pub value: serde_json::Value,
    /// Audit trail of which channel produced the answer.
    pub responded_by: String,
    /// When the response was produced.
    pub responded_at: chrono::DateTime<chrono::Utc>,
}

/// One option in a `human.choice.request`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChoiceOption {
    /// Stable identifier returned in the response.
    pub id: String,
    /// Human-readable label.
    pub label: String,
}

/// Payload for `human.choice.request`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanChoiceRequestPayload {
    /// Prompt shown to the operator.
    pub prompt: String,
    /// Choice options.
    pub options: Vec<ChoiceOption>,
    /// Absolute deadline.
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

/// Payload for `human.choice.response`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanChoiceResponsePayload {
    /// Identifier of the chosen option.
    pub choice_id: String,
    /// Audit trail.
    pub responded_by: String,
    /// When the response was produced.
    pub responded_at: chrono::DateTime<chrono::Utc>,
}

/// Payload for `human.input.cancelled` — emitted when a request is
/// cancelled (deadline elapsed without a default, or fan-out resolved).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanInputCancelledPayload {
    /// Cancellation code.
    pub code: ErrorCode,
    /// Optional human-readable message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}
