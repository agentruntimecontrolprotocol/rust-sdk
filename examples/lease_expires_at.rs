//! ARCP v1.1 §9 — lease with an absolute `expires_at` deadline.
//!
//! Demonstrates requesting a `net.fetch` lease that expires at a fixed wall-
//! clock time rather than a relative TTL. This is useful when the caller
//! knows the outer operation deadline (e.g. the job SLA) and wants the
//! runtime to automatically revoke the lease when the deadline passes,
//! regardless of how long the job has been running.
//!
//! Sequence of events:
//!   1. Open a session and submit a `web-scraper` job with a lease that
//!      expires 30 s from now.
//!   2. Observe `lease.granted` carrying the `lease_id` and `expires_at`.
//!   3. Job completes before the deadline → runtime emits `lease.revoked`
//!      (or the client releases early).
//!   4. A second scenario requests a lease past the job deadline; the
//!      runtime issues a shorter-lived grant (capped to job TTL).
//!
//! Run with:
//!     `cargo run --example lease_expires_at`

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

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use arcp::error::ARCPError;
use arcp::transport::MemoryTransport;
use arcp::ARCPClient;
use serde_json::{json, Value};

type Client = ARCPClient<MemoryTransport>;

fn unix_ts_plus_secs(secs: u64) -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        + secs
}

/// Submit the `web-scraper` tool with a `lease_request` that uses an
/// absolute `expires_at` timestamp.  Returns `(job_id, lease_id)`.
async fn submit_with_expiry(
    _client: &Client,
    _expires_at_unix: u64,
) -> Result<(String, String), ARCPError> {
    let _payload = json!({
        "tool": "web-scraper",
        "arguments": {"urls": ["https://example.com"]},
        "lease_request": {
            "resources": {"net.fetch": ["https://example.com"]},
            "expires_at": _expires_at_unix,
        }
    });
    // client.request(envelope("tool.invoke", payload)) -> (job_id, lease_id)
    // job_id from job.accepted; lease_id from the concurrent lease.granted
    todo!()
}

/// Block until the job reaches a terminal state; returns the result value.
async fn await_terminal(_client: &Client, _job_id: &str) -> Result<Value, ARCPError> {
    todo!()
}

/// Explicitly release a lease before it expires.
async fn release_lease(_client: &Client, _lease_id: &str) -> Result<(), ARCPError> {
    // client.send(envelope("artifact.release", {lease_id: lease_id}))
    todo!()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client: Client = todo!(); // transport, identity, auth elided

    // --- Scenario 1: lease that outlives the job --------------------------------
    println!("--- scenario 1: expires_at = now+30s ---");
    let expires_at = unix_ts_plus_secs(30);
    let (job_id, lease_id) = submit_with_expiry(&client, expires_at).await?;
    println!("job_id={job_id}  lease_id={lease_id}  expires_at={expires_at}");

    let result = await_terminal(&client, &job_id).await?;
    println!("job result: {result}");

    // Explicitly release; runtime also auto-revokes at expires_at.
    release_lease(&client, &lease_id).await?;
    println!("lease released early");

    // --- Scenario 2: expires_at already past ----------------------------------
    println!("--- scenario 2: expires_at = now+0s (already past) ---");
    let stale_ts = unix_ts_plus_secs(0).saturating_sub(1);
    let result = submit_with_expiry(&client, stale_ts).await;
    match result {
        Err(e) => println!("expired lease request rejected: {e}"),
        Ok(_) => println!("runtime granted a 0-second lease (immediate revocation)"),
    }

    println!("done");
    Ok(())
}
