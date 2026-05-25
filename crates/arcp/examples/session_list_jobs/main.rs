//! ARCP v1.1 §6.6 — `session.list_jobs` / `session.jobs` demo.
//!
//! Submits two long-running echo jobs to populate the registry, then
//! sends a `session.list_jobs` request and prints the runtime's
//! `session.jobs` response.
//!
//! Run with:
//!     `cargo run --example session_list_jobs`

#![allow(clippy::similar_names, clippy::expect_used)]

use std::sync::Arc;
use std::time::Duration;

use arcp::auth::BearerAuthenticator;
use arcp::envelope::Envelope;
use arcp::error::ARCPError;
use arcp::messages::{
    AuthScheme, Capabilities, ClientIdentity, Credentials, MessageType, SessionListJobsPayload,
    SessionOpenPayload, ToolInvokePayload,
};
use arcp::runtime::context::ToolContext;
use arcp::runtime::tools::{ToolHandler, ToolRegistryBuilder};
use arcp::runtime::ARCPRuntime;
use arcp::transport::{paired, Transport};
use async_trait::async_trait;

struct SleepTool;

#[async_trait]
impl ToolHandler for SleepTool {
    fn name(&self) -> &'static str {
        "sleep"
    }

    async fn invoke(
        &self,
        _arguments: serde_json::Value,
        ctx: ToolContext,
    ) -> Result<serde_json::Value, ARCPError> {
        // Stay alive long enough for the listing to observe us running.
        tokio::select! {
            () = ctx.cancel.cancelled() => Err(ARCPError::Cancelled { reason: "cancelled".into() }),
            () = tokio::time::sleep(Duration::from_secs(2)) => Ok(serde_json::json!("done")),
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let tools = ToolRegistryBuilder::new().with(Arc::new(SleepTool)).build();
    let runtime = ARCPRuntime::builder()
        .with_authenticator(Box::new(BearerAuthenticator::new().with_token("t", "p")))
        .with_tools(tools)
        .build()
        .await?;
    let (server_t, client_t) = paired();
    let _h = runtime.serve_connection(server_t);

    let mut open = Envelope::new(MessageType::SessionOpen(SessionOpenPayload {
        auth: Credentials {
            scheme: AuthScheme::Bearer,
            token: Some("t".into()),
        },
        client: ClientIdentity {
            kind: "list-jobs-demo".into(),
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
    let session_id = payload.session_id;

    // Submit two sleep jobs.
    for _ in 0..2 {
        let mut invoke = Envelope::new(MessageType::ToolInvoke(ToolInvokePayload::new(
            "sleep",
            serde_json::json!({}),
        )));
        invoke.session_id = Some(session_id.clone());
        client_t.send(invoke).await?;
        // Drain job.accepted so the listing observes a running job
        // (the runtime is still in transit on job.started by now).
        let _ = client_t.recv().await?.ok_or("no job.accepted")?;
    }

    // Give the runtime a moment to flip jobs to running.
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Request the listing.
    let mut list = Envelope::new(MessageType::SessionListJobs(SessionListJobsPayload {
        filter: None,
        limit: None,
        cursor: None,
    }));
    list.session_id = Some(session_id);
    client_t.send(list).await?;

    // Drain envelopes until we see `session.jobs`.
    loop {
        let env = tokio::time::timeout(Duration::from_secs(1), client_t.recv())
            .await??
            .ok_or("transport closed")?;
        if let MessageType::SessionJobs(jobs) = env.payload {
            println!("session.jobs: request_id={} jobs:", jobs.request_id);
            for j in &jobs.jobs {
                println!(
                    "  job_id={} agent={} status={} created_at={}",
                    j.job_id, j.agent, j.status, j.created_at
                );
            }
            assert_eq!(jobs.jobs.len(), 2);
            break;
        }
    }

    println!("done");
    Ok(())
}
