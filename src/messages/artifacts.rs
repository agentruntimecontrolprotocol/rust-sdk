//! Artifact messages (RFC §16).

use serde::{Deserialize, Serialize};

use crate::ids::ArtifactId;

/// Canonical artifact reference (RFC §16.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactRef {
    /// Artifact id.
    pub artifact_id: ArtifactId,
    /// `arcp://` URI for the artifact.
    pub uri: String,
    /// Media type.
    pub media_type: String,
    /// Size in bytes (decoded).
    pub size: u64,
    /// Optional integrity hash.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    /// Optional retention deadline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Payload for `artifact.put`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactPutPayload {
    /// Media type.
    pub media_type: String,
    /// Inline base64 body.
    pub data: String,
    /// Optional pre-computed integrity hash.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    /// Optional retention duration in seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retain_seconds: Option<u64>,
}

/// Payload for `artifact.fetch`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactFetchPayload {
    /// Artifact to fetch.
    pub artifact_id: ArtifactId,
}

/// Payload for `artifact.ref`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactRefPayload {
    /// Reference body.
    pub artifact: ArtifactRef,
}

/// Payload for `artifact.release`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactReleasePayload {
    /// Artifact to release.
    pub artifact_id: ArtifactId,
}
