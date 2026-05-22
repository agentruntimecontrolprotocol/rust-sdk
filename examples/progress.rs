//! ARCP v1.1 §8 — streaming `job.event` progress bars.
//!
//! Submits the `indexer` job and renders a simple text progress bar as
//! `progress` events arrive.  Re-renders the bar on the same stdout line
//! when writing to a TTY, or prints a new line otherwise (CI-friendly).
//!
//! Event kind: `"progress"`; body shape:
//! ```json
//! { "current": 40, "total": 100, "units": "files", "message": "indexing…" }
//! ```
//!
//! Run with:
//!     `cargo run --example progress`

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
use arcp::ARCPClient;
use serde_json::json;

type Client = ARCPClient<MemoryTransport>;

const BAR_WIDTH: usize = 30;

/// Render a text progress bar: `[####......] 40/100`.
fn render_bar(current: u64, total: u64) -> String {
    let ratio = if total == 0 {
        0.0_f64
    } else {
        (current as f64 / total as f64).min(1.0)
    };
    let filled = (BAR_WIDTH as f64 * ratio).round() as usize;
    let empty = BAR_WIDTH - filled;
    format!(
        "[{}{}] {}/{}",
        "#".repeat(filled),
        ".".repeat(empty),
        current,
        total,
    )
}

/// Submit the `indexer` job and return its job ID.
async fn submit_indexer(_client: &Client) -> Result<String, ARCPError> {
    // client.request(envelope("tool.invoke", {
    //   tool: "indexer",
    //   arguments: {total: 100, tick_ms: 30},
    // })) -> job_id from job.accepted
    todo!()
}

/// Drain events for `job_id`; prints progress bars and returns
/// `(final_result, progress_update_count)` when the job terminates.
async fn stream_progress(
    _client: &Client,
    _job_id: &str,
) -> Result<(serde_json::Value, u32), ARCPError> {
    let mut updates: u32 = 0;

    // for await env in client.events():
    //   if env.job_id != job_id { continue }
    //   match env.payload.kind:
    //     "progress" => {
    //       let body: ProgressBody = serde_json::from_value(env.payload.body)?;
    //       updates += 1;
    //       let bar = render_bar(body.current, body.total.unwrap_or(0));
    //       let tail = body.message.as_deref().unwrap_or("");
    //       print!("\r{bar} {}{tail}   ", body.units.as_deref().unwrap_or(""));
    //     }
    //     "job.completed" => return Ok((env.payload.result, updates)),
    //     "job.failed"    => return Err(ARCPError::from(env.payload)),
    //     _ => {}

    todo!()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client: Client = todo!(); // transport, identity, auth elided

    let job_id = submit_indexer(&client).await?;
    println!("accepted: job_id={job_id}");

    let (result, updates) = stream_progress(&client, &job_id).await?;
    // Move to a fresh line after the in-place bar rewrites.
    println!();
    println!("result: {result}  progress-updates={updates}");

    Ok(())
}
