//! Wire-level message payload types (RFC §6.2).
//!
//! [`MessageType`] is a tagged enum (`#[serde(tag = "type", content =
//! "payload")]`) so on the wire a message renders flat:
//!
//! ```json
//! { "type": "ping", "payload": { "nonce": "..." } }
//! ```
//!
//! When this is `#[serde(flatten)]`-embedded into [`crate::envelope::Envelope`]
//! the `type` and `payload` keys appear at the envelope level alongside the
//! other metadata fields, matching the canonical wire format from §6.1.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub mod artifacts;
pub mod control;
pub mod execution;
pub mod human;
pub mod permissions;
pub mod session;
pub mod streaming;
pub mod subscriptions;
pub mod telemetry;

pub use artifacts::{
    ArtifactFetchPayload, ArtifactPutPayload, ArtifactRef, ArtifactRefPayload,
    ArtifactReleasePayload,
};
pub use control::{
    AckPayload, BackpressurePayload, CancelAcceptedPayload, CancelPayload, CancelRefusedPayload,
    CancelTargetKind, InterruptPayload, NackPayload, ResumePayload,
};
pub use execution::{
    AgentDelegatePayload, AgentHandoffPayload, JobAcceptedPayload, JobCancelledPayload,
    JobCheckpointPayload, JobCompletedPayload, JobFailedPayload, JobHeartbeatPayload,
    JobProgressPayload, JobSchedulePayload, JobStartedPayload, JobState, ToolErrorPayload,
    ToolInvokePayload, ToolResultPayload, WorkflowCompletePayload, WorkflowStartPayload,
};
pub use human::{
    ChoiceOption, HumanChoiceRequestPayload, HumanChoiceResponsePayload,
    HumanInputCancelledPayload, HumanInputRequestPayload, HumanInputResponsePayload,
};
pub use permissions::{
    LeaseExtendedPayload, LeaseGrantedPayload, LeaseRefreshPayload, LeaseRevokedPayload,
    PermissionDenyPayload, PermissionGrantPayload, PermissionRequestPayload, TrustLevel,
};
pub use session::{
    AuthScheme, ClientIdentity, Credentials, RuntimeIdentity, SessionAcceptedPayload,
    SessionAuthenticatePayload, SessionChallengePayload, SessionClosePayload,
    SessionEvictedPayload, SessionLease, SessionOpenPayload, SessionRefreshPayload,
    SessionRejectedPayload, SessionUnauthenticatedPayload,
};
pub use streaming::{
    StreamChunkPayload, StreamClosePayload, StreamErrorPayload, StreamKind, StreamOpenPayload,
};
pub use subscriptions::{
    SubscribeAcceptedPayload, SubscribeClosedPayload, SubscribeEventPayload, SubscribePayload,
    SubscriptionFilter, SubscriptionSince, UnsubscribePayload,
};
pub use telemetry::TraceSpanPayload;

/// Negotiated capability set (RFC §7).
///
/// Absent booleans are interpreted as `false` (RFC §7); the corresponding
/// fields here are `Option<bool>` so the on-the-wire representation can
/// distinguish "false" from "not advertised".
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Capabilities {
    /// Per RFC §4.2 / §7.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub streaming: Option<bool>,
    /// Per RFC §10.1.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub durable_jobs: Option<bool>,
    /// Per RFC §10.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoints: Option<bool>,
    /// Per RFC §11.3.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binary_streams: Option<bool>,
    /// Per RFC §14.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_handoff: Option<bool>,
    /// Per RFC §12.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub human_input: Option<bool>,
    /// Per RFC §16.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifacts: Option<bool>,
    /// Per RFC §13.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscriptions: Option<bool>,
    /// Per RFC §10.6.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheduled_jobs: Option<bool>,
    /// Per RFC §10.5.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interrupt: Option<bool>,
    /// Per PLAN.md §A4 choice — anonymous auth is gated on this.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anonymous: Option<bool>,
    /// Per RFC §10.3 — `"fail"` or `"block"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heartbeat_recovery: Option<String>,
    /// Per RFC §11.3 — supported binary encodings.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub binary_encoding: Vec<String>,
    /// Per RFC §7 — advertised extension namespaces.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<String>,
    /// Per RFC §16.3 — artifact retention policy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_retention: Option<ArtifactRetention>,
    /// Forward-compatibility catch-all for unknown booleans / objects
    /// advertised by the peer (PLAN.md §A4 choice).
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// Artifact retention policy advertised in [`Capabilities`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactRetention {
    /// Default retention in seconds.
    pub default_seconds: u64,
    /// Maximum retention in seconds.
    pub max_seconds: u64,
}

