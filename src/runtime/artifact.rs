//! Artifact store (RFC §16).
//!
//! Phase 5 ships an in-memory implementation with inline base64 only.
//! Retention sweep and SQLite-blob persistence are deferred.

use std::sync::Arc;
use std::time::{Duration, SystemTime};

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use dashmap::DashMap;

use crate::error::ARCPError;
use crate::ids::ArtifactId;
use crate::messages::ArtifactRef;

/// In-memory artifact store.
#[derive(Clone, Default)]
pub struct ArtifactStore {
    inner: Arc<DashMap<ArtifactId, StoredArtifact>>,
    default_retention: Duration,
}

impl std::fmt::Debug for ArtifactStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ArtifactStore")
            .field("len", &self.inner.len())
            .field("default_retention_secs", &self.default_retention.as_secs())
            .finish()
    }
}

#[derive(Debug, Clone)]
struct StoredArtifact {
    media_type: String,
    bytes: Vec<u8>,
    /// Reserved for §16.1 integrity verification — currently the field
    /// echoes back what the caller supplied; verification is deferred.
    #[allow(dead_code)]
    sha256: Option<String>,
    expires_at: Option<SystemTime>,
}

/// Maximum payload size accepted in `artifact.put` (PLAN.md §A4.11 choice).
const MAX_INLINE_BYTES: usize = 4 * 1024 * 1024;

impl ArtifactStore {
    /// Construct an empty store with a 1-hour default retention.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(DashMap::new()),
            default_retention: Duration::from_secs(3600),
        }
    }

    /// Override the default retention duration.
    #[must_use]
    pub const fn with_default_retention(mut self, duration: Duration) -> Self {
        self.default_retention = duration;
        self
    }

    /// Store a base64-encoded body. Returns the new [`ArtifactRef`].
    ///
    /// # Errors
    ///
    /// Returns [`ARCPError::InvalidArgument`] if `data` is not valid base64
    /// or exceeds the inline size cap (4 MiB after base64 decode, per
    /// PLAN.md §A4.11).
    pub fn put(
        &self,
        media_type: impl Into<String>,
        data: &str,
        retain_seconds: Option<u64>,
        sha256: Option<String>,
    ) -> Result<ArtifactRef, ARCPError> {
        let bytes = B64.decode(data).map_err(|e| ARCPError::InvalidArgument {
            detail: format!("invalid base64 in artifact.put: {e}"),
        })?;
        if bytes.len() > MAX_INLINE_BYTES {
            return Err(ARCPError::InvalidArgument {
                detail: format!(
                    "artifact body exceeds {MAX_INLINE_BYTES} bytes (got {})",
                    bytes.len()
                ),
            });
        }
        let id = ArtifactId::new();
        let media_type = media_type.into();
        let expires_at = retain_seconds
            .map(Duration::from_secs)
            .or(Some(self.default_retention))
            .map(|d| SystemTime::now() + d);
        let size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        let stored = StoredArtifact {
            media_type: media_type.clone(),
            bytes,
            sha256: sha256.clone(),
            expires_at,
        };
        self.inner.insert(id.clone(), stored);
        Ok(ArtifactRef {
            artifact_id: id.clone(),
            uri: format!("arcp://artifact/{id}"),
            media_type,
            size,
            sha256,
            expires_at: expires_at.map(chrono::DateTime::<chrono::Utc>::from),
        })
    }

    /// Fetch an artifact by id. Returns base64-encoded body alongside its
    /// `media_type`.
    ///
    /// # Errors
    ///
    /// Returns [`ARCPError::NotFound`] if the artifact is unknown or has
    /// expired (the store sweeps lazily on read).
    pub fn fetch(&self, id: &ArtifactId) -> Result<(String, String), ARCPError> {
        if let Some(entry) = self.inner.get(id) {
            if entry.expires_at.is_some_and(|t| SystemTime::now() > t) {
                drop(entry);
                self.inner.remove(id);
                return Err(ARCPError::NotFound {
                    kind: "artifact",
                    id: id.to_string(),
                });
            }
            let body = B64.encode(&entry.bytes);
            Ok((body, entry.media_type.clone()))
        } else {
            Err(ARCPError::NotFound {
                kind: "artifact",
                id: id.to_string(),
            })
        }
    }

    /// Drop an artifact from the store (regardless of expiry).
    pub fn release(&self, id: &ArtifactId) {
        self.inner.remove(id);
    }

    /// Number of stored artifacts (does not sweep).
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// True if the store is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Sweep expired artifacts. Returns the number removed.
    #[must_use]
    pub fn sweep_expired(&self) -> usize {
        let now = SystemTime::now();
        let expired: Vec<ArtifactId> = self
            .inner
            .iter()
            .filter_map(|r| {
                if r.value().expires_at.is_some_and(|t| now > t) {
                    Some(r.key().clone())
                } else {
                    None
                }
            })
            .collect();
        let n = expired.len();
        for id in expired {
            self.inner.remove(&id);
        }
        n
    }
}

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
    fn put_then_fetch_round_trips_bytes() {
        let store = ArtifactStore::new();
        let body = B64.encode(b"hello world");
        let r = store.put("text/plain", &body, Some(60), None).expect("put");
        assert!(r.uri.starts_with("arcp://artifact/art_"));
        assert_eq!(r.size, b"hello world".len() as u64);

        let (back, media) = store.fetch(&r.artifact_id).expect("fetch");
        assert_eq!(back, body);
        assert_eq!(media, "text/plain");
    }

    #[test]
    fn fetch_missing_returns_not_found() {
        let store = ArtifactStore::new();
        let id = ArtifactId::new();
        let err = store.fetch(&id).expect_err("missing");
        assert!(matches!(
            err,
            ARCPError::NotFound {
                kind: "artifact",
                ..
            }
        ));
    }

    #[test]
    fn release_removes_artifact() {
        let store = ArtifactStore::new();
        let r = store
            .put("application/json", &B64.encode(b"{}"), None, None)
            .expect("put");
        store.release(&r.artifact_id);
        assert!(store.fetch(&r.artifact_id).is_err());
    }

    #[test]
    fn invalid_base64_rejected() {
        let store = ArtifactStore::new();
        let err = store
            .put("text/plain", "!!!not-base64", None, None)
            .expect_err("must reject");
        assert!(matches!(err, ARCPError::InvalidArgument { .. }));
    }

    #[test]
    fn oversize_payload_rejected() {
        let store = ArtifactStore::new();
        let big = vec![0u8; MAX_INLINE_BYTES + 1];
        let err = store
            .put("application/octet-stream", &B64.encode(&big), None, None)
            .expect_err("must reject");
        assert!(matches!(err, ARCPError::InvalidArgument { .. }));
    }
}
