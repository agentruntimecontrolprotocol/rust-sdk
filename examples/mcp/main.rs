//! ARCP runtime fronting an MCP server (RFC §20).
//!
//! MCP describes capabilities; ARCP operationalizes them. This bridge
//! translates inbound ARCP `tool.invoke` envelopes into MCP `call_tool`
//! calls against an upstream MCP server, and emits the ARCP job
//! lifecycle back to the calling client.
//!
//! ```text
//! ARCP client  --tool.invoke-->  bridge  --call_tool-->  MCP server
//! ARCP client  <--job.{accepted,started,completed,failed}--  bridge
//! ```
//!
//! Per RFC §20:
//!   MCP tool schema -> ARCP capability  (advertised at session.accepted)
//!   MCP tool call   -> ARCP job
//!   MCP resource    -> ARCP stream of kind: event  (delegated to MCP)

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

mod upstream;

use arcp::error::ARCPError;
use arcp::{Envelope, ErrorCode};
use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::upstream::{upstream_params, ClientSession};

/// MCP `tools/list` -> namespaced ARCP capability extensions. Each upstream
/// tool surfaces as `arcpx.mcp.tool.<name>.v1` so clients can negotiate
/// which tools they require at session open.
async fn advertise_from_mcp(mcp: &ClientSession) -> Vec<String> {
    mcp.list_tools()
        .await
        .into_iter()
        .map(|t| format!("arcpx.mcp.tool.{t}.v1"))
        .collect()
}

/// Translate ARCP `tool.invoke.payload` into MCP `call_tool`. MCP returns
/// typed content blocks; we flatten to a JSON-serializable dict for the
/// ARCP `tool.result` / `job.completed` payload.
async fn call_via_mcp(
    _mcp: &ClientSession,
    tool: &str,
    arguments: Value,
) -> Result<Value, ARCPError> {
    // result = mcp.call_tool(tool, arguments).await
    // if result.is_error: Err(FailedPrecondition{detail: result.text})
    // else: Ok(json!({"content": [...]}))
    let _ = (tool, arguments);
    todo!()
}

/// One inbound ARCP `tool.invoke` -> MCP call -> ARCP job lifecycle.
async fn handle_invoke(
    send: &mpsc::Sender<Envelope>,
    mcp: &ClientSession,
    request: Envelope,
) -> Result<(), ARCPError> {
    let job_id = "job_<rand>";
    // send(envelope("job.accepted", correlation_id=request.id, job_id,
    //   payload={job_id, state: "accepted"}))
    // send(envelope("job.started", job_id, payload={job_id}))

    let tool: String = todo!(); // request.payload["tool"]
    let arguments: Value = todo!(); // request.payload["arguments"]
    match call_via_mcp(mcp, &tool, arguments).await {
        Ok(_result) => {
            // send(envelope("job.completed", job_id, payload={result}))
        }
        Err(_exc) => {
            // send(envelope("job.failed", job_id, payload=exc.to_payload()))
        }
    }
    let _ = (send, request);
    Ok(())
}

/// Wire one MCP session as the upstream for one ARCP runtime.
async fn run_bridge(
    send: mpsc::Sender<Envelope>,
    mut inbound: mpsc::Receiver<Envelope>,
) -> Result<(), ARCPError> {
    let _params = upstream_params();
    let mcp = ClientSession;
    mcp.initialize().await;
    let extensions = advertise_from_mcp(&mcp).await;
    println!("bridged: {extensions:?}");

    while let Some(envelope) = inbound.recv().await {
        // Match on envelope.payload type for ToolInvoke; peek at the wire
        // tag for the runtime's tool.invoke handler.
        handle_invoke(&send, &mcp, envelope).await?;
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Production: instantiate `arcp::ARCPRuntime`, point its tool-invoke
    // handler at `handle_invoke`, let the WebSocket transport carry inbound
    // envelopes from real ARCP clients. We elide the runtime wiring so this
    // file stays focused on the §20 translation between protocols.
    let (send_tx, _send_rx) = mpsc::channel::<Envelope>(64); // bound to runtime's outbound channel
    let (_inbound_tx, inbound_rx) = mpsc::channel::<Envelope>(64);
    run_bridge(send_tx, inbound_rx).await?;
    Ok(())
}