impl Capabilities {
    /// True when `name` is a boolean capability that is set to `true`.
    #[must_use]
    pub const fn has(&self, name: CapabilityName) -> bool {
        match name {
            CapabilityName::Streaming => matches!(self.streaming, Some(true)),
            CapabilityName::DurableJobs => matches!(self.durable_jobs, Some(true)),
            CapabilityName::Checkpoints => matches!(self.checkpoints, Some(true)),
            CapabilityName::BinaryStreams => matches!(self.binary_streams, Some(true)),
            CapabilityName::AgentHandoff => matches!(self.agent_handoff, Some(true)),
            CapabilityName::HumanInput => matches!(self.human_input, Some(true)),
            CapabilityName::Artifacts => matches!(self.artifacts, Some(true)),
            CapabilityName::Subscriptions => matches!(self.subscriptions, Some(true)),
            CapabilityName::ScheduledJobs => matches!(self.scheduled_jobs, Some(true)),
            CapabilityName::Interrupt => matches!(self.interrupt, Some(true)),
            CapabilityName::Anonymous => matches!(self.anonymous, Some(true)),
        }
    }
}

/// Named boolean capability slots, used by [`Capabilities::has`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CapabilityName {
    /// `streaming`
    Streaming,
    /// `durable_jobs`
    DurableJobs,
    /// `checkpoints`
    Checkpoints,
    /// `binary_streams`
    BinaryStreams,
    /// `agent_handoff`
    AgentHandoff,
    /// `human_input`
    HumanInput,
    /// `artifacts`
    Artifacts,
    /// `subscriptions`
    Subscriptions,
    /// `scheduled_jobs`
    ScheduledJobs,
    /// `interrupt`
    Interrupt,
    /// `anonymous` — anonymous auth gate.
    Anonymous,
}

/// Tagged enum of every protocol message payload (RFC §6.2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
#[non_exhaustive]
pub enum MessageType {
    // Identity & authentication
    /// `session.open`
    #[serde(rename = "session.open")]
    SessionOpen(SessionOpenPayload),
    /// `session.challenge`
    #[serde(rename = "session.challenge")]
    SessionChallenge(SessionChallengePayload),
    /// `session.authenticate`
    #[serde(rename = "session.authenticate")]
    SessionAuthenticate(SessionAuthenticatePayload),
    /// `session.accepted`
    #[serde(rename = "session.accepted")]
    SessionAccepted(SessionAcceptedPayload),
    /// `session.unauthenticated`
    #[serde(rename = "session.unauthenticated")]
    SessionUnauthenticated(SessionUnauthenticatedPayload),
    /// `session.rejected`
    #[serde(rename = "session.rejected")]
    SessionRejected(SessionRejectedPayload),
    /// `session.refresh`
    #[serde(rename = "session.refresh")]
    SessionRefresh(SessionRefreshPayload),
    /// `session.evicted`
    #[serde(rename = "session.evicted")]
    SessionEvicted(SessionEvictedPayload),
    /// `session.close`
    #[serde(rename = "session.close")]
    SessionClose(SessionClosePayload),

