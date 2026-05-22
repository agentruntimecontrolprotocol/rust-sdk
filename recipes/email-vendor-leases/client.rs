//! ARCP v1.1 — email-vendor-leases: client side.
//!
//! Submits a `triage` agent job with a lease that grants the read-only inbox
//! tools (`inbox_list`, `inbox_read`) but deliberately omits `send_reply`.
//! When the agent's tool-use loop tries to call `send_reply` the runtime
//! rejects with `PERMISSION_DENIED`; the agent recovers and returns a drafted
//! reply instead.
//!
//! Highlights:
//!   - §13.4  lease violation as a *recoverable* tool error, not session-fatal
//!   - §15 / §8.2  `x-vendor.acme.email.parsed` event kind emitted by server
//!
//! Run (after starting the server in a separate terminal):
//!     `cargo run --example email-vendor-leases-client`

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
use serde_json::{json, Value};

type Client = ARCPClient<MemoryTransport>;

/// Submit the triage job.
///
/// The lease grants `tool.call` only for `inbox_list` and `inbox_read`.
/// `send_reply` is intentionally absent — when the agent proposes that tool
/// the runtime returns `PERMISSION_DENIED` as a recoverable tool result.
async fn submit_triage(_client: &Client) -> Result<String, ARCPError> {
    // client.request(envelope("tool.invoke", {
    //   agent: "triage",
    //   input: {},
    //   lease_request: {
    //     "tool.call": ["inbox_list", "inbox_read"],
    //     // send_reply is NOT listed — attempting it yields PERMISSION_DENIED
    //   },
    // })) -> job_id from job.accepted
    todo!()
}

/// Await the terminal `job.completed` or `job.failed` event.
///
/// The server emits `x-vendor.acme.email.parsed` events for each message it
/// reads; those arrive before the terminal event.  Collect them here so the
/// caller can render them.
async fn await_result(
    _client: &Client,
    _job_id: &str,
) -> Result<Value, ARCPError> {
    // for await env in client.events():
    //   match env.type:
    //     "job.event" if env.payload.kind == "x-vendor.acme.email.parsed" =>
    //       println!("parsed: from={} subject={}", env.payload.body.from, env.payload.body.subject)
    //     "job.completed" => return Ok(env.payload.result)
    //     "job.failed"    => return Err(ARCPError::from(env.payload))
    todo!()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client: Client = todo!(); // transport, identity, auth elided

    let job_id = submit_triage(&client).await?;
    println!("accepted: job_id={job_id}");

    let result = await_result(&client, &job_id).await?;

    // The agent could not send the reply (denied by the lease) so it drafted
    // it instead.  The result carries both fields.
    let drafted = result.get("drafted_reply").and_then(Value::as_str).unwrap_or("");
    let sent    = result.get("sent").and_then(Value::as_bool).unwrap_or(false);
    println!("sent={sent}  drafted_reply={drafted:.80}...");

    println!("done");
    Ok(())
}
