//! Wire-level message payload types.
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
//! other metadata fields, matching the canonical wire format from ARCP v1.1
//! §5. The individual payload modules carry their own §-references.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub mod artifacts;
pub mod control;
pub mod credentials;
pub mod execution;
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
pub use credentials::{CredentialId, CredentialScheme, ProvisionedCredential};
pub use execution::{
    AgentDelegatePayload, AgentHandoffPayload, AgentRef, AgentRefParseError, JobAcceptedPayload,
    JobCancelledPayload, JobCheckpointPayload, JobCompletedPayload, JobFailedPayload,
    JobHeartbeatPayload, JobProgressPayload, JobResultChunkPayload, JobSchedulePayload,
    JobStartedPayload, JobState, ResultChunkAssembler, ResultChunkEncoding, ResultChunkError,
    ToolErrorPayload, ToolInvokePayload, ToolResultPayload, WorkflowCompletePayload,
    WorkflowStartPayload,
};
pub use permissions::{
    CostBudget, CostBudgetAmount, CostBudgetParseError, LeaseExtendedPayload, LeaseGrantedPayload,
    LeaseRefreshPayload, LeaseRequest, LeaseRevokedPayload, LeaseSubsetViolation, ModelUse,
    PermissionDenyPayload, PermissionGrantPayload, PermissionRequestPayload, TrustLevel,
};
pub use session::{
    AuthScheme, ClientIdentity, Credentials, JobListEntry, RuntimeIdentity, SessionAcceptedPayload,
    SessionAckPayload, SessionAuthenticatePayload, SessionChallengePayload, SessionClosePayload,
    SessionEvictedPayload, SessionJobsPayload, SessionLease, SessionListJobsFilter,
    SessionListJobsPayload, SessionOpenPayload, SessionPingPayload, SessionPongPayload,
    SessionRefreshPayload, SessionRejectedPayload, SessionUnauthenticatedPayload,
};
pub use streaming::{
    StreamChunkPayload, StreamClosePayload, StreamErrorPayload, StreamKind, StreamOpenPayload,
};
pub use subscriptions::{
    JobSubscribePayload, JobSubscribedPayload, JobUnsubscribePayload, SubscribeAcceptedPayload,
    SubscribeClosedPayload, SubscribeEventPayload, SubscribePayload, SubscriptionFilter,
    SubscriptionSince, UnsubscribePayload,
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
    /// Per ARCP v1.1 §9.7.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_use: Option<bool>,
    /// Per ARCP v1.1 §9.8.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provisioned_credentials: Option<bool>,
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
    /// Per ARCP v1.1 §7.5 — runtime-side advertisement of available
    /// agents. The wire shape supports both a v1.0-compatible flat list
    /// of names and the v1.1 rich form with versions and a `default`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agents: Option<AgentInventory>,
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

/// One entry in the rich v1.1 form of `capabilities.agents`
/// (ARCP v1.1 §7.5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentInventoryEntry {
    /// Agent name (matches the §7.5 `name` grammar).
    pub name: String,
    /// Available versions for this agent. May be empty if the runtime
    /// advertises the agent without enumerating versions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub versions: Vec<String>,
    /// Version a bare-name reference resolves to. `None` means the
    /// runtime MAY pick any registered version (§7.5).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
}

/// Runtime-side agent inventory advertisement in [`Capabilities`]
/// (ARCP v1.1 §7.5).
///
/// Serializes as either a v1.0-compatible flat array of bare names or
/// the v1.1 rich array of [`AgentInventoryEntry`]. Both forms are
/// accepted on deserialize.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AgentInventory {
    /// v1.0-compatible flat list of bare agent names.
    Flat(Vec<String>),
    /// v1.1 rich list with versions and defaults.
    Rich(Vec<AgentInventoryEntry>),
}