    // Control
    /// `ping`
    #[serde(rename = "ping")]
    Ping(PingPayload),
    /// `pong`
    #[serde(rename = "pong")]
    Pong(PongPayload),
    /// `ack`
    #[serde(rename = "ack")]
    Ack(AckPayload),
    /// `nack`
    #[serde(rename = "nack")]
    Nack(NackPayload),
    /// `cancel`
    #[serde(rename = "cancel")]
    Cancel(CancelPayload),
    /// `cancel.accepted`
    #[serde(rename = "cancel.accepted")]
    CancelAccepted(CancelAcceptedPayload),
    /// `cancel.refused`
    #[serde(rename = "cancel.refused")]
    CancelRefused(CancelRefusedPayload),
    /// `interrupt`
    #[serde(rename = "interrupt")]
    Interrupt(InterruptPayload),
    /// `resume`
    #[serde(rename = "resume")]
    Resume(ResumePayload),
    /// `backpressure`
    #[serde(rename = "backpressure")]
    Backpressure(BackpressurePayload),

    // Execution
    /// `tool.invoke`
    #[serde(rename = "tool.invoke")]
    ToolInvoke(ToolInvokePayload),
    /// `tool.result`
    #[serde(rename = "tool.result")]
    ToolResult(ToolResultPayload),
    /// `tool.error`
    #[serde(rename = "tool.error")]
    ToolError(ToolErrorPayload),
    /// `job.accepted`
    #[serde(rename = "job.accepted")]
    JobAccepted(JobAcceptedPayload),
    /// `job.started`
    #[serde(rename = "job.started")]
    JobStarted(JobStartedPayload),
    /// `job.progress`
    #[serde(rename = "job.progress")]
    JobProgress(JobProgressPayload),
    /// `job.heartbeat`
    #[serde(rename = "job.heartbeat")]
    JobHeartbeat(JobHeartbeatPayload),
    /// `job.completed`
    #[serde(rename = "job.completed")]
    JobCompleted(JobCompletedPayload),
    /// `job.failed`
    #[serde(rename = "job.failed")]
    JobFailed(JobFailedPayload),
    /// `job.cancelled`
    #[serde(rename = "job.cancelled")]
    JobCancelled(JobCancelledPayload),

    // Streaming
    /// `stream.open`
    #[serde(rename = "stream.open")]
    StreamOpen(StreamOpenPayload),
    /// `stream.chunk`
    #[serde(rename = "stream.chunk")]
    StreamChunk(StreamChunkPayload),
    /// `stream.close`
    #[serde(rename = "stream.close")]
    StreamClose(StreamClosePayload),
    /// `stream.error`
    #[serde(rename = "stream.error")]
    StreamError(StreamErrorPayload),

    // Human-in-the-loop
    /// `human.input.request`
    #[serde(rename = "human.input.request")]
    HumanInputRequest(HumanInputRequestPayload),
    /// `human.input.response`
    #[serde(rename = "human.input.response")]
    HumanInputResponse(HumanInputResponsePayload),
    /// `human.choice.request`
    #[serde(rename = "human.choice.request")]
    HumanChoiceRequest(HumanChoiceRequestPayload),
    /// `human.choice.response`
    #[serde(rename = "human.choice.response")]
    HumanChoiceResponse(HumanChoiceResponsePayload),
    /// `human.input.cancelled`
    #[serde(rename = "human.input.cancelled")]
    HumanInputCancelled(HumanInputCancelledPayload),

