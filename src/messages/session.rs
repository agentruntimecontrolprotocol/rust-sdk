//! Session lifecycle messages (RFC §8).

use serde::{Deserialize, Serialize};

use crate::error::ErrorCode;
use crate::ids::SessionId;
use crate::messages::Capabilities;

/// `auth.scheme` discriminator (RFC §8.2).
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

/// Client identity attestation block (RFC §8.2).
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

/// Runtime identity block emitted in `session.accepted` (RFC §8.3).
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

/// Payload for `session.open` (RFC §8.1 step 1).
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

/// Payload for `session.challenge` (RFC §8.1 step 2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionChallengePayload {
    /// Free-form challenge nonce / instructions.
    pub challenge: String,
}

/// Payload for `session.authenticate` (RFC §8.1 step 3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionAuthenticatePayload {
    /// Response to the challenge.
    pub response: String,
}

/// Payload for `session.accepted` (RFC §8.1 step 4).
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
    /// Reason code (drawn from §18 taxonomy).
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
