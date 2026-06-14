//! Integration tests for message-id-only resume (RFC §19).
//!
//! Phase 5 exercises the event log's replay primitives that resume builds
//! on. Wire-level `resume` envelope handling is left to follow-up work —
//! the storage substrate is in place.

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
use arcp::ids::SessionId;
use arcp::messages::{
    AuthScheme, Capabilities, ClientIdentity, Credentials, MessageType, PingPayload,
    SessionOpenPayload, SessionResumePayload, ToolInvokePayload,
};
use arcp::runtime::tools::{ToolHandler, ToolRegistryBuilder};
use arcp::runtime::ARCPRuntime;
use arcp::store::eventlog::EventLog;
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
        _ctx: arcp::runtime::context::ToolContext,
    ) -> Result<serde_json::Value, ARCPError> {
        Ok(arguments)
    }
}

fn open_envelope() -> Envelope {
    let mut open = Envelope::new(MessageType::SessionOpen(SessionOpenPayload {
        auth: Credentials {
            scheme: AuthScheme::Bearer,
            token: Some("t".into()),
        },
        client: ClientIdentity {
            kind: "resume-test".into(),
            version: "0".into(),
            fingerprint: None,
            principal: None,
        },
        capabilities: Capabilities::default(),
    }));
    open.id = arcp::ids::MessageId::new();
    open
}

async fn boot_runtime() -> ARCPRuntime {
    ARCPRuntime::builder()
        .with_authenticator(Box::new(BearerAuthenticator::new().with_token("t", "p")))
        .with_tools(ToolRegistryBuilder::new().with(Arc::new(EchoTool)).build())
        .build()
        .await
        .expect("build")
}

/// ARCP v1.1 §6.3 — a client reconnects with (`resume_token`,
/// `last_event_seq`) and receives a `session.resumed` ack with a rotated
/// token plus replayed events with `seq > last_event_seq`.
#[tokio::test]
async fn session_resume_replays_events_and_rotates_token() {
    let runtime = boot_runtime().await;

    // Connection 1: open, capture session_id + resume_token, run a job.
    let (s1, c1) = paired();
    let _h1 = runtime.serve_connection(s1);
    c1.send(open_envelope()).await.expect("open");
    let accepted = c1.recv().await.expect("recv").expect("present");
    let (session_id, token) = match accepted.payload {
        MessageType::SessionAccepted(p) => (p.session_id, p.resume_token.expect("resume token")),
        other => panic!("expected accepted, got {other:?}"),
    };

    let mut invoke = Envelope::new(MessageType::ToolInvoke(ToolInvokePayload::new(
        "echo",
        serde_json::json!({"v": 1}),
    )));
    invoke.session_id = Some(session_id.clone());
    c1.send(invoke).await.expect("invoke");
    // Drain until job.completed so events are buffered.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    loop {
        let env = tokio::time::timeout_at(deadline, c1.recv())
            .await
            .expect("timely")
            .expect("recv")
            .expect("present");
        if matches!(env.payload, MessageType::JobCompleted(_)) {
            break;
        }
    }

    // Connection 2: resume from seq 0.
    let (s2, c2) = paired();
    let _h2 = runtime.serve_connection(s2);
    let mut resume = Envelope::new(MessageType::SessionResume(SessionResumePayload {
        resume_token: token.clone(),
        last_event_seq: 0,
    }));
    resume.session_id = Some(session_id.clone());
    c2.send(resume).await.expect("resume");

    let resumed = c2.recv().await.expect("recv").expect("present");
    let MessageType::SessionResumed(p) = resumed.payload else {
        panic!("expected session.resumed, got {:?}", resumed.payload);
    };
    assert_eq!(p.session_id, session_id);
    assert!(p.replayed, "buffered events should be replayed");
    assert_ne!(p.resume_token, token, "resume token must rotate");

    // Replayed events follow on the same connection; we should see the
    // job.completed replayed with seq > 0.
    let mut saw_replayed = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    while let Ok(Ok(Some(env))) = tokio::time::timeout_at(deadline, c2.recv()).await {
        if matches!(env.payload, MessageType::JobCompleted(_)) {
            assert!(env.event_seq.is_some_and(|s| s > 0));
            saw_replayed = true;
            break;
        }
    }
    assert!(
        saw_replayed,
        "expected a replayed job.completed after resume"
    );
}