impl AgentInventory {
    /// Normalise into the rich v1.1 shape; flat entries become rich
    /// entries with empty `versions` and no `default`.
    #[must_use]
    pub fn into_rich(self) -> Vec<AgentInventoryEntry> {
        match self {
            Self::Flat(names) => names
                .into_iter()
                .map(|name| AgentInventoryEntry {
                    name,
                    versions: vec![],
                    default: None,
                })
                .collect(),
            Self::Rich(entries) => entries,
        }
    }

    /// True if `agent` is satisfied by this inventory per §7.5
    /// resolution rules. Bare names match any inventory entry with that
    /// name; pinned `name@version` must appear in that entry's
    /// `versions` list. Flat (v1.0) entries match any version since the
    /// runtime does not enumerate them.
    #[must_use]
    pub fn satisfies(&self, agent: &execution::AgentRef) -> bool {
        match self {
            Self::Flat(names) => names.iter().any(|n| n == &agent.name),
            Self::Rich(entries) => entries.iter().any(|e| {
                e.name == agent.name
                    && agent
                        .version
                        .as_ref()
                        .is_none_or(|v| e.versions.iter().any(|known| known == v))
            }),
        }
    }
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
            CapabilityName::ModelUse => matches!(self.model_use, Some(true)),
            CapabilityName::ProvisionedCredentials => {
                matches!(self.provisioned_credentials, Some(true))
            }
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
    /// `model_use`
    ModelUse,
    /// `provisioned_credentials`
    ProvisionedCredentials,
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
    /// `session.ping` (ARCP v1.1 §6.4) — session-scoped heartbeat.
    ///
    /// Canonical heartbeat per ARCP v1.1; the generic `Ping`/`Pong`
    /// variants below were introduced under v1.0's draft scaffolding and
    /// remain only for backwards compatibility.
    #[serde(rename = "session.ping")]
    SessionPing(SessionPingPayload),
    /// `session.pong` (ARCP v1.1 §6.4) — response to `session.ping`.
    #[serde(rename = "session.pong")]
    SessionPong(SessionPongPayload),
    /// `session.ack` (ARCP v1.1 §6.5) — client-side flow-control ack.
    #[serde(rename = "session.ack")]
    SessionAck(SessionAckPayload),
    /// `session.list_jobs` (ARCP v1.1 §6.6) — read-only job inventory
    /// request.
    #[serde(rename = "session.list_jobs")]
    SessionListJobs(SessionListJobsPayload),
    /// `session.jobs` (ARCP v1.1 §6.6) — response to `session.list_jobs`.
    #[serde(rename = "session.jobs")]
    SessionJobs(SessionJobsPayload),

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
    /// `job.result_chunk` (ARCP v1.1 §8.4) — one fragment of a streamed
    /// final result. Terminated by `job.completed` carrying the same
    /// `result_id`.
    #[serde(rename = "job.result_chunk")]
    JobResultChunk(JobResultChunkPayload),

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
    /// `job.subscribe` (ARCP v1.1 §7.6) — cross-session attach to a
    /// running job.
    #[serde(rename = "job.subscribe")]
    JobSubscribe(JobSubscribePayload),
    /// `job.subscribed` (ARCP v1.1 §7.6) — runtime acknowledgement of a
    /// `job.subscribe` request.
    #[serde(rename = "job.subscribed")]
    JobSubscribed(JobSubscribedPayload),
    /// `job.unsubscribe` (ARCP v1.1 §7.6) — terminate a previously
    /// acknowledged job subscription.
    #[serde(rename = "job.unsubscribe")]
    JobUnsubscribe(JobUnsubscribePayload),

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
            Self::SessionPing(_) => "session.ping",
            Self::SessionPong(_) => "session.pong",
            Self::SessionAck(_) => "session.ack",
            Self::SessionListJobs(_) => "session.list_jobs",
            Self::SessionJobs(_) => "session.jobs",
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
            Self::JobResultChunk(_) => "job.result_chunk",
            Self::StreamOpen(_) => "stream.open",
            Self::StreamChunk(_) => "stream.chunk",
            Self::StreamClose(_) => "stream.close",
            Self::StreamError(_) => "stream.error",
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
            Self::JobSubscribe(_) => "job.subscribe",
            Self::JobSubscribed(_) => "job.subscribed",
            Self::JobUnsubscribe(_) => "job.unsubscribe",
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

