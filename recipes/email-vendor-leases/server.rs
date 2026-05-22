//! ARCP v1.1 — email-vendor-leases: runtime / server side.
//!
//! A triage agent receives an "inbox check" task with a lease that grants
//! only the read-only tools (`inbox_list`, `inbox_read`).  A Claude
//! tool-use loop drives the agent.  When Claude proposes `send_reply` the
//! runtime rejects the lease check and feeds back a `PERMISSION_DENIED`
//! tool result — the model recovers and drafts the reply for human review
//! instead of sending it.
//!
//! Highlights:
//!   - §13.4  lease violation as a *recoverable* `tool_result` error
//!   - §8.2 / §15  `x-vendor.acme.email.parsed` vendor-extension event
//!   - realistic Claude `tool_use` loop with graceful deny handling
//!
//! Run:
//!     `cargo run --example email-vendor-leases-server`

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
use arcp::lease::validate_lease_op;
use arcp::transport::MemoryTransport;
use arcp::{ARCPRuntime, JobContext};
use serde_json::{json, Value};

type Runtime = ARCPRuntime<MemoryTransport>;

/// Tool definitions forwarded to Claude.
///
/// Three tools: two read-only ones covered by the client's lease and one
/// (`send_reply`) that is deliberately excluded so a `tool.call` attempt
/// triggers a `PERMISSION_DENIED` response.
fn tool_list() -> Value {
    json!([
        {
            "name": "inbox_list",
            "description": "List recent unread messages.",
            "input_schema": { "type": "object", "properties": {} },
        },
        {
            "name": "inbox_read",
            "description": "Read one message by id.",
            "input_schema": {
                "type": "object",
                "properties": { "id": { "type": "string" } },
                "required": ["id"],
            },
        },
        {
            "name": "send_reply",
            "description": "Send a reply to a message.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "id":   { "type": "string" },
                    "body": { "type": "string" },
                },
                "required": ["id", "body"],
            },
        },
    ])
}

/// Execute the (already-authorised) tool and return its result payload.
async fn run_tool(_name: &str, _input: &Value) -> Result<Value, ARCPError> {
    // inbox_list  → Vec<{id, from, subject, urgency}>
    // inbox_read  → {id, from, subject, body, urgency}
    // send_reply  → never reaches here — denied by lease check first
    todo!()
}

/// The triage agent: drives a Claude `tool_use` loop.
///
/// 1. Ask Claude to triage the inbox.
/// 2. For each `tool_use` block:
///    a. Validate the operation against the job's lease.
///    b. If denied: surface `PERMISSION_DENIED` as a `tool_result` error
///       and feed it back to Claude so it can recover.  The lease violation
///       is NOT session-fatal.
///    c. If authorised: run the tool.  For `inbox_read`, also emit an
///       `x-vendor.acme.email.parsed` event so dashboards can render it.
/// 3. Loop until Claude returns `stop_reason == "end_turn"`.
async fn triage_agent(
    _input: &Value,
    _ctx: &mut JobContext<'_>,
) -> Result<Value, ARCPError> {
    // let anthropic = Anthropic::new(); // requires `anthropic` crate
    // let mut messages = vec![{
    //   "role": "user",
    //   "content": "Triage my inbox. Read each unread message and reply to anything urgent.",
    // }];
    //
    // loop {
    //   let turn = anthropic.messages()
    //     .model("claude-sonnet-4-6")
    //     .max_tokens(1024)
    //     .tools(tool_list())
    //     .messages(&messages)
    //     .send().await?;
    //
    //   if turn.stop_reason == "end_turn" {
    //     let text = turn.text_content();
    //     return Ok(json!({ "drafted_reply": text, "sent": false }));
    //   }
    //
    //   messages.push(turn.assistant_turn());
    //   let mut tool_results = vec![];
    //
    //   for block in turn.tool_use_blocks() {
    //     ctx.tool_call(&block.name, &block.input, &block.id).await?;
    //
    //     match validate_lease_op(ctx.lease(), "tool.call", &block.name) {
    //       Err(ARCPError::PermissionDenied { detail }) => {
    //         // surface as recoverable error on the ARCP stream…
    //         ctx.tool_result_err(&block.id, &detail).await?;
    //         // …and feed denial back to Claude so it can adapt
    //         tool_results.push(tool_result_err(&block.id, &format!("denied: {detail}")));
    //         continue;
    //       }
    //       Err(e) => return Err(e),
    //       Ok(()) => {}
    //     }
    //
    //     let result = run_tool(&block.name, &block.input).await?;
    //     if block.name == "inbox_read" {
    //       ctx.emit_event("x-vendor.acme.email.parsed", json!({
    //         "message_id": result["id"],
    //         "from":       result["from"],
    //         "subject":    result["subject"],
    //         "urgency":    result["urgency"],
    //       })).await?;
    //     }
    //     ctx.tool_result_ok(&block.id, &result).await?;
    //     tool_results.push(tool_result_ok(&block.id, result));
    //   }
    //   messages.push(user_turn_with_results(tool_results));
    // }
    todo!()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let runtime: Runtime = todo!(); // transport, identity, auth elided

    // runtime.register_agent("triage", |input, ctx| Box::pin(triage_agent(input, ctx)));
    // runtime.serve("127.0.0.1:7900").await?;

    Ok(())
}
