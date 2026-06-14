//! Integration tests for ARCP v1.1 §12 wire error codes that the runtime
//! emits on the `job.failed` path: `DUPLICATE_KEY` (§7.2 idempotency
//! conflict) and `AGENT_NOT_AVAILABLE` (§7.5 unregistered agent).

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
use arcp::error::{ARCPError, ErrorCode};
use arcp::ids::IdempotencyKey;
use arcp::messages::{
    AuthScheme, Capabilities, ClientIdentity, Credentials, MessageType, ToolInvokePayload,
};
use arcp::runtime::tools::{ToolHandler, ToolRegistryBuilder};
use arcp::runtime::ARCPRuntime;
use arcp::transport::{paired, MemoryTransport, Transport};
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
        _ctx: arcp::runtime::context::ToolContext,
    ) -> Result<serde_json::Value, ARCPError> {
        Ok(arguments)
    }
}

async fn spawn_and_handshake() -> (MemoryTransport, arcp::ids::SessionId) {
    let tools = ToolRegistryBuilder::new().with(Arc::new(EchoTool)).build();
    let runtime = ARCPRuntime::builder()
        .with_authenticator(Box::new(BearerAuthenticator::new().with_token("t", "p")))
        .with_tools(tools)
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
    (client_t, session_id)
}

async fn recv_until_job_failed(client_t: &MemoryTransport) -> arcp::messages::JobFailedPayload {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    loop {
        let env = tokio::time::timeout_at(deadline, client_t.recv())
            .await
            .expect("timely")
            .expect("recv")
            .expect("present");
        if let MessageType::JobFailed(p) = env.payload {
            return p;
        }
    }
}

/// ARCP v1.1 §7.2 / §12 — a reused `idempotency_key` carrying conflicting
/// arguments is rejected with the `DUPLICATE_KEY` wire code.
#[tokio::test]
async fn idempotency_conflict_yields_duplicate_key() {
    let (client_t, session_id) = spawn_and_handshake().await;
    let key = IdempotencyKey::new("dup-key-1").expect("key");

    // First invoke binds the key to (echo, {"a":1}).
    let mut first = Envelope::new(MessageType::ToolInvoke(ToolInvokePayload::new(
        "echo",
        serde_json::json!({"a": 1}),
    )));
    first.session_id = Some(session_id.clone());
    first.idempotency_key = Some(key.clone());
    client_t.send(first).await.expect("send first");

    // Drain first job.accepted so the index records the binding.
    let _accepted = tokio::time::timeout(Duration::from_millis(500), client_t.recv())
        .await
        .expect("timely")
        .expect("recv")
        .expect("present");

    // Second invoke reuses the key with conflicting arguments.
    let mut second = Envelope::new(MessageType::ToolInvoke(ToolInvokePayload::new(
        "echo",
        serde_json::json!({"a": 2}),
    )));
    second.session_id = Some(session_id);
    second.idempotency_key = Some(key);
    client_t.send(second).await.expect("send second");

    let failed = recv_until_job_failed(&client_t).await;
    assert_eq!(failed.code, ErrorCode::DuplicateKey);
    assert_eq!(failed.code.as_str(), "DUPLICATE_KEY");
    assert_eq!(failed.retryable, Some(false));
}

/// ARCP v1.1 §12 — invoking an unregistered agent yields
/// `AGENT_NOT_AVAILABLE`, not the generic `NOT_FOUND`.
#[tokio::test]
async fn unregistered_agent_yields_agent_not_available() {
    let (client_t, session_id) = spawn_and_handshake().await;

    let mut invoke = Envelope::new(MessageType::ToolInvoke(ToolInvokePayload::new(
        "never-registered",
        serde_json::json!({}),
    )));
    invoke.session_id = Some(session_id);
    client_t.send(invoke).await.expect("send invoke");

    let failed = recv_until_job_failed(&client_t).await;
    assert_eq!(failed.code, ErrorCode::AgentNotAvailable);
    assert_eq!(failed.code.as_str(), "AGENT_NOT_AVAILABLE");
    assert_eq!(failed.retryable, Some(false));
}