    // Permissions & leases
    /// `permission.request`
    #[serde(rename = "permission.request")]
    PermissionRequest(PermissionRequestPayload),
    /// `permission.grant`
    #[serde(rename = "permission.grant")]
    PermissionGrant(PermissionGrantPayload),
    /// `permission.deny`
    #[serde(rename = "permission.deny")]
    PermissionDeny(PermissionDenyPayload),
    /// `lease.granted`
    #[serde(rename = "lease.granted")]
    LeaseGranted(LeaseGrantedPayload),
    /// `lease.extended`
    #[serde(rename = "lease.extended")]
    LeaseExtended(LeaseExtendedPayload),
    /// `lease.revoked`
    #[serde(rename = "lease.revoked")]
    LeaseRevoked(LeaseRevokedPayload),
    /// `lease.refresh`
    #[serde(rename = "lease.refresh")]
    LeaseRefresh(LeaseRefreshPayload),

    // Subscriptions
    /// `subscribe`
    #[serde(rename = "subscribe")]
    Subscribe(SubscribePayload),
    /// `subscribe.accepted`
    #[serde(rename = "subscribe.accepted")]
    SubscribeAccepted(SubscribeAcceptedPayload),
    /// `subscribe.event`
    #[serde(rename = "subscribe.event")]
    SubscribeEvent(SubscribeEventPayload),
    /// `unsubscribe`
    #[serde(rename = "unsubscribe")]
    Unsubscribe(UnsubscribePayload),
    /// `subscribe.closed`
    #[serde(rename = "subscribe.closed")]
    SubscribeClosed(SubscribeClosedPayload),

    // Artifacts
    /// `artifact.put`
    #[serde(rename = "artifact.put")]
    ArtifactPut(ArtifactPutPayload),
    /// `artifact.fetch`
    #[serde(rename = "artifact.fetch")]
    ArtifactFetch(ArtifactFetchPayload),
    /// `artifact.ref`
    #[serde(rename = "artifact.ref")]
    ArtifactRef(ArtifactRefPayload),
    /// `artifact.release`
    #[serde(rename = "artifact.release")]
    ArtifactRelease(ArtifactReleasePayload),

    // Telemetry
    /// `event.emit`
    #[serde(rename = "event.emit")]
    EventEmit(EventEmitPayload),
    /// `log`
    #[serde(rename = "log")]
    Log(LogPayload),
    /// `metric`
    #[serde(rename = "metric")]
    Metric(MetricPayload),
    /// `trace.span`
    #[serde(rename = "trace.span")]
    TraceSpan(TraceSpanPayload),
}

