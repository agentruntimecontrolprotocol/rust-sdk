//! Subscription messages.
//!
//! Two related-but-distinct surfaces live here:
//!
//! - **v1.0-era envelope subscriptions** ([`SubscribePayload`],
//!   [`SubscribeAcceptedPayload`], [`SubscribeEventPayload`],
//!   [`UnsubscribePayload`], [`SubscribeClosedPayload`]): a generic
//!   filter/backfill bus over the whole envelope stream (RFC §13). These
//!   predate v1.1 and remain for backwards compatibility; new code that
//!   only wants live job events should prefer the v1.1 form below.
//! - **ARCP v1.1 §7.6 job subscriptions** ([`JobSubscribePayload`],
//!   [`JobSubscribedPayload`], [`JobUnsubscribePayload`]): a cross-session
//!   attach to a specific job's event stream, optionally with history
//!   replay from a chosen sequence number.

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

// ----------------------------------------------------------------------
// ARCP v1.1 §7.6 — cross-session job subscriptions.
// ----------------------------------------------------------------------

/// Payload for `job.subscribe` (ARCP v1.1 §7.6).
///
/// Lets one session attach to the live event stream of a job that was
/// submitted in (possibly) another session. When `history` is `true`,
/// buffered events with `seq > from_event_seq` are replayed before the
/// runtime resumes live streaming.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobSubscribePayload {
    /// Job to attach to.
    pub job_id: JobId,
    /// Replay floor (exclusive). If `history` is `true`, buffered events
    /// with `seq > from_event_seq` are replayed before live streaming.
    /// When `None`, the subscriber receives only live events.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_event_seq: Option<u64>,
    /// Whether to replay buffered history. Defaults to `false`.
    #[serde(default, skip_serializing_if = "is_false")]
    pub history: bool,
}

/// Payload for `job.subscribed` (ARCP v1.1 §7.6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobSubscribedPayload {
    /// Job that was attached to.
    pub job_id: JobId,
    /// Wire-level status string at subscription time (e.g.
    /// `"running"`, `"completed"`).
    pub current_status: String,
    /// Resolved `name@version` (or bare `name`) the job is running.
    pub agent: String,
    /// Optional parent job id for delegated / child jobs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_job_id: Option<JobId>,
    /// Optional trace identifier inherited from the job.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    /// Event sequence the subscription was attached at (the highest
    /// `event_seq` the runtime had emitted for this job at acknowledgement
    /// time).
    pub subscribed_from: u64,
    /// `true` if buffered history was replayed before the live tail.
    #[serde(default, skip_serializing_if = "is_false")]
    pub replayed: bool,
}

/// Payload for `job.unsubscribe` (ARCP v1.1 §7.6).
///
/// Cancels a previously acknowledged job subscription. The subscription
/// does NOT grant cancel authority over the job itself — only the
/// originating session may cancel a job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobUnsubscribePayload {
    /// Job whose subscription should be cancelled.
    pub job_id: JobId,
}

#[allow(clippy::trivially_copy_pass_by_ref)]
const fn is_false(b: &bool) -> bool {
    !*b
}
