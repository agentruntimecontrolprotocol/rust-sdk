//! ARCP v1.1 §6.5 — `backpressure` / slow-consumer flow-control demo.
//!
//! The runtime tracks unacked event lag per session. When the client falls
//! behind (ack watermark far below the latest `event_seq`), the runtime
//! emits a `backpressure` envelope and may throttle event delivery until
//! the client catches up.
//!
//! This example:
//!   1. Connects with a very slow auto-ack cadence so the consumer
//!      intentionally falls behind.
//!   2. Submits a `chatty` job that emits ~2 000 metric events.
//!   3. Waits to observe a `backpressure` envelope from the runtime.
//!   4. Catches up by sending a `session.ack` at the current high watermark.
//!
//! Run with:
//!     `cargo run --example ack_backpressure`

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

const CHATTY_COUNT: u32 = 2_000;

/// Submit the `chatty` job and return the job ID from `job.accepted`.
async fn submit_chatty(_client: &Client) -> Result<String, ARCPError> {
    // client.request(envelope("tool.invoke", {tool: "chatty",
    //   arguments: {count: CHATTY_COUNT}})) -> job_id from job.accepted
    todo!()
}

/// Send a `session.ack` advancing the watermark to `last_seq`.
async fn ack(_client: &Client, _last_seq: u64) -> Result<(), ARCPError> {
    // client.send(envelope("session.ack", {last_processed_seq: last_seq}))
    todo!()
}

/// Receive events until the job reaches a terminal state.  Returns
/// `(metric_count, back_pressure_observed)`.
async fn drain(_client: &Client, _job_id: &str) -> Result<(u32, bool), ARCPError> {
    let mut metrics: u32 = 0;
    let mut back_pressure = false;

    // for await env in client.events():
    //   match env.payload.type_name():
    //     "metric"       => metrics += 1,
    //     "backpressure" => { back_pressure = true; ack(client, env.event_seq).await? }
    //     "job.completed" | "job.failed" | "job.cancelled" => break,

    Ok((metrics, back_pressure))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Connect with autoAck disabled / cadence set very slow so the
    // consumer falls behind and the runtime emits `backpressure`.
    let client: Client = todo!(); // transport, identity, auth elided

    let job_id = submit_chatty(&client).await?;
    println!("accepted: job_id={job_id}");

    let (metrics, bp) = drain(&client, &job_id).await?;
    println!("metrics observed={metrics}  back_pressure={bp}");

    if !bp {
        return Err("expected a backpressure event but none arrived".into());
    }
    println!("back-pressure observed and acknowledged — done");
    Ok(())
}
