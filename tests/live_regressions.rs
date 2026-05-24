//! Live regression tests for the issues filed from the Rust SDK review.
//!
//! These tests intentionally encode the desired behavior for open issues
//! #53-#60. They are not ignored; they should turn green as those issues are
//! fixed.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_panics_doc,
    clippy::too_many_lines,
    clippy::similar_names
)]

use std::sync::Arc;
use std::time::Duration;

use arcp::auth::BearerAuthenticator;
use arcp::envelope::Envelope;
use arcp::error::ARCPError;
use arcp::ids::{IdempotencyKey, JobId, SessionId};
use arcp::messages::{
    AuthScheme, CancelPayload, CancelTargetKind, Capabilities, ClientIdentity, CostBudget,
    CostBudgetAmount, Credentials, MessageType, SessionOpenPayload, ToolInvokePayload,
};
use arcp::runtime::context::{BudgetTracker, ToolContext};
use arcp::runtime::tools::{ToolHandler, ToolRegistryBuilder};
use arcp::runtime::ARCPRuntime;
use arcp::transport::{paired, MemoryTransport, Transport};
use arcp::ARCPClient;
use async_trait::async_trait;
use tokio::sync::mpsc;

#[derive(Clone)]
struct SlowTool {
    events: mpsc::UnboundedSender<&'static str>,
    delay: Duration,
}

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
        let _ = self.events.send("started");
        tokio::select! {
            () = ctx.cancel.cancelled() => {
                let _ = self.events.send("cancelled");
                Err(ARCPError::Cancelled { reason: "cancelled".into() })
            }
            () = tokio::time::sleep(self.delay) => {
                let _ = self.events.send("completed");
                Ok(arguments)
            }
        }
    }
}

#[derive(Clone)]
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

fn client_identity(kind: &str) -> ClientIdentity {
    ClientIdentity {
        kind: kind.into(),
        version: "0".into(),
        fingerprint: None,
        principal: None,
    }
}

async fn open_session(
    runtime: &ARCPRuntime,
    token: &str,
    kind: &str,
) -> (MemoryTransport, SessionId) {
    let (server_t, client_t) = paired();
    let _handle = runtime.serve_connection(server_t);

    let open = Envelope::new(MessageType::SessionOpen(SessionOpenPayload {
        auth: Credentials {
            scheme: AuthScheme::Bearer,
            token: Some(token.into()),
        },
        client: client_identity(kind),
        capabilities: Capabilities::default(),
    }));
    client_t.send(open).await.expect("send open");
    let accepted = tokio::time::timeout(Duration::from_secs(1), client_t.recv())
        .await
        .expect("handshake response")
        .expect("recv")
        .expect("envelope");
    let MessageType::SessionAccepted(payload) = accepted.payload else {
        panic!("expected session.accepted, got {:?}", accepted.payload);
    };
    (client_t, payload.session_id)
}

async fn recv_until<F>(transport: &MemoryTransport, mut predicate: F) -> Envelope
where
    F: FnMut(&Envelope) -> bool,
{
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    loop {
        let env = tokio::time::timeout_at(deadline, transport.recv())
            .await
            .expect("timely")
            .expect("recv")
            .expect("envelope");
        if predicate(&env) {
            return env;
        }
    }
}

async fn submit_slow_job(client: &MemoryTransport, session_id: &SessionId) -> JobId {
    let mut invoke = Envelope::new(MessageType::ToolInvoke(ToolInvokePayload::new(
        "slow",
        serde_json::json!({"ok": true}),
    )));
    invoke.session_id = Some(session_id.clone());
    client.send(invoke).await.expect("send invoke");

    let accepted = recv_until(client, |env| {
        matches!(env.payload, MessageType::JobAccepted(_))
    })
    .await;
    let MessageType::JobAccepted(payload) = accepted.payload else {
        unreachable!("predicate selected job.accepted");
    };
    payload.job_id
}