/// §6.3 — a stale / unknown resume token is rejected with
/// `RESUME_WINDOW_EXPIRED`.
#[tokio::test]
async fn session_resume_with_stale_token_is_rejected() {
    let runtime = boot_runtime().await;
    let (s, c) = paired();
    let _h = runtime.serve_connection(s);
    let mut resume = Envelope::new(MessageType::SessionResume(SessionResumePayload {
        resume_token: "rt_does_not_exist".into(),
        last_event_seq: 0,
    }));
    resume.session_id = Some(SessionId::new());
    c.send(resume).await.expect("resume");
    let resp = tokio::time::timeout(Duration::from_secs(1), c.recv())
        .await
        .expect("timely")
        .expect("recv")
        .expect("present");
    let MessageType::SessionRejected(rej) = resp.payload else {
        panic!("expected session.rejected, got {:?}", resp.payload);
    };
    assert_eq!(rej.code, ErrorCode::ResumeWindowExpired);
}

/// §6.3 — a resume requesting a sequence beyond what was emitted is
/// uncovered and rejected with `RESUME_WINDOW_EXPIRED`.
#[tokio::test]
async fn session_resume_uncovered_sequence_is_rejected() {
    let runtime = boot_runtime().await;
    let (s, c) = paired();
    let _h = runtime.serve_connection(s);
    c.send(open_envelope()).await.expect("open");
    let accepted = c.recv().await.expect("recv").expect("present");
    let (session_id, token) = match accepted.payload {
        MessageType::SessionAccepted(p) => (p.session_id, p.resume_token.expect("token")),
        other => panic!("expected accepted, got {other:?}"),
    };

    let (s2, c2) = paired();
    let _h2 = runtime.serve_connection(s2);
    let mut resume = Envelope::new(MessageType::SessionResume(SessionResumePayload {
        resume_token: token,
        last_event_seq: 999_999,
    }));
    resume.session_id = Some(session_id);
    c2.send(resume).await.expect("resume");
    let resp = tokio::time::timeout(Duration::from_secs(1), c2.recv())
        .await
        .expect("timely")
        .expect("recv")
        .expect("present");
    let MessageType::SessionRejected(rej) = resp.payload else {
        panic!("expected session.rejected, got {:?}", resp.payload);
    };
    assert_eq!(rej.code, ErrorCode::ResumeWindowExpired);
}

#[tokio::test]
async fn list_after_rowid_returns_only_subsequent_events() {
    let log = EventLog::in_memory().await.expect("open");
    let session = SessionId::new();
    for _ in 0..5 {
        let mut env = Envelope::new(MessageType::Ping(PingPayload::default()));
        env.session_id = Some(session.clone());
        log.append(&env).await.expect("append");
    }

    let all = log.list(session.as_str(), 0, 100).await.expect("list");
    assert_eq!(all.len(), 5);

    let after_second = log
        .list(session.as_str(), all[1].rowid, 100)
        .await
        .expect("list");
    assert_eq!(after_second.len(), 3);
    assert_eq!(after_second[0].rowid, all[2].rowid);
}

#[tokio::test]
async fn resume_across_session_boundary_returns_only_owner_events() {
    let log = EventLog::in_memory().await.expect("open");
    let alice = SessionId::new();
    let bob = SessionId::new();

    for who in [&alice, &bob, &alice, &bob, &alice] {
        let mut env = Envelope::new(MessageType::Ping(PingPayload::default()));
        env.session_id = Some(who.clone());
        log.append(&env).await.expect("append");
    }

    let alice_only = log.list(alice.as_str(), 0, 100).await.expect("alice");
    assert_eq!(alice_only.len(), 3);
    let bob_only = log.list(bob.as_str(), 0, 100).await.expect("bob");
    assert_eq!(bob_only.len(), 2);
}
