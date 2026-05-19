//! ARCP v1.1 §6.5 — `session.ack` flow control demo.
//!
//! The runtime is built with a 1-slot ack window. After the first
//! countable event flows, the writer parks. The client demonstrates
//! this by attempting to read with a short timeout, observing the
//! pause, sending a `session.ack`, and then draining the remaining
//! events.
//!
//! Run with:
//!     `cargo run --example session_ack`

#![allow(clippy::similar_names, clippy::expect_used)]

use std::sync::Arc;
use std::time::Duration;

use arcp::auth::BearerAuthenticator;
use arcp::envelope::Envelope;
use arcp::error::ARCPError;
use arcp::messages::{
    AuthScheme, Capabilities, ClientIdentity, Credentials, MessageType, SessionAckPayload,
    SessionOpenPayload, ToolInvokePayload,
};
use arcp::runtime::context::ToolContext;
use arcp::runtime::tools::{ToolHandler, ToolRegistryBuilder};
use arcp::runtime::ARCPRuntime;
use arcp::transport::{paired, Transport};
use async_trait::async_trait;

struct EchoTool;

#[async_trait]
impl ToolHandler for EchoTool {
    fn name(&self) -> &'static str {
        "echo"
    }

    async fn invoke(
        &self,
        arguments: serde_json::Value,
        _ctx: ToolContext,
    ) -> Result<serde_json::Value, ARCPError> {
        Ok(arguments)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let tools = ToolRegistryBuilder::new().with(Arc::new(EchoTool)).build();
    let runtime = ARCPRuntime::builder()
        .with_authenticator(Box::new(BearerAuthenticator::new().with_token("t", "p")))
        .with_tools(tools)
        .with_ack_window(1)
        .build()
        .await?;
    let (server_t, client_t) = paired();
    let _h = runtime.serve_connection(server_t);

    // Handshake.
    let mut open = Envelope::new(MessageType::SessionOpen(SessionOpenPayload {
        auth: Credentials {
            scheme: AuthScheme::Bearer,
            token: Some("t".into()),
        },
        client: ClientIdentity {
            kind: "ack-demo".into(),
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

    // Submit one echo job. The runtime will try to emit
    // job.accepted -> job.started -> job.completed, all countable.
    let mut invoke = Envelope::new(MessageType::ToolInvoke(ToolInvokePayload::new(
        "echo",
        serde_json::json!({"hello": "world"}),
    )));
    invoke.session_id = Some(session_id.clone());
    client_t.send(invoke).await?;

    // First countable event flows through.
    let first = client_t.recv().await?.ok_or("no first event")?;
    println!(
        "drained 1 countable event: type={} (writer now parked at window=1)",
        first.payload.type_name()
    );

    // Demonstrate the pause: with window exhausted, the next recv times
    // out — the runtime is gated waiting for our session.ack.
    let blocked = tokio::time::timeout(Duration::from_millis(200), client_t.recv()).await;
    assert!(
        blocked.is_err(),
        "expected timeout; writer should be paused"
    );
    println!("verified writer is parked (timeout reached without delivery)");

    // Advance the watermark — runtime resumes.
    let mut ack = Envelope::new(MessageType::SessionAck(SessionAckPayload {
        last_processed_seq: 10,
    }));
    ack.session_id = Some(session_id);
    client_t.send(ack).await?;

    // Drain remaining events to terminal.
    let mut count = 1;
    loop {
        let env = tokio::time::timeout(Duration::from_secs(1), client_t.recv())
            .await??
            .ok_or("transport closed")?;
        count += 1;
        println!(
            "drained countable event #{count}: type={}",
            env.payload.type_name()
        );
        if matches!(
            env.payload,
            MessageType::JobCompleted(_) | MessageType::JobFailed(_) | MessageType::JobCancelled(_)
        ) {
            break;
        }
    }

    println!("done; total events received={count}");
    Ok(())
}
