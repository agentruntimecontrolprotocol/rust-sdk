//! Integration test for ARCP v1.1 §6.7: a graceful `session.close`
//! terminates the session but MUST NOT cancel in-flight jobs — they keep
//! running and remain resumable. The runtime acknowledges with
//! `session.closed`.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_panics_doc
)]

use std::sync::Arc;
use std::time::Duration;

use arcp::auth::BearerAuthenticator;
use arcp::envelope::Envelope;
use arcp::error::ARCPError;
use arcp::messages::{
    AuthScheme, Capabilities, ClientIdentity, Credentials, JobState, MessageType,
    SessionClosePayload, ToolInvokePayload,
};
use arcp::runtime::context::ToolContext;
use arcp::runtime::tools::{ToolHandler, ToolRegistryBuilder};
use arcp::runtime::ARCPRuntime;
use arcp::transport::{paired, Transport};
use async_trait::async_trait;

struct SlowTool;

#[async_trait]
impl ToolHandler for SlowTool {
    fn name(&self) -> &'static str {
        "slow"
    }
    async fn invoke(
        &self,
        arguments: serde_json::Value,
        ctx: ToolContext,
    ) -> Result<serde_json::Value, ARCPError> {
        tokio::select! {
            () = ctx.cancel.cancelled() => Err(ARCPError::Cancelled { reason: "cancelled".into() }),
            () = tokio::time::sleep(Duration::from_millis(200)) => Ok(arguments),
        }
    }
}

#[tokio::test]
async fn session_close_leaves_in_flight_job_running_and_acks_closed() {
    let runtime = ARCPRuntime::builder()
        .with_authenticator(Box::new(BearerAuthenticator::new().with_token("t", "p")))
        .with_tools(ToolRegistryBuilder::new().with(Arc::new(SlowTool)).build())
        .build()
        .await
        .expect("build");
    let (server_t, client_t) = paired();
    let _h = runtime.serve_connection(server_t);

    let mut open = Envelope::new(MessageType::SessionOpen(
        arcp::messages::SessionOpenPayload {
            auth: Credentials {
                scheme: AuthScheme::Bearer,
                token: Some("t".into()),
            },
            client: ClientIdentity {
                kind: "test".into(),
                version: "0".into(),
                fingerprint: None,
                principal: None,
            },
            capabilities: Capabilities::default(),
        },
    ));
    open.id = arcp::ids::MessageId::new();
    client_t.send(open).await.expect("send open");
    let accept = client_t.recv().await.expect("recv").expect("present");
    let session_id = match accept.payload {
        MessageType::SessionAccepted(p) => p.session_id,
        other => panic!("expected accepted, got {other:?}"),
    };

    // Submit a slow job and capture its id from job.accepted.
    let mut invoke = Envelope::new(MessageType::ToolInvoke(ToolInvokePayload::new(
        "slow",
        serde_json::json!({"v": 1}),
    )));
    invoke.session_id = Some(session_id.clone());
    client_t.send(invoke).await.expect("send invoke");
    let accepted = client_t.recv().await.expect("recv").expect("present");
    let MessageType::JobAccepted(accepted) = accepted.payload else {
        panic!("expected job.accepted, got {:?}", accepted.payload);
    };
    let job_id = accepted.job_id;

    // Gracefully close the session immediately.
    let mut close = Envelope::new(MessageType::SessionClose(SessionClosePayload {
        reason: None,
    }));
    close.session_id = Some(session_id);
    client_t.send(close).await.expect("send close");

    // The runtime must acknowledge with session.closed.
    let mut saw_closed = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(1);
    while let Ok(Ok(Some(env))) = tokio::time::timeout_at(deadline, client_t.recv()).await {
        if matches!(env.payload, MessageType::SessionClosed(_)) {
            saw_closed = true;
            break;
        }
    }
    assert!(
        saw_closed,
        "runtime must ack session.close with session.closed"
    );

    // The job must NOT have been cancelled by the close; it keeps running
    // and reaches a terminal Completed state, and remains visible.
    let mut final_state = None;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    while tokio::time::Instant::now() < deadline {
        if let Some(snap) = runtime.job_snapshot(&job_id) {
            if snap.state.is_terminal() {
                final_state = Some(snap.state);
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert_eq!(
        final_state,
        Some(JobState::Completed),
        "session.close must not cancel the in-flight job (§6.7)"
    );
}