    /// True if this variant participates in `event_seq` and is therefore
    /// subject to `session.ack` flow control (ARCP v1.1 §6.5).
    ///
    /// Session-control envelopes (handshake, heartbeat, ack, close, evict,
    /// refresh) are NOT counted. Everything else — job events, tool
    /// results, stream chunks, telemetry, artifacts, subscriptions — IS.
    #[must_use]
    pub const fn is_countable_event(&self) -> bool {
        !matches!(
            self,
            Self::SessionOpen(_)
                | Self::SessionChallenge(_)
                | Self::SessionAuthenticate(_)
                | Self::SessionAccepted(_)
                | Self::SessionUnauthenticated(_)
                | Self::SessionRejected(_)
                | Self::SessionRefresh(_)
                | Self::SessionEvicted(_)
                | Self::SessionClose(_)
                | Self::SessionPing(_)
                | Self::SessionPong(_)
                | Self::SessionAck(_)
                | Self::SessionListJobs(_)
                | Self::SessionJobs(_)
                | Self::Ping(_)
                | Self::Pong(_)
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
    fn session_ping_round_trips_through_serde() {
        let now = chrono::Utc::now();
        let m = MessageType::SessionPing(SessionPingPayload {
            nonce: "p_01J".into(),
            sent_at: now,
        });
        let json = serde_json::to_string(&m).expect("serialize");
        let back: MessageType = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(m, back);
    }

    #[test]
    fn session_ack_round_trips_through_serde() {
        let m = MessageType::SessionAck(SessionAckPayload {
            last_processed_seq: 1827,
        });
        let json = serde_json::to_string(&m).expect("serialize");
        let back: MessageType = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(m, back);
        let v: serde_json::Value = serde_json::from_str(&json).expect("value");
        assert_eq!(
            v,
            serde_json::json!({
                "type": "session.ack",
                "payload": { "last_processed_seq": 1827 },
            })
        );
    }

    #[test]
    fn session_list_jobs_round_trips_through_serde() {
        let m = MessageType::SessionListJobs(SessionListJobsPayload {
            filter: Some(SessionListJobsFilter {
                status: vec!["running".into()],
                agent: Some("echo".into()),
                created_after: None,
                created_before: None,
            }),
            limit: Some(50),
            cursor: None,
        });
        let json = serde_json::to_string(&m).expect("serialize");
        let back: MessageType = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(m, back);
    }

    #[test]
    fn session_jobs_round_trips_through_serde() {
        let now = chrono::Utc::now();
        let m = MessageType::SessionJobs(SessionJobsPayload {
            request_id: "msg_x".into(),
            jobs: vec![JobListEntry {
                job_id: crate::ids::JobId::new(),
                agent: "echo@1.0.0".into(),
                status: "running".into(),
                parent_job_id: None,
                created_at: now,
                trace_id: None,
                last_event_seq: 0,
            }],
            next_cursor: None,
        });
        let json = serde_json::to_string(&m).expect("serialize");
        let back: MessageType = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(m, back);
    }

    #[test]
    fn countable_event_classification_excludes_session_control() {
        let now = chrono::Utc::now();
        assert!(!MessageType::SessionPing(SessionPingPayload {
            nonce: "n".into(),
            sent_at: now,
        })
        .is_countable_event());
        assert!(!MessageType::SessionAck(SessionAckPayload {
            last_processed_seq: 0,
        })
        .is_countable_event());
        assert!(!MessageType::Ping(PingPayload::default()).is_countable_event());
        assert!(
            MessageType::JobAccepted(crate::messages::JobAcceptedPayload {
                job_id: crate::ids::JobId::new(),
                credentials: vec![],
                lease: None,
            })
            .is_countable_event()
        );
    }

    #[test]
    fn session_pong_round_trips_through_serde() {
        let now = chrono::Utc::now();
        let m = MessageType::SessionPong(SessionPongPayload {
            ping_nonce: "p_01J".into(),
            received_at: now,
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
    fn capabilities_with_flat_agents_round_trips() {
        // v1.0-compatible shape: agents as a flat array of names.
        let json = serde_json::json!({
            "agents": ["code-refactor", "web-research"],
        });
        let c: Capabilities = serde_json::from_value(json.clone()).expect("deserialize");
        match c.agents.as_ref().expect("agents present") {
            AgentInventory::Flat(names) => {
                assert_eq!(
                    names,
                    &vec!["code-refactor".to_owned(), "web-research".into()]
                );
            }
            AgentInventory::Rich(_) => panic!("expected flat shape"),
        }
        let re = serde_json::to_value(&c).expect("serialize");
        assert_eq!(re["agents"], json["agents"]);
    }

    #[test]
    fn capabilities_with_rich_agents_round_trips() {
        // v1.1 rich shape: agents as a list of {name, versions, default}.
        let json = serde_json::json!({
            "agents": [
                { "name": "code-refactor", "versions": ["1.0.0", "2.0.0"], "default": "2.0.0" },
                { "name": "indexer", "versions": ["0.9.0"] },
            ],
        });
        let c: Capabilities = serde_json::from_value(json).expect("deserialize");
        let inv = c.agents.as_ref().expect("agents present");
        match inv {
            AgentInventory::Rich(entries) => {
                assert_eq!(entries.len(), 2);
                assert_eq!(entries[0].name, "code-refactor");
                assert_eq!(
                    entries[0].versions,
                    vec!["1.0.0".to_owned(), "2.0.0".into()]
                );
                assert_eq!(entries[0].default.as_deref(), Some("2.0.0"));
            }
            AgentInventory::Flat(_) => panic!("expected rich shape"),
        }
    }

    #[test]
    fn agent_inventory_satisfies_resolution_rules() {
        let flat = AgentInventory::Flat(vec!["echo".into()]);
        // Flat (v1.0) satisfies any version (runtime didn't enumerate).
        assert!(flat.satisfies(&crate::messages::AgentRef::parse("echo").expect("parse")));
        assert!(flat.satisfies(&crate::messages::AgentRef::parse("echo@1.0.0").expect("parse")));

        let rich = AgentInventory::Rich(vec![AgentInventoryEntry {
            name: "echo".into(),
            versions: vec!["1.0.0".into(), "2.0.0".into()],
            default: Some("2.0.0".into()),
        }]);
        assert!(rich.satisfies(&crate::messages::AgentRef::parse("echo").expect("parse")));
        assert!(rich.satisfies(&crate::messages::AgentRef::parse("echo@1.0.0").expect("parse")));
        assert!(!rich.satisfies(&crate::messages::AgentRef::parse("echo@9.9.9").expect("parse")));
        assert!(!rich.satisfies(&crate::messages::AgentRef::parse("other").expect("parse")));
    }

    #[test]
    fn capabilities_round_trip_with_extra_fields() {
        let json = serde_json::json!({
            "streaming": true,
            "extensions": ["arcpx.example.v1"],
            "totally_made_up": true,
        });
        let c: Capabilities = serde_json::from_value(json).expect("deserialize");
        assert!(c.has(CapabilityName::Streaming));
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

    #[test]
    #[allow(clippy::too_many_lines)]
    fn type_name_covers_every_variant() {
        // One instance per MessageType variant. Any future variant added
        // to MessageType but not to this list will fall through to the
        // exhaustive match in MessageType::type_name and the test will
        // surface the omission as a 0%-coverage spike on the new arm.
        let now = chrono::Utc::now();
        let cases: Vec<(MessageType, &'static str)> = vec![
            (
                MessageType::SessionOpen(SessionOpenPayload {
                    auth: Credentials {
                        scheme: AuthScheme::None,
                        token: None,
                    },
                    client: ClientIdentity {
                        kind: "t".into(),
                        version: "0".into(),
                        fingerprint: None,
                        principal: None,
                    },
                    capabilities: Capabilities::default(),
                }),
                "session.open",
            ),
            (
                MessageType::SessionChallenge(SessionChallengePayload {
                    challenge: "x".into(),
                }),
                "session.challenge",
            ),
            (
                MessageType::SessionAuthenticate(SessionAuthenticatePayload {
                    response: "x".into(),
                }),
                "session.authenticate",
            ),
            (
                MessageType::SessionAccepted(SessionAcceptedPayload {
                    session_id: crate::ids::SessionId::new(),
                    runtime: crate::messages::RuntimeIdentity {
                        kind: "rt".into(),
                        version: "0".into(),
                        fingerprint: None,
                        trust_level: None,
                    },
                    capabilities: Capabilities::default(),
                    lease: None,
                }),
                "session.accepted",
            ),
            (
                MessageType::SessionUnauthenticated(SessionUnauthenticatedPayload {
                    code: crate::error::ErrorCode::Unauthenticated,
                    message: "x".into(),
                }),
                "session.unauthenticated",
            ),
            (
                MessageType::SessionRejected(SessionRejectedPayload {
                    code: crate::error::ErrorCode::Unauthenticated,
                    message: "x".into(),
                }),
                "session.rejected",
            ),
            (
                MessageType::SessionRefresh(SessionRefreshPayload {
                    deadline: now,
                    challenge: None,
                }),
                "session.refresh",
            ),
            (
                MessageType::SessionEvicted(SessionEvictedPayload {
                    code: crate::error::ErrorCode::Cancelled,
                    reason: "x".into(),
                }),
                "session.evicted",
            ),
            (
                MessageType::SessionClose(SessionClosePayload::default()),
                "session.close",
            ),
            (
                MessageType::SessionPing(SessionPingPayload {
                    nonce: "n".into(),
                    sent_at: now,
                }),
                "session.ping",
            ),
            (
                MessageType::SessionPong(SessionPongPayload {
                    ping_nonce: "n".into(),
                    received_at: now,
                }),
                "session.pong",
            ),
            (
                MessageType::SessionAck(SessionAckPayload {
                    last_processed_seq: 0,
                }),
                "session.ack",
            ),
            (
                MessageType::SessionListJobs(SessionListJobsPayload::default()),
                "session.list_jobs",
            ),
            (
                MessageType::SessionJobs(SessionJobsPayload {
                    request_id: "r".into(),
                    jobs: vec![],
                    next_cursor: None,
                }),
                "session.jobs",
            ),
            (MessageType::Ping(PingPayload::default()), "ping"),
            (MessageType::Pong(PongPayload::default()), "pong"),
            (MessageType::Ack(AckPayload { note: None }), "ack"),
            (
                MessageType::Nack(NackPayload {
                    code: crate::error::ErrorCode::Unknown,
                    message: "x".into(),
                    details: None,
                }),
                "nack",
            ),
            (
                MessageType::Cancel(CancelPayload {
                    target: CancelTargetKind::Job,
                    target_id: "x".into(),
                    reason: None,
                    deadline_ms: None,
                }),
                "cancel",
            ),
            (
                MessageType::CancelAccepted(CancelAcceptedPayload { target_id: None }),
                "cancel.accepted",
            ),
            (
                MessageType::CancelRefused(CancelRefusedPayload {
                    target_id: "x".into(),
                    reason: "x".into(),
                }),
                "cancel.refused",
            ),
            (
                MessageType::Interrupt(InterruptPayload {
                    target: CancelTargetKind::Job,
                    target_id: "x".into(),
                    prompt: "x".into(),
                }),
                "interrupt",
            ),
            (MessageType::Resume(ResumePayload::default()), "resume"),
            (
                MessageType::Backpressure(BackpressurePayload {
                    desired_rate_per_second: None,
                    buffer_remaining_bytes: None,
                    reason: None,
                }),
                "backpressure",
            ),
            (
                MessageType::ToolInvoke(ToolInvokePayload::new("x", serde_json::json!({}))),
                "tool.invoke",
            ),
            (
                MessageType::ToolResult(ToolResultPayload {
                    value: None,
                    result_ref: None,
                }),
                "tool.result",
            ),
            (
                MessageType::ToolError(ToolErrorPayload {
                    code: crate::error::ErrorCode::Internal,
                    retryable: None,
                    message: "x".into(),
                    details: None,
                }),
                "tool.error",
            ),
            (
                MessageType::JobAccepted(JobAcceptedPayload {
                    job_id: crate::ids::JobId::new(),
                    credentials: vec![],
                    lease: None,
                }),
                "job.accepted",
            ),
            (
                MessageType::JobStarted(JobStartedPayload { description: None }),
                "job.started",
            ),
            (
                MessageType::JobProgress(JobProgressPayload {
                    current: 0.0,
                    total: None,
                    units: None,
                    message: None,
                }),
                "job.progress",
            ),
            (
                MessageType::JobHeartbeat(JobHeartbeatPayload {
                    sequence: 1,
                    deadline_ms: None,
                    state: JobState::Running,
                }),
                "job.heartbeat",
            ),
            (
                MessageType::JobCompleted(JobCompletedPayload {
                    value: None,
                    result_ref: None,
                    result_id: None,
                    result_size: None,
                    summary: None,
                }),
                "job.completed",
            ),
            (
                MessageType::JobResultChunk(JobResultChunkPayload {
                    result_id: "r".into(),
                    chunk_seq: 0,
                    data: "x".into(),
                    encoding: crate::messages::ResultChunkEncoding::Utf8,
                    more: false,
                }),
                "job.result_chunk",
            ),
            (
                MessageType::JobFailed(JobFailedPayload {
                    code: crate::error::ErrorCode::Internal,
                    retryable: None,
                    message: "x".into(),
                    details: None,
                }),
                "job.failed",
            ),
            (
                MessageType::JobCancelled(JobCancelledPayload { reason: None }),
                "job.cancelled",
            ),
            (
                MessageType::StreamOpen(StreamOpenPayload {
                    kind: StreamKind::Text,
                    content_type: None,
                    encoding: None,
                }),
                "stream.open",
            ),
            (
                MessageType::StreamChunk(StreamChunkPayload {
                    sequence: 0,
                    data: serde_json::json!(""),
                    content_type: None,
                    sha256: None,
                    redacted: false,
                    role: None,
                }),
                "stream.chunk",
            ),
            (
                MessageType::StreamClose(StreamClosePayload::default()),
                "stream.close",
            ),
            (
                MessageType::StreamError(StreamErrorPayload {
                    code: crate::error::ErrorCode::Internal,
                    message: "x".into(),
                }),
                "stream.error",
            ),
            (
                MessageType::PermissionRequest(PermissionRequestPayload {
                    permission: "p".into(),
                    resource: "r".into(),
                    operation: "o".into(),
                    reason: None,
                    requested_lease_seconds: None,
                }),
                "permission.request",
            ),
            (
                MessageType::PermissionGrant(PermissionGrantPayload {
                    permission: "p".into(),
                    resource: "r".into(),
                    operation: "o".into(),
                    lease_seconds: 1,
                }),
                "permission.grant",
            ),
            (
                MessageType::PermissionDeny(PermissionDenyPayload {
                    permission: "p".into(),
                    reason: "x".into(),
                }),
                "permission.deny",
            ),
            (
                MessageType::LeaseGranted(LeaseGrantedPayload {
                    lease_id: crate::ids::LeaseId::new(),
                    permission: "p".into(),
                    resource: "r".into(),
                    operation: "o".into(),
                    expires_at: now,
                }),
                "lease.granted",
            ),
            (
                MessageType::LeaseExtended(LeaseExtendedPayload {
                    lease_id: crate::ids::LeaseId::new(),
                    expires_at: now,
                }),
                "lease.extended",
            ),
            (
                MessageType::LeaseRevoked(LeaseRevokedPayload {
                    lease_id: crate::ids::LeaseId::new(),
                    reason: "x".into(),
                }),
                "lease.revoked",
            ),
            (
                MessageType::LeaseRefresh(LeaseRefreshPayload {
                    lease_id: crate::ids::LeaseId::new(),
                    additional_seconds: 1,
                }),
                "lease.refresh",
            ),
            (
                MessageType::Subscribe(SubscribePayload::default()),
                "subscribe",
            ),
            (
                MessageType::SubscribeAccepted(SubscribeAcceptedPayload {
                    subscription_id: crate::ids::SubscriptionId::new(),
                }),
                "subscribe.accepted",
            ),
            (
                MessageType::SubscribeEvent(SubscribeEventPayload {
                    event: serde_json::json!({}),
                }),
                "subscribe.event",
            ),
            (
                MessageType::Unsubscribe(UnsubscribePayload {
                    subscription_id: crate::ids::SubscriptionId::new(),
                }),
                "unsubscribe",
            ),
            (
                MessageType::SubscribeClosed(SubscribeClosedPayload {
                    subscription_id: crate::ids::SubscriptionId::new(),
                    code: crate::error::ErrorCode::Cancelled,
                    reason: "x".into(),
                }),
                "subscribe.closed",
            ),
            (
                MessageType::JobSubscribe(JobSubscribePayload {
                    job_id: crate::ids::JobId::new(),
                    from_event_seq: None,
                    history: false,
                }),
                "job.subscribe",
            ),
            (
                MessageType::JobSubscribed(JobSubscribedPayload {
                    job_id: crate::ids::JobId::new(),
                    current_status: "running".into(),
                    agent: "echo".into(),
                    parent_job_id: None,
                    trace_id: None,
                    subscribed_from: 0,
                    replayed: false,
                }),
                "job.subscribed",
            ),
            (
                MessageType::JobUnsubscribe(JobUnsubscribePayload {
                    job_id: crate::ids::JobId::new(),
                }),
                "job.unsubscribe",
            ),
            (
                MessageType::ArtifactPut(ArtifactPutPayload {
                    media_type: "x".into(),
                    data: String::new(),
                    sha256: None,
                    retain_seconds: None,
                }),
                "artifact.put",
            ),
            (
                MessageType::ArtifactFetch(ArtifactFetchPayload {
                    artifact_id: crate::ids::ArtifactId::new(),
                }),
                "artifact.fetch",
            ),
            (
                MessageType::ArtifactRef(ArtifactRefPayload {
                    artifact: ArtifactRef {
                        artifact_id: crate::ids::ArtifactId::new(),
                        uri: "arcp://x".into(),
                        media_type: "x".into(),
                        size: 0,
                        sha256: None,
                        expires_at: None,
                    },
                }),
                "artifact.ref",
            ),
            (
                MessageType::ArtifactRelease(ArtifactReleasePayload {
                    artifact_id: crate::ids::ArtifactId::new(),
                }),
                "artifact.release",
            ),
            (
                MessageType::EventEmit(EventEmitPayload {
                    name: "x".into(),
                    data: None,
                }),
                "event.emit",
            ),
            (
                MessageType::Log(LogPayload {
                    level: LogLevel::Info,
                    message: "x".into(),
                    attributes: None,
                }),
                "log",
            ),
            (
                MessageType::Metric(MetricPayload {
                    name: "x".into(),
                    value: 0.0,
                    unit: "u".into(),
                    dims: None,
                }),
                "metric",
            ),
            (
                MessageType::TraceSpan(TraceSpanPayload {
                    name: "x".into(),
                    trace_id: crate::ids::TraceId::new("t").expect("non-empty"),
                    span_id: crate::ids::SpanId::new("s").expect("non-empty"),
                    parent_span_id: None,
                    start_time: now,
                    end_time: now,
                    attributes: None,
                }),
                "trace.span",
            ),
        ];
        for (msg, expected) in &cases {
            assert_eq!(msg.type_name(), *expected);
        }
        // 62 message variants — sanity-check we built exactly that many.
        // Bump this when MessageType grows.
        assert_eq!(cases.len(), 62);
    }
}
