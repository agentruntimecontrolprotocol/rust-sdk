//! Integration tests for `artifact.put` / `artifact.fetch` /
//! `artifact.release` dispatch through the runtime (RFC §16.2).

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_panics_doc
)]

use arcp::auth::BearerAuthenticator;
use arcp::error::ARCPError;
use arcp::messages::{AuthScheme, Capabilities, ClientIdentity, Credentials};
use arcp::runtime::ARCPRuntime;
use arcp::transport::paired;
use arcp::ARCPClient;
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;

async fn open_session(
) -> arcp::client::Session<arcp::client::Authenticated, arcp::transport::MemoryTransport> {
    let runtime = ARCPRuntime::builder()
        .with_authenticator(Box::new(BearerAuthenticator::new().with_token("t", "p")))
        .with_capabilities(Capabilities {
            artifacts: Some(true),
            ..Default::default()
        })
        .build()
        .await
        .expect("build");
    let (server_t, client_t) = paired();
    let _h = runtime.serve_connection(server_t);
    ARCPClient::new(client_t)
        .open()
        .expect("open")
        .authenticate(
            Credentials {
                scheme: AuthScheme::Bearer,
                token: Some("t".into()),
            },
            ClientIdentity {
                kind: "artifact-test".into(),
                version: "0".into(),
                fingerprint: None,
                principal: None,
            },
            Capabilities {
                artifacts: Some(true),
                ..Default::default()
            },
        )
        .await
        .expect("auth")
}

#[tokio::test]
async fn put_then_fetch_round_trip() {
    let session = open_session().await;
    let body = B64.encode(b"hello arcp");

    let reference = session
        .put_artifact("text/plain", body.clone(), Some(30))
        .await
        .expect("put");
    assert_eq!(reference.media_type, "text/plain");
    assert!(reference.uri.starts_with("arcp://artifact/"));

    let (got_body, media) = session
        .fetch_artifact(reference.artifact_id.clone())
        .await
        .expect("fetch");
    assert_eq!(got_body, body);
    assert_eq!(media, "text/plain");
}

#[tokio::test]
async fn release_then_fetch_yields_not_found() {
    let session = open_session().await;
    let body = B64.encode(b"transient");

    let reference = session
        .put_artifact("application/octet-stream", body, None)
        .await
        .expect("put");
    session
        .release_artifact(reference.artifact_id.clone())
        .await
        .expect("release");

    let err = session
        .fetch_artifact(reference.artifact_id)
        .await
        .expect_err("must fail");
    assert!(matches!(err, ARCPError::NotFound { .. }), "got: {err}");
}

#[tokio::test]
async fn put_with_invalid_base64_yields_invalid_argument() {
    let session = open_session().await;
    let err = session
        .put_artifact("text/plain", "!!not-base64!!", None)
        .await
        .expect_err("must fail");
    assert!(
        matches!(err, ARCPError::InvalidArgument { .. }),
        "got: {err}"
    );
}

#[tokio::test]
async fn fetching_unknown_id_yields_not_found() {
    let session = open_session().await;
    let id = arcp::ids::ArtifactId::new();
    let err = session.fetch_artifact(id).await.expect_err("must fail");
    assert!(matches!(err, ARCPError::NotFound { .. }), "got: {err}");
}
