//! Two scenarios over the §10.4 / §10.5 control surface.
//!
//! - `cancel`: cooperative termination. Runtime drives target to a clean
//!   checkpoint inside `deadline_ms`, then escalates to `ABORTED`.
//! - `interrupt`: pauses the job (`blocked`) and emits a
//!   `permission.request`. The job is NOT terminated.

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

use std::env;

use arcp::error::ARCPError;
use arcp::transport::MemoryTransport;
use arcp::{ARCPClient, Envelope, ErrorCode};
use serde_json::json;

type Client = ARCPClient<MemoryTransport>;

const CANCEL_DEADLINE_MS: u32 = 5_000;

async fn start_long_job(_client: &Client) -> Result<String, ARCPError> {
    // client.request(envelope("tool.invoke", {tool: "demo.long_running",
    //   arguments: {work_seconds: 600}})) -> job_id from `job.accepted`
    todo!()
}

/// Cooperative cancel. Runtime drives target to a clean checkpoint inside
/// `deadline_ms` before terminating; escalates to `ABORTED` on timeout
/// (RFC §10.4). Returns the `cancel.accepted` envelope.
async fn cancel_job(
    _client: &Client,
    _job_id: &str,
    _reason: &str,
    _deadline_ms: u32,
) -> Result<Envelope, ARCPError> {
    // reply = client.request(envelope("cancel", {target: "job",
    //   target_id: job_id, reason, deadline_ms}), timeout=deadline_ms+5s)
    // if reply.type == "cancel.refused": Err(FailedPrecondition)
    todo!()
}

/// Distinct from cancel: pauses the job (`blocked`); the runtime emits
/// `permission.request`. The job is NOT terminated (RFC §10.5).
async fn interrupt_job(_client: &Client, _job_id: &str, _prompt: &str) -> Result<(), ARCPError> {
    // client.send(envelope("interrupt", {target: "job", target_id, prompt}))
    todo!()
}

async fn await_terminal(_client: &Client, _job_id: &str) -> Result<Envelope, ARCPError> {
    // for await env in client.events():
    //   if env.job_id == job_id and env.type in
    //     {"job.completed", "job.failed", "job.cancelled"}: return env
    todo!()
}

async fn scenario_cancel() -> Result<(), ARCPError> {
    let client: Client = todo!(); // transport, identity, auth elided
    let job_id = start_long_job(&client).await?;
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    let ack = cancel_job(&client, &job_id, "user_aborted", CANCEL_DEADLINE_MS).await?;
    println!("cancel ack: {:?}", ack.payload);
    let terminal = await_terminal(&client, &job_id).await?;
    println!("terminal: {:?}", terminal.payload);
    Ok(())
}

async fn scenario_interrupt() -> Result<(), ARCPError> {
    let client: Client = todo!();
    let job_id = start_long_job(&client).await?;
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    interrupt_job(
        &client,
        &job_id,
        "Pause and ask before touching production tables.",
    )
    .await?;
    // Runtime now emits permission.request; handle the grant/deny flow.
    let _next: Envelope = todo!();
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    match env::args().nth(1).as_deref().unwrap_or("cancel") {
        "cancel" => scenario_cancel().await?,
        "interrupt" => scenario_interrupt().await?,
        other => {
            eprintln!("unknown scenario: {other}");
            std::process::exit(2);
        }
    }
    Ok(())
}
