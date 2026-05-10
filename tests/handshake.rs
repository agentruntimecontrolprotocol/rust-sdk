//! Integration tests for the four-step handshake (RFC §8.1).

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_panics_doc
)]

mod common;

use arcp::messages::{Capabilities, CapabilityName};
use common::{bearer, none_creds, spawn_runtime_with_bearer, test_client_identity};

#[tokio::test]
async fn happy_path_bearer_authenticates_and_negotiates_capabilities() {
    let (_runtime, client) = spawn_runtime_with_bearer("good-token", "alice", false).await;

    let session = client
        .open()
        .expect("open")
        .authenticate(
            bearer("good-token"),
            test_client_identity(),
            Capabilities {
                streaming: Some(true),
                human_input: Some(true),
                ..Default::default()
            },
        )
        .await
        .expect("authenticate");

    let id = session.id().await.expect("session id");
    assert!(id.as_str().starts_with("sess_"));

    let caps = session.capabilities().await;
    assert!(caps.has(CapabilityName::Streaming));
    assert!(caps.has(CapabilityName::HumanInput));
    // Runtime didn't advertise scheduled_jobs, so even if the client
    // had asked for it, the negotiated set must drop it.
    assert!(!caps.has(CapabilityName::ScheduledJobs));
}

#[tokio::test]
async fn bad_bearer_token_yields_unauthenticated() {
    let (_runtime, client) = spawn_runtime_with_bearer("good-token", "alice", false).await;

    let err = client
        .open()
        .expect("open")
        .authenticate(
            bearer("WRONG"),
            test_client_identity(),
            Capabilities::default(),
        )
        .await
        .expect_err("authentication must fail");

    let s = err.to_string();
    assert!(
        s.contains("unauthenticated") || s.contains("Unauthenticated"),
        "got: {s}"
    );
}

#[tokio::test]
async fn anonymous_succeeds_only_when_capability_negotiated() {
    // anonymous=false → reject
    let (_rt1, client1) = spawn_runtime_with_bearer("ignored", "ignored", false).await;
    let err = client1
        .open()
        .expect("open")
        .authenticate(
            none_creds(),
            test_client_identity(),
            Capabilities::default(),
        )
        .await
        .expect_err("must reject");
    assert!(err.to_string().contains("anonymous"));

    // anonymous=true on both sides → accept
    let (_rt2, client2) = spawn_runtime_with_bearer("ignored", "ignored", true).await;
    let session = client2
        .open()
        .expect("open")
        .authenticate(
            none_creds(),
            test_client_identity(),
            Capabilities {
                anonymous: Some(true),
                ..Default::default()
            },
        )
        .await
        .expect("anonymous accepted when negotiated");
    assert!(session
        .id()
        .await
        .expect("id")
        .as_str()
        .starts_with("sess_"));
}

#[tokio::test]
async fn unconfigured_scheme_is_rejected() {
    let (_runtime, client) = spawn_runtime_with_bearer("good-token", "alice", false).await;

    let err = client
        .open()
        .expect("open")
        .authenticate(
            arcp::messages::Credentials {
                scheme: arcp::messages::AuthScheme::Oauth2,
                token: Some("opaque".into()),
            },
            test_client_identity(),
            Capabilities::default(),
        )
        .await
        .expect_err("oauth2 not registered");
    assert!(err.to_string().contains("not configured"), "got: {err}");
}