impl MessageType {
    /// Wire-level discriminator string for this variant.
    #[must_use]
    pub const fn type_name(&self) -> &'static str {
        match self {
            Self::SessionOpen(_) => "session.open",
            Self::SessionChallenge(_) => "session.challenge",
            Self::SessionAuthenticate(_) => "session.authenticate",
            Self::SessionAccepted(_) => "session.accepted",
            Self::SessionUnauthenticated(_) => "session.unauthenticated",
            Self::SessionRejected(_) => "session.rejected",
            Self::SessionRefresh(_) => "session.refresh",
            Self::SessionEvicted(_) => "session.evicted",
            Self::SessionClose(_) => "session.close",
            Self::Ping(_) => "ping",
            Self::Pong(_) => "pong",
            Self::Ack(_) => "ack",
            Self::Nack(_) => "nack",
            Self::Cancel(_) => "cancel",
            Self::CancelAccepted(_) => "cancel.accepted",
            Self::CancelRefused(_) => "cancel.refused",
            Self::Interrupt(_) => "interrupt",
            Self::Resume(_) => "resume",
            Self::Backpressure(_) => "backpressure",
            Self::ToolInvoke(_) => "tool.invoke",
            Self::ToolResult(_) => "tool.result",
            Self::ToolError(_) => "tool.error",
            Self::JobAccepted(_) => "job.accepted",
            Self::JobStarted(_) => "job.started",
            Self::JobProgress(_) => "job.progress",
            Self::JobHeartbeat(_) => "job.heartbeat",
            Self::JobCompleted(_) => "job.completed",
            Self::JobFailed(_) => "job.failed",
            Self::JobCancelled(_) => "job.cancelled",
            Self::StreamOpen(_) => "stream.open",
            Self::StreamChunk(_) => "stream.chunk",
            Self::StreamClose(_) => "stream.close",
            Self::StreamError(_) => "stream.error",
            Self::HumanInputRequest(_) => "human.input.request",
            Self::HumanInputResponse(_) => "human.input.response",
            Self::HumanChoiceRequest(_) => "human.choice.request",
            Self::HumanChoiceResponse(_) => "human.choice.response",
            Self::HumanInputCancelled(_) => "human.input.cancelled",
            Self::PermissionRequest(_) => "permission.request",
            Self::PermissionGrant(_) => "permission.grant",
            Self::PermissionDeny(_) => "permission.deny",
            Self::LeaseGranted(_) => "lease.granted",
            Self::LeaseExtended(_) => "lease.extended",
            Self::LeaseRevoked(_) => "lease.revoked",
            Self::LeaseRefresh(_) => "lease.refresh",
            Self::Subscribe(_) => "subscribe",
            Self::SubscribeAccepted(_) => "subscribe.accepted",
            Self::SubscribeEvent(_) => "subscribe.event",
            Self::Unsubscribe(_) => "unsubscribe",
            Self::SubscribeClosed(_) => "subscribe.closed",
            Self::ArtifactPut(_) => "artifact.put",
            Self::ArtifactFetch(_) => "artifact.fetch",
            Self::ArtifactRef(_) => "artifact.ref",
            Self::ArtifactRelease(_) => "artifact.release",
            Self::EventEmit(_) => "event.emit",
            Self::Log(_) => "log",
            Self::Metric(_) => "metric",
            Self::TraceSpan(_) => "trace.span",
        }
    }

    /// True if this variant is one of the handshake messages allowed before
    /// `session.accepted` (RFC §8.1).
    #[must_use]
    pub const fn is_handshake(&self) -> bool {
        matches!(
            self,
            Self::SessionOpen(_)
                | Self::SessionChallenge(_)
                | Self::SessionAuthenticate(_)
                | Self::SessionAccepted(_)
                | Self::SessionUnauthenticated(_)
                | Self::SessionRejected(_)
        )
    }
}

/// Payload for a `ping` message. Optional opaque nonce echoed back in `pong`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PingPayload {
    /// Optional nonce; if present, the corresponding `pong` echoes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nonce: Option<String>,
}

/// Payload for a `pong` message.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PongPayload {
    /// Echoed nonce, if the `ping` carried one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nonce: Option<String>,
}

/// Payload for an `event.emit` message.
///
/// `event.emit` is a generic carrier; the meaning of each event lives in
/// `name` (e.g. `"subscription.backfill_complete"`) and any additional
/// data is opaque JSON in `data`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventEmitPayload {
    /// Event name (a dotted, namespaced identifier).
    pub name: String,
    /// Opaque event data; schema is determined by `name`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// Severity level for [`LogPayload`] (RFC §17.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    /// `trace`
    Trace,
    /// `debug`
    Debug,
    /// `info`
    Info,
    /// `warn`
    Warn,
    /// `error`
    Error,
    /// `critical`
    Critical,
}

/// Payload for a `log` message (RFC §17.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogPayload {
    /// Severity level.
    pub level: LogLevel,
    /// Free-form human-readable message.
    pub message: String,
    /// Optional structured attributes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attributes: Option<serde_json::Value>,
}

/// Payload for a `metric` message (RFC §17.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricPayload {
    /// Metric name. Standard names are listed in [`standard_names`].
    pub name: String,
    /// Numeric value.
    pub value: f64,
    /// Unit of measure.
    pub unit: String,
    /// Optional dimensions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dims: Option<serde_json::Value>,
}

