//! Sandboxed on-call agent. Lease-gated shell, reasoning streamed.
//!
//! Reads use a coarse 30-minute lease per host; writes use a 60-second
//! lease scoped to one binary + one target. RFC §15.4 / §15.5.

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

mod agent;

use arcp::error::ARCPError;
use arcp::transport::MemoryTransport;
use arcp::{ARCPClient, ErrorCode};

use crate::agent::{next_step, LlmStep, ToolCall};

type Client = ARCPClient<MemoryTransport>;

const READ_BINARIES: &[&str] = &[
    "/usr/bin/journalctl",
    "/usr/bin/cat",
    "/usr/bin/ss",
    "/usr/bin/ps",
];
const WRITE_BINARIES: &[&str] = &["/usr/bin/systemctl", "/usr/bin/kill"];
const READ_LEASE_SECONDS: u32 = 30 * 60;
const WRITE_LEASE_SECONDS: u32 = 60;

struct Classification {
    permission: &'static str,
    resource: String,
    operation: &'static str,
    seconds: u32,
}

fn classify(argv: &[String], host: &str) -> Result<Classification, ARCPError> {
    let binary = argv.first().map(String::as_str).unwrap_or_default();
    if READ_BINARIES.contains(&binary) {
        return Ok(Classification {
            permission: "host.read",
            resource: format!("host:{host}"),
            operation: "read",
            seconds: READ_LEASE_SECONDS,
        });
    }
    if WRITE_BINARIES.contains(&binary) {
        let target = if binary == "/usr/bin/systemctl" {
            argv.get(2)
        } else {
            argv.get(1)
        }
        .map(String::as_str)
        .unwrap_or("");
        return Ok(Classification {
            permission: "host.write",
            resource: format!("host:{host}/{binary}/{target}"),
            operation: "write",
            seconds: WRITE_LEASE_SECONDS,
        });
    }
    Err(ARCPError::PermissionDenied {
        detail: format!("binary not allowed: {binary}"),
    })
}

/// Send `permission.request`, await `permission.grant` / `permission.deny`,
/// return the lease id.
async fn acquire_lease(
    _client: &Client,
    _c: &Classification,
    _reason: &str,
) -> Result<String, ARCPError> {
    // payload = {permission, resource, operation, reason,
    //   requested_lease_seconds: c.seconds}
    todo!()
}

async fn run_command(
    client: &Client,
    argv: Vec<String>,
    reason: &str,
    host: &str,
) -> Result<String, ARCPError> {
    let c = classify(&argv, host)?;
    let lease = acquire_lease(client, &c, reason).await?;
    // The lease is the only guard. Spawn the subprocess elsewhere.
    Ok(format!("<would run {argv:?} under lease {lease}>"))
}

async fn emit_thought(
    _client: &Client,
    _stream_id: &str,
    _sequence: u32,
    _text: &str,
) -> Result<(), ARCPError> {
    // client.send(envelope("stream.chunk", stream_id,
    //   payload={sequence, kind: "thought", role: "assistant_thought", content}))
    todo!()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client: Client = todo!(); // transport, identity (constrained), auth elided
    let stream_id = "str_<unix>";
    // client.send(envelope("stream.open", stream_id, payload={kind: "thought"}))

    let incident = "api-gateway pod is OOMing every 4 minutes";
    let mut seq: u32 = 0;
    while let Some(step) = next_step(incident, None).await {
        emit_thought(&client, stream_id, seq, &step.thought).await?;
        seq += 1;
        if let Some(tc) = step.tool_call {
            match run_command(&client, tc.argv, &tc.reason, "edge-pod-04").await {
                Ok(_) | Err(ARCPError::PermissionDenied { .. }) => continue,
                Err(other) => return Err(other.into()),
            }
        }
        if let Some(answer) = step.final_ {
            println!("{answer}");
            break;
        }
    }
    Ok(())
}
