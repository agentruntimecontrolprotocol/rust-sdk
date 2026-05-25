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

use arcp::envelope::Envelope;
use arcp::ids::SessionId;
use arcp::messages::{MessageType, PingPayload};
use arcp::store::eventlog::EventLog;

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