/// Reserved standard metric names (RFC §17.3.1).
pub mod standard_names {
    /// `tokens.used`
    pub const TOKENS_USED: &str = "tokens.used";
    /// `cost.usd`
    pub const COST_USD: &str = "cost.usd";
    /// `gpu.seconds`
    pub const GPU_SECONDS: &str = "gpu.seconds";
    /// `tool.invocations`
    pub const TOOL_INVOCATIONS: &str = "tool.invocations";
    /// `latency.ms`
    pub const LATENCY_MS: &str = "latency.ms";
    /// `bytes.in`
    pub const BYTES_IN: &str = "bytes.in";
    /// `bytes.out`
    pub const BYTES_OUT: &str = "bytes.out";
    /// `errors.total`
    pub const ERRORS_TOTAL: &str = "errors.total";
}

/// Synthetic event name emitted at the boundary between subscription
/// backfill and live tail (RFC §13.3).
pub const SUBSCRIPTION_BACKFILL_COMPLETE: &str = "subscription.backfill_complete";

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_panics_doc
)]
mod tests {
    use super::*;

    #[test]
    fn ping_round_trips_through_serde() {
        let m = MessageType::Ping(PingPayload {
            nonce: Some("abc".into()),
        });
        let json = serde_json::to_string(&m).expect("serialize");
        let back: MessageType = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(m, back);
    }

    #[test]
    fn ping_wire_shape_matches_rfc() {
        let m = MessageType::Ping(PingPayload { nonce: None });
        let json = serde_json::to_value(&m).expect("serialize");
        assert_eq!(json, serde_json::json!({"type": "ping", "payload": {}}));
    }

    #[test]
    fn metric_with_standard_name_round_trips() {
        let m = MessageType::Metric(MetricPayload {
            name: standard_names::TOKENS_USED.into(),
            value: 1432.0,
            unit: "tokens".into(),
            dims: Some(serde_json::json!({"model": "claude-3.5", "kind": "input"})),
        });
        let json = serde_json::to_string(&m).expect("serialize");
        let back: MessageType = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(m, back);
    }

    #[test]
    fn type_name_matches_wire_discriminator() {
        let cases = [
            (MessageType::Ping(PingPayload::default()), "ping"),
            (MessageType::Pong(PongPayload::default()), "pong"),
            (
                MessageType::EventEmit(EventEmitPayload {
                    name: "x".into(),
                    data: None,
                }),
                "event.emit",
            ),
        ];
        for (m, expected) in cases {
            assert_eq!(m.type_name(), expected);
        }
    }

    #[test]
    fn capabilities_default_is_empty() {
        let c = Capabilities::default();
        assert!(!c.has(CapabilityName::Streaming));
        assert!(!c.has(CapabilityName::Anonymous));
    }

    #[test]
    fn capabilities_round_trip_with_extra_fields() {
        let json = serde_json::json!({
            "streaming": true,
            "human_input": true,
            "extensions": ["arcpx.example.v1"],
            "totally_made_up": true,
        });
        let c: Capabilities = serde_json::from_value(json).expect("deserialize");
        assert!(c.has(CapabilityName::Streaming));
        assert!(c.has(CapabilityName::HumanInput));
        assert_eq!(c.extensions, vec!["arcpx.example.v1"]);
        assert!(c.extra.contains_key("totally_made_up"));
    }

    #[test]
    fn unknown_type_fails_deserialize() {
        let bad = "{\"type\":\"never.heard.of.it\",\"payload\":{}}";
        let result: Result<MessageType, _> = serde_json::from_str(bad);
        assert!(result.is_err());
    }

    #[test]
    fn handshake_messages_classified_correctly() {
        assert!(MessageType::SessionOpen(SessionOpenPayload {
            auth: Credentials {
                scheme: AuthScheme::None,
                token: None
            },
            client: ClientIdentity {
                kind: "test".into(),
                version: "0".into(),
                fingerprint: None,
                principal: None
            },
            capabilities: Capabilities::default(),
        })
        .is_handshake());
        assert!(!MessageType::Ping(PingPayload::default()).is_handshake());
    }
}
