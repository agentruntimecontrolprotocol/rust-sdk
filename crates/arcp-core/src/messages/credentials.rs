//! Wire types for provisioned credentials (ARCP v1.1 §9.8).
//!
//! Issuance / revocation / ledger machinery is the runtime's job; what lives
//! here are only the types that travel on the wire (`job.accepted`'s
//! `credentials` array, `credential.revoke`, etc.).

use serde::{Deserialize, Serialize};

use crate::messages::permissions::LeaseRequest;

/// Stable id for a runtime-issued provisioned credential.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CredentialId(
    /// Wire string.
    pub String,
);

impl CredentialId {
    /// Mint a credential id with the canonical `cred_` prefix.
    #[must_use]
    pub fn new(sequence: u64) -> Self {
        Self(format!("cred_{sequence:016x}"))
    }

    /// Borrow the wire string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for CredentialId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Provisioned credential authentication scheme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum CredentialScheme {
    /// HTTP bearer token.
    Bearer,
}

/// Wire shape for a provisioned credential (ARCP v1.1 §9.8.1).
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct ProvisionedCredential {
    /// Credential identifier.
    pub id: CredentialId,
    /// Authentication scheme.
    pub scheme: CredentialScheme,
    /// Secret value. This is intentionally redacted from [`Debug`].
    pub value: String,
    /// Upstream endpoint where the credential is valid.
    pub endpoint: String,
    /// Optional provider profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    /// Lease constraints baked into the upstream credential.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constraints: Option<LeaseRequest>,
}

impl std::fmt::Debug for ProvisionedCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProvisionedCredential")
            .field("id", &self.id)
            .field("scheme", &self.scheme)
            .field("value", &"<redacted>")
            .field("endpoint", &self.endpoint)
            .field("profile", &self.profile)
            .field("constraints", &self.constraints)
            .finish()
    }
}
