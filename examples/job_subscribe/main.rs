//! ARCP v1.1 §7.6 — `job.subscribe` cross-session demo.
//!
//! Two clients share the same principal:
//!
//!   Submitter (A) — invokes a slow `timer` job.
//!   Observer  (B) — discovers the job via `session.list_jobs`, then
//!                   subscribes to its event stream and prints each
//!                   event until the job terminates.
//!
//! Run with:
//!     `cargo run --example job_subscribe`

#![allow(clippy::similar_names, clippy::expect_used, clippy::print_stdout)]

use std::sync::Arc;
use std::time::Duration;

use arcp::auth::BearerAuthenticator;
use arcp::envelope::Envelope;
use arcp::error::ARCPError;
use arcp::messages::{
    AuthScheme, Capabilities, ClientIdentity, Credentials, JobSubscribePayload,
    JobUnsubscribePayload, MessageType, SessionListJobsPayload, SessionOpenPayload,
    ToolInvokePayload,
};
use arcp::runtime::context::ToolContext;
use arcp::runtime::tools::{ToolHandler, ToolRegistryBuilder};
use arcp::runtime::ARCPRuntime;
use arcp::transport::{paired, MemoryTransport, Transport};
use async_trait::async_trait;

struct TimerTool;

#[async_trait]
impl ToolHandler for TimerTool {
    fn name(&self) -> &'static str {
        "timer"
    }

    async fn invoke(
        &self,
        _arguments: serde_json::Value,
        ctx: ToolContext,
    ) -> Result<serde_json::Value, ARCPError> {
        tokio::select! {
            () = ctx.cancel.cancelled() => Err(ARCPError::Cancelled { reason: "cancelled".into() }),
            () = tokio::time::sleep(Duration::from_millis(400)) => Ok(serde_json::json!({"ticks": 4})),
        }
    }
}

async fn open_session(
    runtime: &ARCPRuntime,
    kind: &'static str,
) -> Result<(MemoryTransport, arcp::ids::SessionId), Box<dyn std::error::Error>> {
    let (server_t, client_t) = paired();
    let _h = runtime.serve_connection(server_t);

    let mut open = Envelope::new(MessageType::SessionOpen(SessionOpenPayload {
        auth: Credentials {
            scheme: AuthScheme::Bearer,
            token: Some("demo-token".into()),
        },
        client: ClientIdentity {
            kind: kind.into(),
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
    Ok((client_t, payload.session_id))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let runtime = ARCPRuntime::builder()
        .with_authenticator(Box::new(
            BearerAuthenticator::new().with_token("demo-token", "demo-principal"),
        ))
        .with_tools(ToolRegistryBuilder::new().with(Arc::new(TimerTool)).build())
        .build()
        .await?;

    let (a, a_session) = open_session(&runtime, "submitter").await?;
    println!("[A] connected as submitter");

    // Submitter invokes a slow job.
    let mut invoke = Envelope::new(MessageType::ToolInvoke(ToolInvokePayload {
        tool: "timer".into(),
        arguments: serde_json::json!({}),
    }));
    invoke.session_id = Some(a_session.clone());
    a.send(invoke).await?;
    let accepted_env = a.recv().await?.ok_or("no job.accepted")?;
    let MessageType::JobAccepted(accepted) = accepted_env.payload else {
        return Err("expected job.accepted".into());
    };
    let job_id = accepted.job_id;
    println!("[A] submitted job_id={job_id}");

    // Observer connects — same principal, so authorization passes.
    let (b, b_session) = open_session(&runtime, "observer").await?;
    println!("[B] connected as observer");

    // B sees the job via session.list_jobs (same principal; visible).
    let mut list = Envelope::new(MessageType::SessionListJobs(
        SessionListJobsPayload::default(),
    ));
    list.session_id = Some(b_session.clone());
    b.send(list).await?;
    // Drain until we get session.jobs (skip own session.* events).
    loop {
        let env = tokio::time::timeout(Duration::from_secs(1), b.recv())
            .await??
            .ok_or("transport closed")?;
        if let MessageType::SessionJobs(jobs) = env.payload {
            println!("[B] session.list_jobs returned {} job(s)", jobs.jobs.len());
            break;
        }
    }

    // B subscribes to the running job.
    let mut sub = Envelope::new(MessageType::JobSubscribe(JobSubscribePayload {
        job_id: job_id.clone(),
        from_event_seq: None,
        history: false,
    }));
    sub.session_id = Some(b_session.clone());
    b.send(sub).await?;

    // Read events on B until job.completed (or timeout).
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while std::time::Instant::now() < deadline {
        let Ok(Ok(Some(env))) = tokio::time::timeout(Duration::from_millis(500), b.recv()).await
        else {
            break;
        };
        match env.payload {
            MessageType::JobSubscribed(ack) => {
                println!(
                    "[B] job.subscribed subscribed_from={} status={}",
                    ack.subscribed_from, ack.current_status,
                );
            }
            MessageType::JobCompleted(_) => {
                println!("[B] received job.completed (cross-session forward)");
                break;
            }
            other => {
                println!("[B] event: {}", other.type_name());
            }
        }
    }

    // B unsubscribes.
    let mut unsub = Envelope::new(MessageType::JobUnsubscribe(JobUnsubscribePayload {
        job_id,
    }));
    unsub.session_id = Some(b_session);
    b.send(unsub).await?;
    println!("[B] sent job.unsubscribe");

    Ok(())
}
