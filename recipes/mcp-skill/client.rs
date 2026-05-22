//! ARCP v1.1 — mcp-skill: MCP tool caller (Claude Code side).
//!
//! This file represents the Claude Code / MCP client perspective.  In
//! practice Claude Code invokes the `research` MCP tool directly; this
//! file shows what the MCP server receives and forwards to ARCP.
//!
//! The MCP server (`server.rs`) maintains **one long-lived ARCP session**
//! per process.  Each `/research` invocation submits a `planner` job and
//! blocks until the job reaches a terminal state, then returns the result
//! as an MCP `text` content block.
//!
//! See `skills/research/SKILL.md` for the Claude Code skill definition that
//! wires the MCP tool into the agent's context.
//!
//! Highlights:
//!   - §6   one ARCP session shared across many MCP tool calls
//!   - §7   `tool.invoke` → `job.accepted` → `job.completed`
//!   - §9.6 `cost.budget` lease passed from MCP caller to planner
//!
//! Run (after starting the server):
//!     `cargo run --example mcp-skill-client`

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

/// Invoke the `research` MCP skill by submitting a `planner` ARCP job.
///
/// `budget_usd` becomes the `cost.budget` cap on the top-level lease so
/// spending cannot exceed what the MCP caller authorised.
async fn research(
    _client: &Client,
    _question: &str,
    _budget_usd: f64,
) -> Result<Value, ARCPError> {
    // client.request(envelope("tool.invoke", {
    //   agent: "planner",
    //   input: { question, budget_usd },
    //   lease_request: {
    //     "cost.budget":  [format!("USD:{budget_usd:.2}")],
    //     "tool.call":    ["llm.complete"],
    //     "agent.delegate": ["worker"],
    //   },
    // })) -> job_id from job.accepted
    //
    // then drain events until job.completed / job.failed:
    //   return Ok(env.payload.result)
    todo!()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // In practice this client is the MCP server process itself — it holds
    // the persistent ARCP session and is invoked by Claude Code.
    let client: Client = todo!(); // transport, identity, auth elided

    let result = research(&client, "What causes urban heat islands?", 0.50).await?;
    println!("result: {result}");

    println!("done");
    Ok(())
}
