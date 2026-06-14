//! Integration tests for `lease_request.expires_at` validation and
//! enforcement (ARCP v1.1 §9.5).
//!
//! §9.5 specifies two MUSTs:
//!
//! 1. `expires_at` MUST be UTC (`Z` suffix) and strictly in the future
//!    at submission. Past or invalid values are rejected with
//!    `INVALID_REQUEST` before any `job.accepted` is emitted.
//! 2. Operations attempted at or after `expires_at` MUST fail with
//!    `LEASE_EXPIRED` (`retryable: false`). The runtime MAY proactively
//!    terminate jobs whose leases have expired.

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
use arcp::messages::{
    AuthScheme, Capabilities, ClientIdentity, Credentials, LeaseRequest, MessageType,
    SessionOpenPayload, ToolInvokePayload,
};
use arcp::runtime::context::ToolContext;
use arcp::runtime::tools::{ToolHandler, ToolRegistryBuilder};
use arcp::runtime::ARCPRuntime;
use arcp::transport::{paired, MemoryTransport, Transport};
use async_trait::async_trait;
use chrono::{Duration as ChronoDuration, Utc};

/// Handler that sleeps long enough to outlive a short-lived lease.
struct SlowTool;

#[async_trait]
impl ToolHandler for SlowTool {
    fn name(&self) -> &'static str {
        "slow"
    }

    async fn invoke(
        &self,
        _arguments: serde_json::Value,
        _ctx: ToolContext,
    ) -> Result<serde_json::Value, ARCPError> {
        tokio::time::sleep(Duration::from_secs(10)).await;
        Ok(serde_json::json!({"ok": true}))
    }
}

/// Handler that completes immediately. Used to confirm that a generous
/// lease does not interfere with normal completion.
struct FastTool;

#[async_trait]
impl ToolHandler for FastTool {
    fn name(&self) -> &'static str {
        "fast"
    }

    async fn invoke(
        &self,
        _arguments: serde_json::Value,
        _ctx: ToolContext,
    ) -> Result<serde_json::Value, ARCPError> {
        Ok(serde_json::json!({"ok": true}))
    }
}

/// Lightweight session+transport bundle.
struct Session {
    transport: MemoryTransport,
    session_id: arcp::ids::SessionId,
}

impl Session {
    async fn submit(&self, tool: &'static str, lease_request: Option<LeaseRequest>) {
        let mut invoke = Envelope::new(MessageType::ToolInvoke(ToolInvokePayload {
            tool: tool.into(),
            arguments: serde_json::json!({}),
            cost_budget: None,
            lease_request,
        }));
        invoke.session_id = Some(self.session_id.clone());
        self.transport.send(invoke).await.expect("send invoke");
    }

    async fn next_terminal(&self) -> arcp::messages::JobFailedPayload {
        loop {
            let env = tokio::time::timeout(Duration::from_secs(5), self.transport.recv())
                .await
                .expect("timely")
                .expect("recv")
                .expect("envelope");
            match env.payload {
                MessageType::JobFailed(p) => return p,
                MessageType::JobCompleted(_) => {
                    panic!("expected job.failed, got job.completed");
                }
                MessageType::JobCancelled(_) => {
                    panic!("expected job.failed, got job.cancelled");
                }
                _ => {}
            }
        }
    }

    async fn recv_until_completed(&self) {
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            let env = tokio::time::timeout(Duration::from_millis(500), self.transport.recv())
                .await
                .expect("timely")
                .expect("recv")
                .expect("envelope");
            if matches!(env.payload, MessageType::JobCompleted(_)) {
                return;
            }
        }
        panic!("did not see job.completed within deadline");
    }
}

async fn boot(tools: ToolRegistryBuilder) -> Session {
    let runtime = ARCPRuntime::builder()
        .with_authenticator(Box::new(BearerAuthenticator::new().with_token("t", "p")))
        .with_tools(tools.build())
        .build()
        .await
        .expect("build");
    let (server_t, client_t) = paired();
    let _handle = runtime.serve_connection(server_t);

    let mut open = Envelope::new(MessageType::SessionOpen(SessionOpenPayload {
        auth: Credentials {
            scheme: AuthScheme::Bearer,
            token: Some("t".into()),
        },
        client: ClientIdentity {
            kind: "lease-expiry-test".into(),
            version: "0".into(),
            fingerprint: None,
            principal: None,
        },
        capabilities: Capabilities::default(),
    }));
    open.id = arcp::ids::MessageId::new();
    client_t.send(open).await.expect("send open");
    let accepted = client_t
        .recv()
        .await
        .expect("recv")
        .expect("session.accepted envelope");
    let MessageType::SessionAccepted(payload) = accepted.payload else {
        panic!("expected session.accepted");
    };
    Session {
        transport: client_t,
        session_id: payload.session_id,
    }
}

#[tokio::test]
async fn past_expires_at_is_rejected_with_invalid_request() {
    // §9.5: expires_at MUST be in the future at submission. A past value
    // is rejected with INVALID_REQUEST and no job.accepted is emitted.
    let session = boot(ToolRegistryBuilder::new().with(Arc::new(FastTool))).await;
    let lease = LeaseRequest {
        expires_at: Some(Utc::now() - ChronoDuration::seconds(60)),
        ..LeaseRequest::default()
    };
    session.submit("fast", Some(lease)).await;

    let failed = session.next_terminal().await;
    assert_eq!(
        failed.code,
        ErrorCode::InvalidRequest,
        "past expires_at must yield INVALID_REQUEST: {failed:?}"
    );
    assert_eq!(failed.retryable, Some(false));
    assert!(
        failed.message.contains("expires_at"),
        "error message should reference expires_at: {}",
        failed.message,
    );
}

#[tokio::test]
async fn future_expires_at_is_accepted_and_job_completes() {
    // A generous future expires_at MUST NOT interfere with a fast job.
    let session = boot(ToolRegistryBuilder::new().with(Arc::new(FastTool))).await;
    let lease = LeaseRequest {
        expires_at: Some(Utc::now() + ChronoDuration::seconds(60)),
        ..LeaseRequest::default()
    };
    session.submit("fast", Some(lease)).await;
    session.recv_until_completed().await;
}

#[tokio::test]
async fn lease_expiry_during_execution_yields_lease_expired() {
    // §9.5: the runtime MUST surface LEASE_EXPIRED (retryable:false)
    // when the handler is still running at expires_at. SlowTool sleeps
    // for 10s, but the lease only covers ~250ms, so the runtime should
    // preempt the handler with LEASE_EXPIRED.
    let session = boot(ToolRegistryBuilder::new().with(Arc::new(SlowTool))).await;
    let lease = LeaseRequest {
        expires_at: Some(Utc::now() + ChronoDuration::milliseconds(250)),
        ..LeaseRequest::default()
    };
    session.submit("slow", Some(lease)).await;

    let failed = session.next_terminal().await;
    assert_eq!(
        failed.code,
        ErrorCode::LeaseExpired,
        "in-flight overrun must yield LEASE_EXPIRED: {failed:?}"
    );
    assert_eq!(
        failed.retryable,
        Some(false),
        "§12: LEASE_EXPIRED MUST be retryable:false"
    );
    assert!(
        failed.message.contains("lease expired"),
        "error message should reference lease expiry: {}",
        failed.message,
    );
}
