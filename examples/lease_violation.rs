//! ARCP v1.1 §9.5 — lease violation handling.
//!
//! A lease grants time-limited access to a named resource set. If an agent
//! attempts to access a resource that is NOT covered by its current lease,
//! the runtime rejects the action with `LEASE_VIOLATION` (or revokes the
//! lease and emits `lease.revoked` with reason `violated`).
//!
//! This example demonstrates three scenarios:
//!   1. Normal use — agent stays within its granted resource set.
//!   2. Over-reach — agent attempts a resource outside the lease scope;
//!      the runtime emits `lease.revoked` with `reason: "violated"`.
//!   3. Expired lease — the agent holds an expired lease and attempts
//!      a resource access; the runtime rejects with `LEASE_EXPIRED`.
//!
//! Run with:
//!     `cargo run --example lease_violation`

#![allow(
    clippy::todo,
    clippy::unimplemented,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::doc_markdown,
    clippy::needless_pass_by_value,
    clippy::too_many_arguments,
    clippy::unused_async,
    clippy::diverging_sub_expression,
    clippy::no_effect_underscore_binding,
    clippy::let_unit_value,
    clippy::used_underscore_binding,
    clippy::let_underscore_untyped,
    clippy::struct_field_names,
    clippy::manual_let_else,
    clippy::map_unwrap_or,
    clippy::redundant_pub_crate,
    dead_code,
    unreachable_code,
    unused_assignments,
    unused_mut,
    unused_imports,
    unused_variables
)]

use std::env;

use arcp::error::{ARCPError, ErrorCode};
use arcp::transport::MemoryTransport;
use arcp::ARCPClient;
use serde_json::json;

type Client = ARCPClient<MemoryTransport>;

/// Submit a job requesting a `net.fetch` lease scoped to `allowed_urls`.
/// Returns `(job_id, lease_id)`.
async fn submit_fetcher(
    _client: &Client,
    _allowed_urls: &[&str],
) -> Result<(String, String), ARCPError> {
    // client.request(envelope("tool.invoke", {
    //   tool: "url-fetcher",
    //   arguments: {},
    //   lease_request: {resources: {"net.fetch": allowed_urls}},
    // })) -> (job_id from job.accepted, lease_id from lease.granted)
    todo!()
}

/// Tell the running agent (via the runtime) to fetch `url`.
/// Returns `Ok` when the fetch is permitted, `Err(LeaseViolation)` when
/// the runtime rejects the resource as out-of-scope.
async fn agent_fetch(_client: &Client, _job_id: &str, _url: &str) -> Result<String, ARCPError> {
    // client.request(envelope("tool.invoke", {
    //   target_job_id: job_id,
    //   tool: "url-fetcher.fetch",
    //   arguments: {url: url},
    // }))
    todo!()
}

async fn await_revoked_event(_client: &Client, _lease_id: &str) -> Result<String, ARCPError> {
    // for await env in client.events():
    //   if env.type == "lease.revoked" and env.payload.lease_id == lease_id:
    //     return env.payload.reason
    todo!()
}

async fn scenario_normal(client: &Client) -> Result<(), ARCPError> {
    let allowed = &["https://example.com/**"];
    let (job_id, lease_id) = submit_fetcher(client, allowed).await?;
    println!("[normal] job={job_id} lease={lease_id}");

    let body = agent_fetch(client, &job_id, "https://example.com/index.html").await?;
    println!("[normal] fetch succeeded, len={}", body.len());
    Ok(())
}

async fn scenario_over_reach(client: &Client) -> Result<(), ARCPError> {
    let allowed = &["https://example.com/**"];
    let (job_id, lease_id) = submit_fetcher(client, allowed).await?;
    println!("[over-reach] job={job_id} lease={lease_id}");

    // Attempt a URL that is outside the granted scope.
    let result = agent_fetch(client, &job_id, "https://evil.example.net/data").await;
    match result {
        Err(ARCPError::Custom {
            code: ErrorCode::LeaseViolation,
            ..
        }) => {
            let reason = await_revoked_event(client, &lease_id).await?;
            println!("[over-reach] lease revoked with reason={reason} — expected");
        }
        other => {
            return Err(format!("expected LeaseViolation but got {other:?}").into());
        }
    }
    Ok(())
}

async fn scenario_expired_lease(client: &Client) -> Result<(), ARCPError> {
    // Request a lease with a 0-second TTL so it expires immediately.
    let allowed = &["https://example.com/**"];
    let (job_id, lease_id) = submit_fetcher(client, allowed).await?;

    // Force expiry by waiting briefly; in tests a clock injection is used.
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let result = agent_fetch(client, &job_id, "https://example.com/data").await;
    match result {
        Err(ARCPError::Custom {
            code: ErrorCode::LeaseExpired,
            ..
        }) => {
            println!("[expired] access after expiry rejected — expected");
        }
        other => eprintln!("[expired] unexpected outcome: {other:?}"),
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client: Client = todo!(); // transport, identity, auth elided

    match env::args().nth(1).as_deref().unwrap_or("normal") {
        "normal" => scenario_normal(&client).await?,
        "over-reach" | "over_reach" => scenario_over_reach(&client).await?,
        "expired" => scenario_expired_lease(&client).await?,
        other => {
            eprintln!("unknown scenario: {other}  (normal|over-reach|expired)");
            std::process::exit(2);
        }
    }
    println!("done");
    Ok(())
}
