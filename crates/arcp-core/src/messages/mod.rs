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
pub mod result_chunk;
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
    JobHeartbeatPayload, JobProgressPayload, JobSchedulePayload, JobStartedPayload, JobState,
    ToolErrorPayload, ToolInvokePayload, ToolResultPayload, WorkflowCompletePayload,
    WorkflowStartPayload,
};
pub use permissions::{
    CostBudget, CostBudgetAmount, CostBudgetParseError, LeaseExtendedPayload, LeaseGrantedPayload,
    LeaseRefreshPayload, LeaseRequest, LeaseRevokedPayload, LeaseSubsetViolation, ModelUse,
    PermissionDenyPayload, PermissionGrantPayload, PermissionRequestPayload, TrustLevel,
};
pub use result_chunk::{
    JobResultChunkPayload, ResultChunkAssembler, ResultChunkEncoding, ResultChunkError,
};
pub use session::{
    AuthScheme, ClientIdentity, Credentials, JobListEntry, RuntimeIdentity, SessionAcceptedPayload,
    SessionAckPayload, SessionAuthenticatePayload, SessionChallengePayload, SessionClosePayload,
    SessionClosedPayload, SessionEvictedPayload, SessionJobsPayload, SessionLease,
    SessionListJobsFilter, SessionListJobsPayload, SessionOpenPayload, SessionPingPayload,
    SessionPongPayload, SessionRefreshPayload, SessionRejectedPayload, SessionResumePayload,
    SessionResumedPayload, SessionUnauthenticatedPayload,
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
    /// `session.closed` — runtime ack of a graceful close (ARCP v1.1 §6.7).
    #[serde(rename = "session.closed")]
    SessionClosed(SessionClosedPayload),
    /// `session.resume` — client reconnect with resume token (ARCP v1.1 §6.3).
    #[serde(rename = "session.resume")]
    SessionResume(SessionResumePayload),
    /// `session.resumed` — runtime ack of a successful resume (ARCP v1.1 §6.3).
    #[serde(rename = "session.resumed")]
    SessionResumed(SessionResumedPayload),
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
            Self::SessionClosed(_) => "session.closed",
            Self::SessionResume(_) => "session.resume",
            Self::SessionResumed(_) => "session.resumed",
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
                // §6.3 resume is a reconnect handshake: a fresh connection
                // sends session.resume in place of session.open.
                | Self::SessionResume(_)
                | Self::SessionResumed(_)
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
                | Self::SessionClosed(_)
                | Self::SessionResume(_)
                | Self::SessionResumed(_)
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
mod tests;
