//! Persistent metadata shapes for outstanding provisioned credentials.

use chrono::{DateTime, Utc};

use crate::ids::JobId;
use crate::runtime::CredentialId;

/// One outstanding credential ledger row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutstandingCredential {
    /// Credential id.
    pub credential_id: CredentialId,
    /// Job that owns the credential.
    pub job_id: JobId,
    /// Issue timestamp.
    pub issued_at: DateTime<Utc>,
    /// Revocation timestamp, if completed.
    pub revoked_at: Option<DateTime<Utc>>,
}
