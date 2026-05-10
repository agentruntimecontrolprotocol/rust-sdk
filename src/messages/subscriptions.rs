//! Subscription messages (RFC §13).

use serde::{Deserialize, Serialize};

use crate::envelope::Priority;
use crate::error::ErrorCode;
use crate::ids::{JobId, MessageId, SessionId, StreamId, SubscriptionId, TraceId};

/// Filter clauses for a `subscribe` request (RFC §13.2).
///
/// Within a clause, list elements are OR'ed; across clauses, all conditions
/// are AND'ed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubscriptionFilter {
    /// Match envelopes whose `session_id` is in this list.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub session_id: Vec<SessionId>,
    /// Match envelopes whose `trace_id` is in this list.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trace_id: Vec<TraceId>,
    /// Match envelopes whose `job_id` is in this list.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub job_id: Vec<JobId>,
    /// Match envelopes whose `stream_id` is in this list.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stream_id: Vec<StreamId>,
    /// Match envelopes whose `type` is in this list.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub types: Vec<String>,
    /// Minimum priority.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_priority: Option<Priority>,
}

/// `since` clause for backfill (RFC §13.3).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubscriptionSince {
    /// Replay strictly after this message id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_message_id: Option<MessageId>,
}

/// Payload for `subscribe`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubscribePayload {
    /// Filter.
    #[serde(default)]
    pub filter: SubscriptionFilter,
    /// Optional backfill clause.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<SubscriptionSince>,
}

/// Payload for `subscribe.accepted`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubscribeAcceptedPayload {
    /// Newly minted subscription id.
    pub subscription_id: SubscriptionId,
}

/// Payload for `subscribe.event` — wraps another envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubscribeEventPayload {
    /// The wrapped event envelope (as JSON to avoid recursive enum issues).
    pub event: serde_json::Value,
}

/// Payload for `unsubscribe`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnsubscribePayload {
    /// Subscription to terminate.
    pub subscription_id: SubscriptionId,
}

/// Payload for `subscribe.closed`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubscribeClosedPayload {
    /// Subscription that was closed.
    pub subscription_id: SubscriptionId,
    /// Reason code.
    pub code: ErrorCode,
    /// Free-form reason.
    pub reason: String,
}