#[tokio::test]
async fn issue_53_cross_session_cancel_is_refused() {
    let (events_tx, _events_rx) = mpsc::unbounded_channel();
    let runtime = ARCPRuntime::builder()
        .with_authenticator(Box::new(
            BearerAuthenticator::new()
                .with_token("submitter-token", "submitter")
                .with_token("observer-token", "observer"),
        ))
        .with_tools(
            ToolRegistryBuilder::new()
                .with(Arc::new(SlowTool {
                    events: events_tx,
                    delay: Duration::from_secs(5),
                }))
                .build(),
        )
        .build()
        .await
        .expect("build");

    let (submitter, submitter_session) =
        open_session(&runtime, "submitter-token", "submitter").await;
    let (observer, observer_session) = open_session(&runtime, "observer-token", "observer").await;
    let job_id = submit_slow_job(&submitter, &submitter_session).await;

    let mut cancel = Envelope::new(MessageType::Cancel(CancelPayload {
        target: CancelTargetKind::Job,
        target_id: job_id.to_string(),
        reason: Some("not your job".into()),
        deadline_ms: None,
    }));
    cancel.session_id = Some(observer_session);
    observer.send(cancel).await.expect("send cancel");

    let response = recv_until(&observer, |env| {
        matches!(
            env.payload,
            MessageType::CancelAccepted(_) | MessageType::CancelRefused(_)
        )
    })
    .await;
    match response.payload {
        MessageType::CancelRefused(payload) => {
            assert!(
                payload.reason.contains("permission") || payload.reason.contains("authorized"),
                "unexpected refusal reason: {}",
                payload.reason
            );
        }
        other => panic!("cross-session cancel must be refused, got {other:?}"),
    }
}

#[tokio::test]
async fn issue_54_transport_drop_does_not_cancel_durable_job() {
    let (events_tx, mut events_rx) = mpsc::unbounded_channel();
    let runtime = ARCPRuntime::builder()
        .with_authenticator(Box::new(BearerAuthenticator::new().with_token("t", "p")))
        .with_tools(
            ToolRegistryBuilder::new()
                .with(Arc::new(SlowTool {
                    events: events_tx,
                    delay: Duration::from_millis(250),
                }))
                .build(),
        )
        .build()
        .await
        .expect("build");

    let (client, session_id) = open_session(&runtime, "t", "client").await;
    let _job_id = submit_slow_job(&client, &session_id).await;
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(1), events_rx.recv())
            .await
            .expect("tool started"),
        Some("started")
    );

    drop(client);

    let outcome = tokio::time::timeout(Duration::from_secs(1), events_rx.recv())
        .await
        .expect("tool should finish or cancel")
        .expect("tool outcome");
    assert_eq!(outcome, "completed", "transport drop must not cancel jobs");
}

#[tokio::test]
async fn issue_55_countable_events_carry_event_seq() {
    let (events_tx, _events_rx) = mpsc::unbounded_channel();
    let runtime = ARCPRuntime::builder()
        .with_authenticator(Box::new(BearerAuthenticator::new().with_token("t", "p")))
        .with_tools(
            ToolRegistryBuilder::new()
                .with(Arc::new(SlowTool {
                    events: events_tx,
                    delay: Duration::from_secs(1),
                }))
                .build(),
        )
        .build()
        .await
        .expect("build");
    let (client, session_id) = open_session(&runtime, "t", "client").await;
    let _job_id = submit_slow_job(&client, &session_id).await;

    let accepted = recv_until(&client, |env| {
        matches!(env.payload, MessageType::JobStarted(_))
    })
    .await;
    let wire = serde_json::to_value(&accepted).expect("serialize event");
    let seq = wire
        .get("event_seq")
        .and_then(serde_json::Value::as_u64)
        .expect("countable job events must carry event_seq");
    assert!(seq > 0);
}

#[tokio::test]
async fn issue_56_invoke_returns_error_when_runtime_fails_before_job_accepted() {
    let runtime = ARCPRuntime::builder()
        .with_authenticator(Box::new(BearerAuthenticator::new().with_token("t", "p")))
        .build()
        .await
        .expect("build");
    let (server_t, client_t) = paired();
    let _handle = runtime.serve_connection(server_t);
    let session = ARCPClient::new(client_t)
        .open()
        .expect("open")
        .authenticate(
            Credentials {
                scheme: AuthScheme::Bearer,
                token: Some("t".into()),
            },
            client_identity("client"),
            Capabilities::default(),
        )
        .await
        .expect("auth");

    let result = tokio::time::timeout(
        Duration::from_millis(500),
        session.invoke("bad@", serde_json::json!({})),
    )
    .await
    .expect("invoke should resolve instead of hanging");
    assert!(
        result.is_err(),
        "invalid agent reference should be an error"
    );
}

