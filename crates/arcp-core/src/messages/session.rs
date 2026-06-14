//! Session lifecycle messages (ARCP v1.1 §6).
//!
//! ARCP v1.1 §6.2 names the two handshake envelopes `session.hello` and
//! `session.welcome`; the SDK serializes these as [`SessionOpenPayload`]
//! and [`SessionAcceptedPayload`] for historical reasons. The
//! `session.challenge`/`session.authenticate` pair is an SDK extension
//! beyond v1.1, which collapses authentication to a single bearer token
//! carried in the hello envelope (§6.1).

use serde::{Deserialize, Serialize};

use crate::error::ErrorCode;
use crate::ids::SessionId;
use crate::messages::Capabilities;

/// `auth.scheme` discriminator. ARCP v1.1 §6.1 defines only `bearer` as
/// normative; the additional variants are SDK extensions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum AuthScheme {
    /// Opaque token validated against the runtime trust store.
    Bearer,
    /// Signed JWT with `aud` set to the runtime identity.
    SignedJwt,
    /// Anonymous; only valid when `capabilities.anonymous: true` is negotiated.
    None,
    /// Mutual TLS established at the transport (v0.2).
    Mtls,
    /// `OAuth2` access token (v0.2).
    Oauth2,
}

/// Credentials block carried in `session.open`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Credentials {
    /// Auth scheme.
    pub scheme: AuthScheme,
    /// Token payload (omitted for `mtls` and `none`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

/// Client identity attestation block (ARCP v1.1 §6.2 hello payload).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientIdentity {
    /// Implementation kind, e.g. `"example-client"`.
    pub kind: String,
    /// Implementation version.
    pub version: String,
    /// Optional fingerprint (REQUIRED for `mtls`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    /// Optional principal identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub principal: Option<String>,
}

/// Runtime identity block emitted in `session.accepted` (ARCP v1.1 §6.2
/// welcome payload).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeIdentity {
    /// Implementation kind, e.g. `"arcp-rs"`.
    pub kind: String,
    /// Implementation version.
    pub version: String,
    /// Optional fingerprint clients can pin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    /// Trust level, one of `untrusted`/`constrained`/`trusted`/`privileged`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trust_level: Option<String>,
}

/// Session lease information surfaced in `session.accepted`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionLease {
    /// Absolute expiry time.
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

/// Payload for `session.open` (SDK serialization of the ARCP v1.1 §6.2
/// `session.hello` envelope).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionOpenPayload {
    /// Credentials.
    pub auth: Credentials,
    /// Client identity attestation.
    pub client: ClientIdentity,
    /// Proposed capability set.
    #[serde(default)]
    pub capabilities: Capabilities,
}

/// Payload for `session.challenge` (SDK extension; ARCP v1.1 §6.1 does
/// not define a challenge / response auth flow).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionChallengePayload {
    /// Free-form challenge nonce / instructions.
    pub challenge: String,
}

/// Payload for `session.authenticate` (SDK extension; pairs with
/// [`SessionChallengePayload`], not part of ARCP v1.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionAuthenticatePayload {
    /// Response to the challenge.
    pub response: String,
}

/// Payload for `session.accepted` (SDK serialization of the ARCP v1.1
/// §6.2 `session.welcome` envelope).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionAcceptedPayload {
    /// Newly minted session id.
    pub session_id: SessionId,
    /// Runtime identity block.
    pub runtime: RuntimeIdentity,
    /// Negotiated capability set.
    pub capabilities: Capabilities,
    /// Session lease (optional but recommended).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lease: Option<SessionLease>,
    /// Resume token (ARCP v1.1 §6.3). Rotates on every successful welcome;
    /// the client presents the most recent value in `session.resume` to
    /// reconnect after a transport drop.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_token: Option<String>,
}

/// Payload for `session.resume` (ARCP v1.1 §6.3).
///
/// A reconnecting client presents its most recent `resume_token` and the
/// `last_event_seq` it has received; the runtime replays buffered events
/// with `seq > last_event_seq` or returns `RESUME_WINDOW_EXPIRED`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionResumePayload {
    /// Most recent resume token issued to this client.
    pub resume_token: String,
    /// Highest `event_seq` the client has already received.
    pub last_event_seq: u64,
}

/// Payload for `session.resumed` — the runtime's acknowledgement of a
/// successful `session.resume` (ARCP v1.1 §6.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionResumedPayload {
    /// The resumed session id.
    pub session_id: SessionId,
    /// Rotated resume token for the next reconnect.
    pub resume_token: String,
    /// The `last_event_seq` the client presented; replay covers events
    /// with `seq > replayed_from`.
    pub replayed_from: u64,
    /// Whether any buffered events were replayed.
    pub replayed: bool,
}

