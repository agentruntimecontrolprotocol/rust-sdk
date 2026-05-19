//! Integration tests for `job.subscribe` / `job.subscribed` /
//! `job.unsubscribe` dispatch (ARCP v1.1 §7.6).

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
    AuthScheme, Capabilities, ClientIdentity, Credentials, JobSubscribePayload,
    JobUnsubscribePayload, MessageType, SessionOpenPayload, ToolInvokePayload,
};
use arcp::runtime::context::ToolContext;
use arcp::runtime::tools::{ToolHandler, ToolRegistryBuilder};
use arcp::runtime::ARCPRuntime;
use arcp::transport::{paired, Transport};
use async_trait::async_trait;

struct SlowEchoTool;

#[async_trait]
impl ToolHandler for SlowEchoTool {
    fn name(&self) -> &'static str {
        "slow-echo"
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

async fn open_session(
    runtime: &ARCPRuntime,
    kind: &'static str,
    principal_token: &'static str,
) -> (arcp::transport::MemoryTransport, arcp::ids::SessionId) {
    let (server_t, client_t) = paired();
    let _h = runtime.serve_connection(server_t);

    let mut open = Envelope::new(MessageType::SessionOpen(SessionOpenPayload {
        auth: Credentials {
            scheme: AuthScheme::Bearer,
            token: Some(principal_token.into()),
        },
        client: ClientIdentity {
            kind: kind.into(),
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
        .expect("session.accepted");
    let MessageType::SessionAccepted(payload) = accepted.payload else {
        panic!("expected session.accepted, got {:?}", accepted.payload);
    };
    (client_t, payload.session_id)
}

#[tokio::test]
async fn cross_session_subscribe_receives_terminal_event() {
    let runtime = ARCPRuntime::builder()
        .with_authenticator(Box::new(
            BearerAuthenticator::new().with_token("token-A", "shared-principal"),
        ))
        .with_tools(
            ToolRegistryBuilder::new()
                .with(Arc::new(SlowEchoTool))
                .build(),
        )
        .build()
        .await
        .expect("build");

    // Both sessions auth as `shared-principal`.
    let (submitter, sub_session) = open_session(&runtime, "submitter", "token-A").await;
    let (observer, obs_session) = open_session(&runtime, "observer", "token-A").await;

    // Submitter submits a long-ish job.
    let mut invoke = Envelope::new(MessageType::ToolInvoke(ToolInvokePayload {
        tool: "slow-echo".into(),
        arguments: serde_json::json!({"hello": "world"}),
    }));
    invoke.session_id = Some(sub_session.clone());
    submitter.send(invoke).await.expect("send invoke");
    let accepted = submitter.recv().await.expect("recv").expect("envelope");
    let MessageType::JobAccepted(accepted) = accepted.payload else {
        panic!("expected job.accepted");
    };
    let job_id = accepted.job_id;

    // Observer subscribes to the job.
    let mut sub = Envelope::new(MessageType::JobSubscribe(JobSubscribePayload {
        job_id: job_id.clone(),
        from_event_seq: None,
        history: false,
    }));
    sub.session_id = Some(obs_session.clone());
    observer.send(sub).await.expect("send subscribe");

    // Observer should receive job.subscribed.
    let envelope = tokio::time::timeout(Duration::from_secs(1), observer.recv())
        .await
        .expect("timely")
        .expect("recv")
        .expect("envelope");
    let MessageType::JobSubscribed(ack) = envelope.payload else {
        panic!("expected job.subscribed, got {:?}", envelope.payload);
    };
    assert_eq!(ack.job_id, job_id);
    assert!(matches!(
        ack.current_status.as_str(),
        "running" | "accepted"
    ));

    // The observer should now see job.completed forwarded for the
    // submitter's job (with session_id rewritten to the observer's
    // session).
    let mut got_completed = false;
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while std::time::Instant::now() < deadline {
        if let Ok(Ok(Some(env))) =
            tokio::time::timeout(Duration::from_millis(300), observer.recv()).await
        {
            if let MessageType::JobCompleted(_) = env.payload {
                got_completed = true;
                assert_eq!(env.session_id.as_ref(), Some(&obs_session));
                break;
            }
        }
    }
    assert!(got_completed, "observer did not see job.completed forward");
}

#[tokio::test]
async fn job_subscribe_for_unknown_job_returns_not_found_nack() {
    let runtime = ARCPRuntime::builder()
        .with_authenticator(Box::new(BearerAuthenticator::new().with_token("t", "p")))
        .build()
        .await
        .expect("build");
    let (client, session) = open_session(&runtime, "observer", "t").await;

    let mut sub = Envelope::new(MessageType::JobSubscribe(JobSubscribePayload {
        job_id: arcp::ids::JobId::new(),
        from_event_seq: None,
        history: false,
    }));
    sub.session_id = Some(session);
    client.send(sub).await.expect("send");

    let envelope = tokio::time::timeout(Duration::from_secs(1), client.recv())
        .await
        .expect("timely")
        .expect("recv")
        .expect("envelope");
    let MessageType::Nack(nack) = envelope.payload else {
        panic!("expected nack, got {:?}", envelope.payload);
    };
    assert_eq!(nack.code, arcp::error::ErrorCode::NotFound);
}

#[tokio::test]
async fn job_subscribe_denied_for_different_principal() {
    let runtime = ARCPRuntime::builder()
        .with_authenticator(Box::new(
            BearerAuthenticator::new()
                .with_token("token-A", "principal-A")
                .with_token("token-B", "principal-B"),
        ))
        .with_tools(
            ToolRegistryBuilder::new()
                .with(Arc::new(SlowEchoTool))
                .build(),
        )
        .build()
        .await
        .expect("build");
    let (submitter, sub_session) = open_session(&runtime, "submitter", "token-A").await;
    let (observer, obs_session) = open_session(&runtime, "observer", "token-B").await;

    let mut invoke = Envelope::new(MessageType::ToolInvoke(ToolInvokePayload {
        tool: "slow-echo".into(),
        arguments: serde_json::json!({}),
    }));
    invoke.session_id = Some(sub_session.clone());
    submitter.send(invoke).await.expect("send");
    let accepted = submitter.recv().await.expect("recv").expect("envelope");
    let MessageType::JobAccepted(accepted) = accepted.payload else {
        panic!("expected job.accepted");
    };
    let job_id = accepted.job_id;

    let mut sub = Envelope::new(MessageType::JobSubscribe(JobSubscribePayload {
        job_id,
        from_event_seq: None,
        history: false,
    }));
    sub.session_id = Some(obs_session);
    observer.send(sub).await.expect("send");

    let envelope = tokio::time::timeout(Duration::from_secs(1), observer.recv())
        .await
        .expect("timely")
        .expect("recv")
        .expect("envelope");
    let MessageType::Nack(nack) = envelope.payload else {
        panic!("expected nack, got {:?}", envelope.payload);
    };
    assert_eq!(nack.code, arcp::error::ErrorCode::PermissionDenied);
}

#[tokio::test]
async fn job_unsubscribe_stops_forwarding() {
    let runtime = ARCPRuntime::builder()
        .with_authenticator(Box::new(BearerAuthenticator::new().with_token("t", "p")))
        .with_tools(
            ToolRegistryBuilder::new()
                .with(Arc::new(SlowEchoTool))
                .build(),
        )
        .build()
        .await
        .expect("build");
    let (submitter, sub_session) = open_session(&runtime, "submitter", "t").await;
    let (observer, obs_session) = open_session(&runtime, "observer", "t").await;

    let mut invoke = Envelope::new(MessageType::ToolInvoke(ToolInvokePayload {
        tool: "slow-echo".into(),
        arguments: serde_json::json!({}),
    }));
    invoke.session_id = Some(sub_session.clone());
    submitter.send(invoke).await.expect("send");
    let accepted = submitter.recv().await.expect("recv").expect("envelope");
    let MessageType::JobAccepted(accepted) = accepted.payload else {
        panic!("expected job.accepted");
    };
    let job_id = accepted.job_id;

    let mut sub = Envelope::new(MessageType::JobSubscribe(JobSubscribePayload {
        job_id: job_id.clone(),
        from_event_seq: None,
        history: false,
    }));
    sub.session_id = Some(obs_session.clone());
    observer.send(sub).await.expect("send");
    let ack = observer.recv().await.expect("recv").expect("envelope");
    assert!(matches!(ack.payload, MessageType::JobSubscribed(_)));

    // Unsubscribe immediately.
    let mut unsub = Envelope::new(MessageType::JobUnsubscribe(JobUnsubscribePayload {
        job_id,
    }));
    unsub.session_id = Some(obs_session);
    observer.send(unsub).await.expect("send");

    // After unsubscribing, the observer should not receive job.completed.
    let mut saw_completed = false;
    let deadline = std::time::Instant::now() + Duration::from_millis(500);
    while std::time::Instant::now() < deadline {
        if let Ok(Ok(Some(env))) =
            tokio::time::timeout(Duration::from_millis(200), observer.recv()).await
        {
            if let MessageType::JobCompleted(_) = env.payload {
                saw_completed = true;
                break;
            }
        }
    }
    assert!(
        !saw_completed,
        "observer received job.completed after unsubscribe"
    );
}