#[tokio::test]
async fn issue_57_ack_window_shutdown_does_not_hang_writer() {
    let runtime = ARCPRuntime::builder()
        .with_authenticator(Box::new(BearerAuthenticator::new().with_token("t", "p")))
        .with_tools(ToolRegistryBuilder::new().with(Arc::new(EchoTool)).build())
        .with_ack_window(1)
        .build()
        .await
        .expect("build");
    let (server_t, client_t) = paired();
    let handle = runtime.serve_connection(server_t);

    let open = Envelope::new(MessageType::SessionOpen(SessionOpenPayload {
        auth: Credentials {
            scheme: AuthScheme::Bearer,
            token: Some("t".into()),
        },
        client: client_identity("client"),
        capabilities: Capabilities::default(),
    }));
    client_t.send(open).await.expect("send open");
    let accepted = client_t.recv().await.expect("recv").expect("accepted");
    let MessageType::SessionAccepted(payload) = accepted.payload else {
        panic!("expected session.accepted");
    };

    let mut invoke = Envelope::new(MessageType::ToolInvoke(ToolInvokePayload::new(
        "echo",
        serde_json::json!({"hello": "world"}),
    )));
    invoke.session_id = Some(payload.session_id);
    client_t.send(invoke).await.expect("send invoke");
    let first = recv_until(&client_t, |env| {
        matches!(env.payload, MessageType::JobAccepted(_))
    })
    .await;
    assert!(matches!(first.payload, MessageType::JobAccepted(_)));

    drop(client_t);
    tokio::time::timeout(Duration::from_secs(1), handle)
        .await
        .expect("connection task should exit when peer drops")
        .expect("join connection task");
}

#[test]
fn issue_58_budget_tracker_rejects_the_charge_that_overspends() {
    let budget = CostBudget {
        amounts: vec![CostBudgetAmount {
            currency: "USD".into(),
            amount: 1.0,
        }],
    };
    let tracker = BudgetTracker::from_budget(&budget);
    let err = tracker
        .charge("USD", 100.0)
        .expect_err("oversized charge should fail immediately");
    assert!(matches!(err, ARCPError::BudgetExhausted { .. }));
}

#[tokio::test]
async fn issue_59_idempotency_key_replays_existing_job_ack() {
    let (events_tx, _events_rx) = mpsc::unbounded_channel();
    let runtime = ARCPRuntime::builder()
        .with_authenticator(Box::new(BearerAuthenticator::new().with_token("t", "p")))
        .with_tools(
            ToolRegistryBuilder::new()
                .with(Arc::new(SlowTool {
                    events: events_tx,
                    delay: Duration::from_secs(2),
                }))
                .build(),
        )
        .build()
        .await
        .expect("build");
    let (client, session_id) = open_session(&runtime, "t", "client").await;
    let key = IdempotencyKey::new("same-logical-command").expect("non-empty key");

    let mut first = Envelope::new(MessageType::ToolInvoke(ToolInvokePayload::new(
        "slow",
        serde_json::json!({"input": 1}),
    )));
    first.session_id = Some(session_id.clone());
    first.idempotency_key = Some(key.clone());
    client.send(first).await.expect("send first");
    let first_ack = recv_until(&client, |env| {
        matches!(env.payload, MessageType::JobAccepted(_))
    })
    .await;
    let MessageType::JobAccepted(first_payload) = first_ack.payload else {
        unreachable!("predicate selected job.accepted");
    };

    let mut retry = Envelope::new(MessageType::ToolInvoke(ToolInvokePayload::new(
        "slow",
        serde_json::json!({"input": 1}),
    )));
    retry.session_id = Some(session_id);
    retry.idempotency_key = Some(key);
    client.send(retry).await.expect("send retry");
    let retry_ack = recv_until(&client, |env| {
        matches!(env.payload, MessageType::JobAccepted(_))
    })
    .await;
    let MessageType::JobAccepted(retry_payload) = retry_ack.payload else {
        unreachable!("predicate selected job.accepted");
    };

    assert_eq!(
        retry_payload.job_id, first_payload.job_id,
        "same idempotency key should replay the original job acknowledgement"
    );
}

