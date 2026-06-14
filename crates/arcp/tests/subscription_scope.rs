//! Integration tests for ARCP v1.1 §14 subscription scoping: a generic
//! `subscribe` defaults to same-principal scope and must reject filters
//! that name another principal's session with `PERMISSION_DENIED`.

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
    AuthScheme, Capabilities, ClientIdentity, Credentials, MessageType, SubscribePayload,
    SubscriptionFilter, ToolInvokePayload,
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

async fn build_runtime() -> ARCPRuntime {
    ARCPRuntime::builder()
        .with_authenticator(Box::new(
            BearerAuthenticator::new()
                .with_token("token-A", "alice")
                .with_token("token-B", "bob"),
        ))
        .with_capabilities(Capabilities {
            subscriptions: Some(true),
            ..Default::default()
        })
        .with_tools(ToolRegistryBuilder::new().with(Arc::new(EchoTool)).build())
        .build()
        .await
        .expect("build")
}

async fn open(runtime: &ARCPRuntime, token: &str) -> (MemoryTransport, arcp::ids::SessionId) {
    let (server_t, client_t) = paired();
    let _h = runtime.serve_connection(server_t);
    let mut open = Envelope::new(MessageType::SessionOpen(
        arcp::messages::SessionOpenPayload {
            auth: Credentials {
                scheme: AuthScheme::Bearer,
                token: Some(token.into()),
            },
            client: ClientIdentity {
                kind: "test".into(),
                version: "0".into(),
                fingerprint: None,
                principal: None,
            },
            capabilities: Capabilities {
                subscriptions: Some(true),
                ..Default::default()
            },
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

/// §14 — a default (unscoped) subscribe from alice must not receive events
/// from bob's session (a different principal).
#[tokio::test]
async fn default_subscribe_does_not_leak_other_principal_events() {
    let runtime = build_runtime().await;
    let (alice_t, alice_session) = open(&runtime, "token-A").await;
    let (bob_t, bob_session) = open(&runtime, "token-B").await;
    let bob_session_str = bob_session.to_string();

    // Alice subscribes with a default (empty) filter.
    let mut sub = Envelope::new(MessageType::Subscribe(SubscribePayload {
        filter: SubscriptionFilter::default(),
        since: None,
    }));
    sub.session_id = Some(alice_session);
    alice_t.send(sub).await.expect("send subscribe");
    let accepted = alice_t.recv().await.expect("recv").expect("present");
    assert!(matches!(
        accepted.payload,
        MessageType::SubscribeAccepted(_)
    ));

    // Bob runs a job that emits a terminal event.
    let mut invoke = Envelope::new(MessageType::ToolInvoke(ToolInvokePayload::new(
        "echo",
        serde_json::json!({"v": 1}),
    )));
    invoke.session_id = Some(bob_session);
    bob_t.send(invoke).await.expect("send invoke");

    // Alice may receive her own session's events (e.g. subscribe.accepted
    // echoed), but never a subscribe.event whose inner event belongs to
    // bob's session.
    let deadline = tokio::time::Instant::now() + Duration::from_millis(600);
    while let Ok(Ok(Some(env))) = tokio::time::timeout_at(deadline, alice_t.recv()).await {
        if let MessageType::SubscribeEvent(payload) = env.payload {
            let inner_session = payload
                .event
                .get("session_id")
                .and_then(serde_json::Value::as_str);
            assert_ne!(
                inner_session,
                Some(bob_session_str.as_str()),
                "alice leaked a subscribe.event from another principal's session"
            );
        }
    }
}

/// §14 — a subscribe whose filter names another principal's session is
/// rejected with `PERMISSION_DENIED`.
#[tokio::test]
async fn subscribe_filter_for_other_principal_is_denied() {
    let runtime = build_runtime().await;
    let (alice_t, alice_session) = open(&runtime, "token-A").await;
    let (_bob_t, bob_session) = open(&runtime, "token-B").await;

    let mut sub = Envelope::new(MessageType::Subscribe(SubscribePayload {
        filter: SubscriptionFilter {
            session_id: vec![bob_session],
            ..SubscriptionFilter::default()
        },
        since: None,
    }));
    sub.session_id = Some(alice_session);
    alice_t.send(sub).await.expect("send subscribe");

    let resp = tokio::time::timeout(Duration::from_secs(1), alice_t.recv())
        .await
        .expect("timely")
        .expect("recv")
        .expect("present");
    let MessageType::Nack(nack) = resp.payload else {
        panic!("expected PERMISSION_DENIED nack, got {:?}", resp.payload);
    };
    assert_eq!(nack.code, ErrorCode::PermissionDenied);
}

/// §14 — a subscriber still sees events from another session owned by the
/// SAME principal (the spec default is same-principal, not same-session).
#[tokio::test]
async fn same_principal_cross_session_subscribe_is_allowed() {
    let runtime = build_runtime().await;
    let (sub_t, sub_session) = open(&runtime, "token-A").await;
    let (cmd_t, cmd_session) = open(&runtime, "token-A").await;

    let mut sub = Envelope::new(MessageType::Subscribe(SubscribePayload {
        filter: SubscriptionFilter {
            types: vec!["job.completed".into()],
            ..SubscriptionFilter::default()
        },
        since: None,
    }));
    sub.session_id = Some(sub_session);
    sub_t.send(sub).await.expect("send subscribe");
    let accepted = sub_t.recv().await.expect("recv").expect("present");
    assert!(matches!(
        accepted.payload,
        MessageType::SubscribeAccepted(_)
    ));

    let mut invoke = Envelope::new(MessageType::ToolInvoke(ToolInvokePayload::new(
        "echo",
        serde_json::json!({"v": 1}),
    )));
    invoke.session_id = Some(cmd_session);
    cmd_t.send(invoke).await.expect("send invoke");

    // The subscriber should see a subscribe.event for the other
    // same-principal session's job.completed.
    let mut saw_event = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    while tokio::time::Instant::now() < deadline {
        if let Ok(Ok(Some(env))) =
            tokio::time::timeout(Duration::from_millis(300), sub_t.recv()).await
        {
            if matches!(env.payload, MessageType::SubscribeEvent(_)) {
                saw_event = true;
                break;
            }
        }
    }
    assert!(
        saw_event,
        "same-principal subscriber should receive the other session's job.completed"
    );
}
