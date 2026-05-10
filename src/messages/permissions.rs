//! Permission challenge and lease lifecycle (RFC §15).

use serde::{Deserialize, Serialize};

use crate::ids::LeaseId;

/// Trust level (RFC §15.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrustLevel {
    /// External / public.
    Untrusted,
    /// Limited access.
    Constrained,
    /// Internal.
    Trusted,
    /// System-level.
    Privileged,
}

/// Payload for `permission.request` (RFC §15.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionRequestPayload {
    /// Permission name (e.g. `payment.refund.create`).
    pub permission: String,
    /// Resource identifier.
    pub resource: String,
    /// Operation identifier.
    pub operation: String,
    /// Operator-facing reason.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Requested lease duration in seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requested_lease_seconds: Option<u64>,
}

/// Payload for `permission.grant`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionGrantPayload {
    /// Granted permission.
    pub permission: String,
    /// Resource identifier.
    pub resource: String,
    /// Operation identifier.
    pub operation: String,
    /// Lease duration in seconds.
    pub lease_seconds: u64,
}

/// Payload for `permission.deny`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionDenyPayload {
    /// Denied permission.
    pub permission: String,
    /// Free-form reason.
    pub reason: String,
}

/// Payload for `lease.granted` (RFC §15.5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaseGrantedPayload {
    /// Newly minted lease id.
    pub lease_id: LeaseId,
    /// Permission the lease covers.
    pub permission: String,
    /// Resource the lease covers.
    pub resource: String,
    /// Operation the lease covers.
    pub operation: String,
    /// Absolute expiry time.
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

/// Payload for `lease.refresh` — holder asks for an extension.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaseRefreshPayload {
    /// The lease being refreshed.
    pub lease_id: LeaseId,
    /// Requested additional duration in seconds.
    pub additional_seconds: u64,
}

/// Payload for `lease.extended`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaseExtendedPayload {
    /// The lease that was extended.
    pub lease_id: LeaseId,
    /// New absolute expiry time.
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

/// Payload for `lease.revoked`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaseRevokedPayload {
    /// The revoked lease.
    pub lease_id: LeaseId,
    /// Free-form reason for revocation.
    pub reason: String,
}
