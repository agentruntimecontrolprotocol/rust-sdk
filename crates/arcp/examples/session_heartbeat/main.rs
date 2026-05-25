//! ARCP v1.1 §6.4 — `session.ping` / `session.pong` heartbeat demo.
//!
//! Runs a runtime and a client over an in-process [`paired`] transport.
//! The client emits a `session.ping` every 200 ms. The runtime responds
//! with `session.pong` echoing the nonce. The client logs each
//! round-trip and exits after a few exchanges.
//!
//! Run with:
//!     `cargo run --example session_heartbeat`

#![allow(clippy::similar_names)]

use std::time::Duration;

use arcp::auth::BearerAuthenticator;
use arcp::envelope::Envelope;
use arcp::messages::{
    AuthScheme, Capabilities, ClientIdentity, Credentials, MessageType, SessionOpenPayload,
    SessionPingPayload,
};
use arcp::runtime::ARCPRuntime;
use arcp::transport::{paired, Transport};

const HEARTBEAT_INTERVAL: Duration = Duration::from_millis(200);
const PINGS_TO_SEND: usize = 3;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let runtime = ARCPRuntime::builder()
        .with_authenticator(Box::new(BearerAuthenticator::new().with_token("t", "p")))
        .build()
        .await?;
    let (server_t, client_t) = paired();
    let _h = runtime.serve_connection(server_t);

    // Drive the handshake.
    let mut open = Envelope::new(MessageType::SessionOpen(SessionOpenPayload {
        auth: Credentials {
            scheme: AuthScheme::Bearer,
            token: Some("t".into()),
        },
        client: ClientIdentity {
            kind: "heartbeat-demo".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            fingerprint: None,
            principal: None,
        },
        capabilities: Capabilities::default(),
    }));
    open.id = arcp::ids::MessageId::new();
    client_t.send(open).await?;
    let accepted = client_t.recv().await?.ok_or("no session.accepted")?;
    let MessageType::SessionAccepted(payload) = accepted.payload else {
        return Err("expected session.accepted".into());
    };
    let session_id = payload.session_id;
    println!("handshake complete: session_id={session_id:?}");

    for i in 0..PINGS_TO_SEND {
        let nonce = format!("p_{i:03}");
        let mut ping = Envelope::new(MessageType::SessionPing(SessionPingPayload {
            nonce: nonce.clone(),
            sent_at: chrono::Utc::now(),
        }));
        ping.session_id = Some(session_id.clone());
        let ping_id = ping.id.clone();
        client_t.send(ping).await?;
        let pong = tokio::time::timeout(Duration::from_secs(1), client_t.recv())
            .await
            .map_err(|_| "no pong within 1s")??
            .ok_or("transport closed")?;
        let MessageType::SessionPong(p) = pong.payload else {
            return Err("expected session.pong".into());
        };
        assert_eq!(p.ping_nonce, nonce);
        assert_eq!(pong.correlation_id.as_ref(), Some(&ping_id));
        println!(
            "round-trip {i}: nonce={} received_at={}",
            p.ping_nonce, p.received_at
        );
        tokio::time::sleep(HEARTBEAT_INTERVAL).await;
    }

    println!("done; sent {PINGS_TO_SEND} heartbeats");
    Ok(())
}
