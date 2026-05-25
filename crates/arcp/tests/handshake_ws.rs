//! End-to-end handshake against the real `WebSocket` transport (RFC §22).

#![cfg(feature = "transport-ws")]
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_panics_doc
)]

use arcp::auth::BearerAuthenticator;
use arcp::messages::{AuthScheme, Capabilities, ClientIdentity, Credentials};
use arcp::runtime::ARCPRuntime;
use arcp::transport::websocket::WebSocketTransport;
use arcp::ARCPClient;

#[tokio::test]
async fn handshake_round_trip_over_real_websocket() {
    let runtime = ARCPRuntime::builder()
        .with_authenticator(Box::new(
            BearerAuthenticator::new().with_token("t", "alice"),
        ))
        .with_capabilities(Capabilities {
            streaming: Some(true),
            ..Default::default()
        })
        .build()
        .await
        .expect("build");

    let (server_t, client_t) = WebSocketTransport::serve_loopback()
        .await
        .expect("loopback");
    let _h = runtime.serve_connection(server_t);

    let session = ARCPClient::new(client_t)
        .open()
        .expect("open")
        .authenticate(
            Credentials {
                scheme: AuthScheme::Bearer,
                token: Some("t".into()),
            },
            ClientIdentity {
                kind: "ws-test".into(),
                version: "0".into(),
                fingerprint: None,
                principal: None,
            },
            Capabilities {
                streaming: Some(true),
                ..Default::default()
            },
        )
        .await
        .expect("auth over WS");

    let id = session.id().await.expect("session id");
    assert!(id.as_str().starts_with("sess_"));
}
