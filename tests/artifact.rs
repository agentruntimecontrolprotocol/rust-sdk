//! Integration tests for the artifact store (RFC §16).
//!
//! Phase 5 exercises [`ArtifactStore`] directly. Wire-level
//! `artifact.put` / `artifact.fetch` dispatch through the runtime is left
//! to a follow-up — the store is structured for that integration.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_panics_doc
)]

use std::time::Duration;

use arcp::error::ARCPError;
use arcp::ids::ArtifactId;
use arcp::runtime::ArtifactStore;
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;

#[test]
fn put_then_fetch_round_trips_bytes() {
    let store = ArtifactStore::new();
    let body = B64.encode(b"hello arcp");
    let r = store
        .put("application/octet-stream", &body, Some(60), None)
        .expect("put");
    assert!(r.uri.starts_with("arcp://artifact/"));

    let (got_body, media) = store.fetch(&r.artifact_id).expect("fetch");
    assert_eq!(got_body, body);
    assert_eq!(media, "application/octet-stream");
}

#[test]
fn release_then_fetch_yields_not_found() {
    let store = ArtifactStore::new();
    let body = B64.encode(b"transient");
    let r = store.put("text/plain", &body, None, None).expect("put");
    store.release(&r.artifact_id);
    let err = store.fetch(&r.artifact_id).expect_err("must fail");
    assert!(matches!(
        err,
        ARCPError::NotFound {
            kind: "artifact",
            ..
        }
    ));
}

#[test]
fn fetching_unknown_id_yields_not_found() {
    let store = ArtifactStore::new();
    let err = store.fetch(&ArtifactId::new()).expect_err("not found");
    assert!(matches!(
        err,
        ARCPError::NotFound {
            kind: "artifact",
            ..
        }
    ));
}

#[test]
fn invalid_base64_input_rejected() {
    let store = ArtifactStore::new();
    let err = store
        .put("text/plain", "not!base64!", None, None)
        .expect_err("must reject");
    assert!(matches!(err, ARCPError::InvalidArgument { .. }));
}

#[test]
fn sweep_expired_drops_past_retention() {
    // Construct a store with zero default retention so any insertion is
    // already expired.
    let store = ArtifactStore::new().with_default_retention(Duration::from_secs(0));
    let body = B64.encode(b"vanish");
    let r = store.put("text/plain", &body, None, None).expect("put");
    let dropped = store.sweep_expired();
    assert!(
        dropped >= 1,
        "expected at least one expired artifact; got {dropped}"
    );
    assert!(store.fetch(&r.artifact_id).is_err());
}
