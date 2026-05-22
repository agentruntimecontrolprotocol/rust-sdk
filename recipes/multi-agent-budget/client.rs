//! ARCP v1.1 §9.6 — multi-agent-budget: client side.
//!
//! Submits a top-level research question with a USD:0.50 budget cap.
//! The planner decomposes the question and delegates sub-questions to
//! worker agents, each with a budget slice carved from the planner's own
//! remaining allowance.
//!
//! Highlights:
//!   - §9.6  `cost.budget` auto-decrement on `cost.*` metrics
//!   - §10   `agent.delegate` lease — workers inherit a subset of the cap
//!   - §13.2 lease-subset enforcement at delegate time
//!
//! Run (after starting the server in a separate terminal):
//!     `cargo run --example multi-agent-budget-client`

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

/// Submit the top-level research job.
///
/// The lease caps total cost at USD:0.50, allows `llm.complete` tool calls,
/// and permits the planner to delegate to `worker` agents.
async fn submit_research(_client: &Client, _question: &str) -> Result<String, ARCPError> {
    // client.request(envelope("tool.invoke", {
    //   agent: "planner",
    //   input: { question },
    //   lease_request: {
    //     "cost.budget":    ["USD:0.50"],
    //     "tool.call":      ["llm.complete"],
    //     "agent.delegate": ["worker"],
    //   },
    // })) -> job_id from job.accepted
    todo!()
}

/// Drain job events until the terminal `job.completed` or `job.failed`.
///
/// Worker sub-job events arrive in the same stream, stamped with the same
/// monotonic `event_seq`, so no demultiplexing is required.
async fn await_result(_client: &Client, _job_id: &str) -> Result<Value, ARCPError> {
    // for await env in client.events():
    //   match env.type:
    //     "job.event"     => println!("{}: {}", env.payload.kind, env.payload.body)
    //     "job.completed" => return Ok(env.payload.result)
    //     "job.failed"    => return Err(ARCPError::from(env.payload))
    todo!()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client: Client = todo!(); // transport, identity, auth elided

    let question = "What causes urban heat islands?";
    let job_id = submit_research(&client, question).await?;
    println!("accepted: job_id={job_id}");

    // Worker sub-jobs run inside the same event stream — no second session
    // needed.  When the budget no longer fits a grant the planner skips
    // remaining sub-questions and returns whatever it has.
    let result = await_result(&client, &job_id).await?;
    println!("result: {result}");

    println!("done");
    Ok(())
}
