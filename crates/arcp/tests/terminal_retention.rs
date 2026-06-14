//! Integration tests for terminal-job retention (#72) and idempotency-index
//! bounding (#85). Terminal jobs and their idempotency records are evicted
//! once they age past the configured retention window.

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

async fn handshake(client_t: &MemoryTransport) -> arcp::ids::SessionId {
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
    match accept.payload {
        MessageType::SessionAccepted(p) => p.session_id,
        other => panic!("expected accepted, got {other:?}"),
    }
}

/// Drain envelopes until `count` job.completed envelopes have been seen.
async fn drain_completions(client_t: &MemoryTransport, count: usize) {
    let mut seen = 0;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while seen < count {
        let env = tokio::time::timeout_at(deadline, client_t.recv())
            .await
            .expect("timely")
            .expect("recv")
            .expect("present");
        if matches!(env.payload, MessageType::JobCompleted(_)) {
            seen += 1;
        }
    }
}

/// ARCP §10.1 retention — with a zero retention window, completed jobs and
/// their idempotency records are removed on the next maintenance sweep, so
/// the registry does not grow with historical terminal jobs (#72, #85).
#[tokio::test]
async fn terminal_jobs_and_idempotency_records_are_swept() {
    const N: usize = 5;
    let runtime = ARCPRuntime::builder()
        .with_authenticator(Box::new(BearerAuthenticator::new().with_token("t", "p")))
        .with_tools(ToolRegistryBuilder::new().with(Arc::new(EchoTool)).build())
        .with_terminal_retention(Duration::ZERO)
        .build()
        .await
        .expect("build");
    let (server_t, client_t) = paired();
    let _h = runtime.serve_connection(server_t);
    let session_id = handshake(&client_t).await;

    for i in 0..N {
        let mut invoke = Envelope::new(MessageType::ToolInvoke(ToolInvokePayload::new(
            "echo",
            serde_json::json!({ "i": i }),
        )));
        invoke.session_id = Some(session_id.clone());
        invoke.idempotency_key = Some(IdempotencyKey::new(format!("key-{i}")).expect("key"));
        client_t.send(invoke).await.expect("send invoke");
    }

    drain_completions(&client_t, N).await;

    assert_eq!(runtime.job_count(), N, "all jobs retained pre-sweep");
    assert_eq!(
        runtime.idempotency_index_len(),
        N,
        "one idempotency record per distinct key"
    );

    let swept = runtime.sweep_terminal_jobs();
    assert_eq!(swept, N, "all terminal jobs swept past zero window");
    assert_eq!(runtime.job_count(), 0, "registry bounded after sweep");
    assert_eq!(
        runtime.idempotency_index_len(),
        0,
        "idempotency records evicted with their jobs"
    );
}

/// Within the retention window, an idempotent replay still resolves to the
/// original `job.accepted` (#85 — replay must survive until the window
/// closes).
#[tokio::test]
async fn idempotent_replay_resolves_within_retention_window() {
    let runtime = ARCPRuntime::builder()
        .with_authenticator(Box::new(BearerAuthenticator::new().with_token("t", "p")))
        .with_tools(ToolRegistryBuilder::new().with(Arc::new(EchoTool)).build())
        .with_terminal_retention(Duration::from_secs(3600))
        .build()
        .await
        .expect("build");
    let (server_t, client_t) = paired();
    let _h = runtime.serve_connection(server_t);
    let session_id = handshake(&client_t).await;

    let key = IdempotencyKey::new("stable-key").expect("key");
    let args = serde_json::json!({ "v": 1 });

    let mut first = Envelope::new(MessageType::ToolInvoke(ToolInvokePayload::new(
        "echo",
        args.clone(),
    )));
    first.session_id = Some(session_id.clone());
    first.idempotency_key = Some(key.clone());
    client_t.send(first).await.expect("send first");
    let accepted = client_t.recv().await.expect("recv").expect("present");
    let MessageType::JobAccepted(first_accept) = accepted.payload else {
        panic!("expected job.accepted");
    };

    // A sweep inside the window must not evict the live record.
    let _ = runtime.sweep_terminal_jobs();

    // Replay the exact same command intent.
    let mut replay = Envelope::new(MessageType::ToolInvoke(ToolInvokePayload::new(
        "echo", args,
    )));
    replay.session_id = Some(session_id);
    replay.idempotency_key = Some(key);
    client_t.send(replay).await.expect("send replay");

    // The replay resolves to the SAME job id.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    loop {
        let env = tokio::time::timeout_at(deadline, client_t.recv())
            .await
            .expect("timely")
            .expect("recv")
            .expect("present");
        if let MessageType::JobAccepted(replay_accept) = env.payload {
            assert_eq!(
                replay_accept.job_id, first_accept.job_id,
                "idempotent replay must resolve to the original job"
            );
            break;
        }
    }
}