#[test]
fn issue_60_readme_event_seq_reference_resolves_to_real_envelope_field() {
    // The README walkthrough reads `env.event_seq` to track the highest
    // session-scoped sequence across reconnects (§6.5 / §6.6). That
    // field MUST resolve to a real Option<u64> on Envelope so the
    // documented usage actually compiles.
    let env = Envelope::new(MessageType::SessionAck(arcp::messages::SessionAckPayload {
        last_processed_seq: 0,
    }));
    // Compile-time evidence that the documented API matches the type:
    // if event_seq is removed or renamed, this stops compiling and the
    // regression breaks at build, not in production.
    let _: Option<u64> = env.event_seq;

    let readme = include_str!("../README.md");
    assert!(
        readme.contains("env.event_seq"),
        "README walkthrough should keep documenting Envelope::event_seq now that the field exists"
    );

    // The getting-started example must not point users at binaries
    // whose top-level path still contains `todo!()` placeholders.
    let getting_started = include_str!("../docs/getting-started.md");
    for binary in ["submit_and_stream", "resumability"] {
        let header = format!("cargo run --example {binary}");
        if getting_started.contains(&header) {
            let path = format!("examples/{binary}.rs");
            let direct = std::fs::read_to_string(&path).ok();
            let nested = std::fs::read_to_string(format!("examples/{binary}/main.rs")).ok();
            let body = direct.or(nested).unwrap_or_default();
            assert!(
                !body.contains("todo!()"),
                "getting-started.md tells users to run `{header}` but {binary} still has todo!() in the top-level path",
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Issue #61: focused regression tests for the low-coverage critical paths.
// These mirror the gap list in the audit (client/api.rs pending cleanup
// across artifact + subscribe entry points, runtime authorization for
// the positive cancel path, ack-window normalization, and idempotency
// across connections) and intentionally exercise edge cases the live
// regressions above did not cover.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn issue_61_put_artifact_returns_unavailable_when_runtime_drops() {
    let runtime = ARCPRuntime::builder()
        .with_authenticator(Box::new(BearerAuthenticator::new().with_token("t", "p")))
        .build()
        .await
        .expect("build");
    let (server_t, client_t) = paired();
    let handle = runtime.serve_connection(server_t);
    let session = ARCPClient::new(client_t)
        .open()
        .expect("open")
        .authenticate(
            Credentials {
                scheme: AuthScheme::Bearer,
                token: Some("t".into()),
            },
            client_identity("client"),
            Capabilities::default(),
        )
        .await
        .expect("auth");

    // Drop the server task so the runtime side of the transport closes.
    handle.abort();
    let _ = handle.await;

    let result = tokio::time::timeout(
        Duration::from_secs(1),
        session.put_artifact("text/plain", "aGVsbG8=", None),
    )
    .await
    .expect("put_artifact must resolve instead of hanging");
    assert!(result.is_err(), "put_artifact on dead transport must error");
}

#[tokio::test]
async fn issue_61_subscribe_returns_unavailable_when_runtime_drops() {
    let runtime = ARCPRuntime::builder()
        .with_authenticator(Box::new(BearerAuthenticator::new().with_token("t", "p")))
        .build()
        .await
        .expect("build");
    let (server_t, client_t) = paired();
    let handle = runtime.serve_connection(server_t);
    let session = ARCPClient::new(client_t)
        .open()
        .expect("open")
        .authenticate(
            Credentials {
                scheme: AuthScheme::Bearer,
                token: Some("t".into()),
            },
            client_identity("client"),
            Capabilities::default(),
        )
        .await
        .expect("auth");
    handle.abort();
    let _ = handle.await;

    let result = tokio::time::timeout(
        Duration::from_secs(1),
        session.subscribe(arcp::messages::SubscriptionFilter::default()),
    )
    .await
    .expect("subscribe must resolve instead of hanging");
    assert!(result.is_err(), "subscribe on dead transport must error");
}

#[tokio::test]
async fn issue_61_owning_session_can_cancel_its_own_job() {
    let (events_tx, mut events_rx) = mpsc::unbounded_channel();
    let runtime = ARCPRuntime::builder()
        .with_authenticator(Box::new(BearerAuthenticator::new().with_token("t", "p")))
        .with_tools(
            ToolRegistryBuilder::new()
                .with(Arc::new(SlowTool {
                    events: events_tx,
                    delay: Duration::from_secs(5),
                }))
                .build(),
        )
        .build()
        .await
        .expect("build");
    let (client, session_id) = open_session(&runtime, "t", "client").await;
    let job_id = submit_slow_job(&client, &session_id).await;
    // Wait until the tool actually starts so cancel isn't racing
    // job.accepted.
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(1), events_rx.recv())
            .await
            .expect("started"),
        Some("started")
    );

    let mut cancel = Envelope::new(MessageType::Cancel(CancelPayload {
        target: CancelTargetKind::Job,
        target_id: job_id.to_string(),
        reason: Some("user".into()),
        deadline_ms: None,
    }));
    cancel.session_id = Some(session_id);
    client.send(cancel).await.expect("send cancel");

    let response = recv_until(&client, |env| {
        matches!(
            env.payload,
            MessageType::CancelAccepted(_) | MessageType::CancelRefused(_)
        )
    })
    .await;
    assert!(
        matches!(response.payload, MessageType::CancelAccepted(_)),
        "owning session's cancel must be accepted, got {:?}",
        response.payload
    );
    // The runtime's per-job task races the cancel token against the
    // handler future, so the tool's own "cancelled" event isn't
    // guaranteed; the wire-level job.cancelled IS — it's the canonical
    // terminal signal we promise callers.
    let terminal = tokio::time::timeout(
        Duration::from_secs(1),
        recv_until(&client, |env| {
            matches!(env.payload, MessageType::JobCancelled(_))
        }),
    )
    .await
    .expect("job.cancelled must follow cancel.accepted within deadline");
    assert!(matches!(terminal.payload, MessageType::JobCancelled(_)));
    // events_rx may or may not carry the tool-internal "cancelled" tag;
    // either is acceptable, so drain without asserting.
    drop(events_rx);
}

#[tokio::test]
async fn issue_61_ack_window_zero_is_normalized_and_does_not_deadlock() {
    // with_ack_window(0) used to install an unsatisfiable gate so the
    // very first countable event hung the writer. The builder must
    // normalize 0 to "disabled" instead.
    let runtime = ARCPRuntime::builder()
        .with_authenticator(Box::new(BearerAuthenticator::new().with_token("t", "p")))
        .with_tools(ToolRegistryBuilder::new().with(Arc::new(EchoTool)).build())
        .with_ack_window(0)
        .build()
        .await
        .expect("build");
    let (client, session_id) = open_session(&runtime, "t", "client").await;
    let mut invoke = Envelope::new(MessageType::ToolInvoke(ToolInvokePayload::new(
        "echo",
        serde_json::json!({"ok": true}),
    )));
    invoke.session_id = Some(session_id);
    client.send(invoke).await.expect("send invoke");
    let completed = tokio::time::timeout(
        Duration::from_secs(2),
        recv_until(&client, |env| {
            matches!(env.payload, MessageType::JobCompleted(_))
        }),
    )
    .await
    .expect("ack_window(0) must not deadlock the writer");
    assert!(matches!(completed.payload, MessageType::JobCompleted(_)));
}

#[tokio::test]
async fn issue_61_idempotency_key_replays_across_independent_connections() {
    // Retrying the same logical command after a reconnect under the
    // same principal must resolve to the original job (§6.4) rather
    // than spawning a duplicate.
    let runtime = ARCPRuntime::builder()
        .with_authenticator(Box::new(
            BearerAuthenticator::new().with_token("t", "alice"),
        ))
        .with_tools(ToolRegistryBuilder::new().with(Arc::new(EchoTool)).build())
        .build()
        .await
        .expect("build");
    let (first, first_session) = open_session(&runtime, "t", "client").await;
    let key = IdempotencyKey::new("xact-92").expect("non-empty key");

    let mut first_invoke = Envelope::new(MessageType::ToolInvoke(ToolInvokePayload::new(
        "echo",
        serde_json::json!({"once": true}),
    )));
    first_invoke.session_id = Some(first_session);
    first_invoke.idempotency_key = Some(key.clone());
    first.send(first_invoke).await.expect("send first invoke");
    let first_ack = recv_until(&first, |env| {
        matches!(env.payload, MessageType::JobAccepted(_))
    })
    .await;
    let MessageType::JobAccepted(first_payload) = first_ack.payload else {
        unreachable!("predicate selected job.accepted");
    };
    drop(first);

    // Open a fresh connection (same principal) and replay.
    let (second, second_session) = open_session(&runtime, "t", "client").await;
    let mut retry = Envelope::new(MessageType::ToolInvoke(ToolInvokePayload::new(
        "echo",
        serde_json::json!({"once": true}),
    )));
    retry.session_id = Some(second_session);
    retry.idempotency_key = Some(key);
    second.send(retry).await.expect("send retry");
    let retry_ack = recv_until(&second, |env| {
        matches!(env.payload, MessageType::JobAccepted(_))
    })
    .await;
    let MessageType::JobAccepted(retry_payload) = retry_ack.payload else {
        unreachable!("predicate selected job.accepted");
    };
    assert_eq!(
        retry_payload.job_id, first_payload.job_id,
        "idempotency key must replay across connections under the same principal"
    );
}

#[tokio::test]
async fn issue_61_idempotency_key_with_conflicting_payload_is_refused() {
    // Same principal + same key + DIFFERENT command intent must be
    // refused rather than transparently rewriting the prior job.
    let runtime = ARCPRuntime::builder()
        .with_authenticator(Box::new(
            BearerAuthenticator::new().with_token("t", "alice"),
        ))
        .with_tools(ToolRegistryBuilder::new().with(Arc::new(EchoTool)).build())
        .build()
        .await
        .expect("build");
    let (client, session_id) = open_session(&runtime, "t", "client").await;
    let key = IdempotencyKey::new("xact-93").expect("non-empty key");

    let mut first = Envelope::new(MessageType::ToolInvoke(ToolInvokePayload::new(
        "echo",
        serde_json::json!({"value": 1}),
    )));
    first.session_id = Some(session_id.clone());
    first.idempotency_key = Some(key.clone());
    client.send(first).await.expect("send first");
    let _accepted = recv_until(&client, |env| {
        matches!(env.payload, MessageType::JobAccepted(_))
    })
    .await;

    let mut conflict = Envelope::new(MessageType::ToolInvoke(ToolInvokePayload::new(
        "echo",
        serde_json::json!({"value": 2}),
    )));
    conflict.session_id = Some(session_id);
    conflict.idempotency_key = Some(key);
    client.send(conflict).await.expect("send conflict");
    let response = recv_until(&client, |env| {
        matches!(
            env.payload,
            MessageType::JobFailed(_) | MessageType::JobAccepted(_)
        )
    })
    .await;
    let MessageType::JobFailed(failed) = response.payload else {
        panic!(
            "conflicting idempotency payload must be refused, got {:?}",
            response.payload
        );
    };
    assert_eq!(failed.code, arcp::error::ErrorCode::FailedPrecondition);
}

#[test]
fn issue_61_envelope_event_seq_is_omitted_when_unset() {
    // Locally constructed envelopes (i.e. anything not yet emitted by a
    // runtime writer) must not carry a synthetic event_seq, otherwise
    // wire-level equality and replay diffing break.
    let env = Envelope::new(MessageType::SessionAck(arcp::messages::SessionAckPayload {
        last_processed_seq: 0,
    }));
    let wire = serde_json::to_value(&env).expect("serialize");
    assert!(
        wire.get("event_seq").is_none(),
        "event_seq must be elided when unstamped, got {wire}"
    );
}