/// Payload for `session.unauthenticated` — emitted before authentication
/// completes when the runtime requires re-attempt with corrected creds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionUnauthenticatedPayload {
    /// Reason code.
    pub code: ErrorCode,
    /// Human-readable detail.
    pub message: String,
}

/// Payload for `session.rejected` (terminal handshake failure).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRejectedPayload {
    /// Reason code.
    pub code: ErrorCode,
    /// Human-readable detail.
    pub message: String,
}

/// Payload for `session.refresh` — runtime asks the client to re-authenticate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRefreshPayload {
    /// Absolute deadline by which a fresh `session.authenticate` is expected.
    pub deadline: chrono::DateTime<chrono::Utc>,
    /// Optional new challenge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub challenge: Option<String>,
}

/// Payload for `session.evicted` — runtime ended the session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionEvictedPayload {
    /// Reason code (drawn from the ARCP v1.1 §12 error taxonomy).
    pub code: ErrorCode,
    /// Free-form reason text.
    pub reason: String,
}

/// Payload for `session.close` — graceful close from either side.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionClosePayload {
    /// Optional human-readable reason.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Payload for `session.closed` (ARCP v1.1 §6.7).
///
/// The runtime's acknowledgement of a graceful `session.close`. In-flight
/// jobs are not affected by the close; they keep running and remain
/// resumable within the resume window.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionClosedPayload {
    /// Optional human-readable reason echoed back to the client.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Payload for `session.ping` (ARCP v1.1 §6.4).
///
/// Either peer MAY emit `session.ping` if idle and expect a prompt
/// `session.pong` echoing the same `nonce`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionPingPayload {
    /// Opaque nonce; the corresponding `session.pong` echoes it as
    /// `ping_nonce`.
    pub nonce: String,
    /// Sender timestamp (RFC 3339).
    pub sent_at: chrono::DateTime<chrono::Utc>,
}

/// Payload for `session.pong` (ARCP v1.1 §6.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionPongPayload {
    /// Echoed `nonce` from the inciting `session.ping`.
    pub ping_nonce: String,
    /// Receiver timestamp (RFC 3339).
    pub received_at: chrono::DateTime<chrono::Utc>,
}

/// Payload for `session.ack` (ARCP v1.1 §6.5).
///
/// The client periodically informs the runtime of its highest processed
/// event sequence; the runtime MAY free buffered events with
/// `seq <= last_processed_seq` and MAY use the lag between the latest
/// emitted seq and `last_processed_seq` to detect slow consumers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionAckPayload {
    /// Highest event sequence the client has processed.
    pub last_processed_seq: u64,
}

/// Optional filter for `session.list_jobs` (ARCP v1.1 §6.6).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionListJobsFilter {
    /// Match jobs whose current status is one of these values.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub status: Vec<String>,
    /// Match jobs whose agent identifier (or `agent@version`) equals
    /// this value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    /// Match jobs created strictly after this instant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_after: Option<chrono::DateTime<chrono::Utc>>,
    /// Match jobs created strictly before this instant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_before: Option<chrono::DateTime<chrono::Utc>>,
}

/// Payload for `session.list_jobs` (ARCP v1.1 §6.6).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionListJobsPayload {
    /// Optional filter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<SessionListJobsFilter>,
    /// Maximum number of entries to return.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// Opaque pagination cursor returned by a previous response's
    /// `next_cursor`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

/// One entry in `session.jobs.payload.jobs` (ARCP v1.1 §6.6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobListEntry {
    /// Job identifier.
    pub job_id: crate::ids::JobId,
    /// Resolved `name@version` (or bare `name` if no version was pinned).
    pub agent: String,
    /// Current status (e.g. `running`, `completed`).
    pub status: String,
    /// Optional parent job id for delegated/child jobs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_job_id: Option<crate::ids::JobId>,
    /// Job submission timestamp.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Optional trace identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    /// Last event sequence emitted for the job in this session.
    pub last_event_seq: u64,
}

/// Payload for `session.jobs` (ARCP v1.1 §6.6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionJobsPayload {
    /// Correlated request id from the matching `session.list_jobs`.
    pub request_id: String,
    /// Job summaries.
    pub jobs: Vec<JobListEntry>,
    /// Opaque continuation cursor; `None` if there are no further pages.
    pub next_cursor: Option<String>,
}
