//! ARCP v1.1 — mcp-skill: MCP → ARCP bridge (server / runtime side).
//!
//! This process is both an **MCP server** (Claude Code connects to it) and
//! an **ARCP client** (it connects to the ARCP runtime).
//!
//! Architecture:
//!
//!   Claude Code  ──MCP──►  this process  ──ARCP──►  ARCP runtime
//!                  tool calls               job.invoke
//!
//! One persistent ARCP session is established at startup.  Each incoming
//! MCP `research` tool call submits a `planner` job to the runtime, waits
//! for the terminal event, and returns the result as an MCP text content
//! block.
//!
//! Highlights:
//!   - §6   single ARCP session reused across all MCP calls
//!   - §7   `tool.invoke` → `job.accepted` → `job.completed`
//!   - §9.6 `cost.budget` lease derived from the MCP `budget_usd` param
//!   - §10  `agent.delegate` lease so the planner may spawn workers
//!
//! Run:
//!     `cargo run --example mcp-skill-server`
//!
//! See `skills/research/SKILL.md` for the Claude Code skill definition.

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

use std::sync::Arc;

use arcp::error::ARCPError;
use arcp::transport::MemoryTransport;
use arcp::ARCPClient;
use serde_json::{json, Value};
use tokio::sync::Mutex;

type Client = ARCPClient<MemoryTransport>;

/// The shared ARCP client — one per process.
struct McpBridge {
    arcp: Arc<Mutex<Client>>,
}

impl McpBridge {
    /// Establish the ARCP session and return the bridge.
    async fn connect() -> Result<Self, ARCPError> {
        // let transport = WebSocketTransport::connect("ws://127.0.0.1:7898/arcp").await?;
        // let client = ARCPClient::new(transport, identity, auth)?;
        // client.connect().await?;
        // Ok(Self { arcp: Arc::new(Mutex::new(client)) })
        todo!()
    }

    /// Handle the MCP `research` tool call.
    ///
    /// Translates the MCP input into an ARCP `tool.invoke`, waits for
    /// `job.completed` / `job.failed`, and returns the text result.
    async fn research(&self, _question: &str, _budget_usd: f64) -> Result<Value, ARCPError> {
        // let client = self.arcp.lock().await;
        //
        // let job_id = client.request(envelope("tool.invoke", {
        //   agent: "planner",
        //   input: { question, budget_usd },
        //   lease_request: {
        //     "cost.budget":    [format!("USD:{budget_usd:.2}")],
        //     "tool.call":      ["llm.complete"],
        //     "agent.delegate": ["worker"],
        //   },
        // }))
        // .await?
        // .job_id;
        //
        // // drain events until terminal
        // loop {
        //   let env = client.next_event().await?;
        //   match env.r#type.as_str() {
        //     "job.completed" => return Ok(env.payload.result),
        //     "job.failed"    => return Err(ARCPError::from(env.payload)),
        //     _               => continue,
        //   }
        // }
        todo!()
    }
}

/// MCP tool handler — called by the MCP server framework for each
/// `/research` tool invocation from Claude Code.
async fn handle_mcp_research(
    bridge: Arc<McpBridge>,
    input: Value,
) -> Result<Value, Box<dyn std::error::Error>> {
    let question   = input["question"].as_str().unwrap_or("");
    let budget_usd = input["budget_usd"].as_f64().unwrap_or(0.10);

    let result = bridge.research(question, budget_usd).await?;

    // Return an MCP text content block.
    Ok(json!({ "type": "text", "text": result["summary"].as_str().unwrap_or("") }))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bridge = Arc::new(McpBridge::connect().await?);

    // Register the research tool with the MCP server framework and start
    // listening for Claude Code connections.
    //
    // mcp_server::register_tool("research", {
    //   description: "Deep-research a question up to budget_usd USD.",
    //   schema: { question: string, budget_usd: number },
    //   handler: |input| handle_mcp_research(bridge.clone(), input),
    // });
    // mcp_server::listen("stdio").await?;

    println!("MCP bridge ready — waiting for Claude Code connections");
    Ok(())
}
