//! Control plane messages. Spans several ARCP v1.1 surfaces:
//!
//! - Cancellation — §7.4.
//! - Acknowledgement / negative acknowledgement — §6.5.
//! - Resume — §6.3 (the v1.1 form is `session.resume`; the [`ResumePayload`]
//!   here is the SDK's older `resume` envelope carrying
//!   `after_message_id`/`checkpoint_id` and is retained for compatibility).
//! - Backpressure — implementation-defined under §6.5's slow-consumer
//!   guidance.
//! - `interrupt` — SDK extension; v1.1 explicitly defers pause/unpause
//!   surfaces.

use serde::{Deserialize, Serialize};

use crate::error::ErrorCode;
use crate::ids::MessageId;

pub use crate::messages::{PingPayload, PongPayload};

/// Payload for `ack` — generic acknowledgement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AckPayload {
    /// Optional human-readable note.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// Payload for `nack` — negative acknowledgement; carries an error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NackPayload {
    /// Canonical error code.
    pub code: ErrorCode,
    /// Human-readable explanation.
    pub message: String,
    /// Optional structured details.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

/// Cancellation target discriminator (ARCP v1.1 §7.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CancelTargetKind {
    /// Cancel a job.
    Job,
    /// Cancel a stream.
    Stream,
    /// Cancel an entire session.
    Session,
}

/// Payload for `cancel` (ARCP v1.1 §7.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CancelPayload {
    /// Kind of target.
    pub target: CancelTargetKind,
    /// Identifier of the target.
    pub target_id: String,
    /// Free-form reason.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Cooperative-cancel deadline in milliseconds. Default per A4.7 = 5000.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline_ms: Option<u64>,
}

/// Payload for `cancel.accepted`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CancelAcceptedPayload {
    /// Echo of the cancel target id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_id: Option<String>,
}

/// Payload for `cancel.refused`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CancelRefusedPayload {
    /// Echo of the cancel target id.
    pub target_id: String,
    /// Reason for refusal.
    pub reason: String,
}

/// Payload for `interrupt`. SDK extension; ARCP v1.1 §1 explicitly defers
/// pause/unpause from the v1.1 surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InterruptPayload {
    /// Kind of target.
    pub target: CancelTargetKind,
    /// Identifier of the target.
    pub target_id: String,
    /// Operator-supplied prompt for the human follow-up.
    pub prompt: String,
}

/// Payload for the SDK's legacy `resume` envelope.
///
/// The v1.1 resume contract is `session.resume` (ARCP v1.1 §6.3) carrying
/// `last_event_seq` and `resume_token`; this older shape
/// (`after_message_id` / `checkpoint_id`) is retained for backward
/// compatibility with v1.0 peers.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResumePayload {
    /// Replay starting strictly after this message id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_message_id: Option<MessageId>,
    /// Checkpoint id to restore from (v0.2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint_id: Option<String>,
    /// If true, re-open active streams as part of the resume.
    #[serde(default, skip_serializing_if = "is_false")]
    pub include_open_streams: bool,
}

#[allow(clippy::trivially_copy_pass_by_ref)]
const fn is_false(b: &bool) -> bool {
    !*b
}

/// Payload for `backpressure`. Implementation-defined under ARCP v1.1
/// §6.5's slow-consumer guidance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackpressurePayload {
    /// Desired chunk-rate in chunks per second (PLAN.md §A4.8 choice).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub desired_rate_per_second: Option<u32>,
    /// Approximate buffer headroom in bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub buffer_remaining_bytes: Option<u64>,
    /// Free-form reason.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}
