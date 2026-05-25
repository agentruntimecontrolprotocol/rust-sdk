//! ARCP v1.1 §7 — submit a job and stream every event until terminal.
//!
//! Connects to the runtime, submits a single `data-analyzer` job, prints
//! every `job.event` envelope as it arrives, then prints the terminal
//! `job.result` (or propagates the error on `job.failed`).
//!
//! This is the minimal end-to-end pattern most agents follow:
//!   1. Open session (`session.open` → `session.accepted`).
//!   2. Invoke tool (`tool.invoke` → `job.accepted`).
//!   3. Stream events (`job.event`) until `job.completed` or `job.failed`.
//!
//! Run with:
//!     `cargo run --example submit_and_stream`

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

use arcp::error::ARCPError;
use arcp::transport::MemoryTransport;
use arcp::{ARCPClient, Envelope};
use serde_json::json;

type Client = ARCPClient<MemoryTransport>;

/// Submit `data-analyzer` and return its job ID from `job.accepted`.
async fn submit(_client: &Client) -> Result<String, ARCPError> {
    // client.request(envelope("tool.invoke", {
    //   tool: "data-analyzer",
    //   arguments: {dataset: "s3://example/sales.csv"},
    //   lease_request: {resources: {"net.fetch": ["s3://example/**"]}},
    //   idempotency_key: "sales-q1-analysis",
    // })) -> job_id from job.accepted
    todo!()
}

/// Stream all events for `job_id`, printing each one, until the job
/// reaches a terminal state.  Returns the final result payload.
async fn stream_until_done(
    _client: &Client,
    _job_id: &str,
) -> Result<serde_json::Value, ARCPError> {
    // for await env in client.events():
    //   if env.job_id != job_id { continue }
    //   match env.type:
    //     "job.event" => {
    //       println!(
    //         "event[seq={}] {} {}",
    //         env.event_seq,
    //         env.payload.kind,
    //         serde_json::to_string(&env.payload.body).unwrap_or_default(),
    //       );
    //     }
    //     "job.completed" => return Ok(env.payload.result),
    //     "job.failed"    => return Err(ARCPError::from(env.payload)),
    //     "job.cancelled" => return Err(ARCPError::Cancelled { job_id: job_id.into() }),
    //     _ => {}
    todo!()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client: Client = todo!(); // transport, identity, auth elided

    // Print session info (session_id, runtime name) from session.accepted.
    // welcome = client.connect(transport)?;
    // println!("welcome: session={} runtime={}", client.session_id(), welcome.runtime.name);

    let job_id = submit(&client).await?;
    println!("accepted: job_id={job_id}");

    let result = stream_until_done(&client, &job_id).await?;
    println!("result: {result}");

    Ok(())
}
