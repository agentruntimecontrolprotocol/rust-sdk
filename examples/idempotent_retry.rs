//! Idempotent job submission with deduplication (RFC §8.3).
//!
//! Clients may attach an `idempotency_key` to a `tool.invoke` request.
//! The runtime deduplicates on `(principal, idempotency_key)` for ~24 h:
//!
//!   1. First call with key K → normal `job.accepted` with a fresh job ID.
//!   2. Retry with the same K and identical `(tool, arguments)` → runtime
//!      returns the **same** `job_id` (the job may already be running or
//!      completed).
//!   3. Same K but different `tool` → `nack` with `DUPLICATE_KEY`.
//!
//! This pattern lets callers retry without risk of running the job twice.
//!
//! Run with:
//!     `cargo run --example idempotent_retry`

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

use arcp::error::{ARCPError, ErrorCode};
use arcp::transport::MemoryTransport;
use arcp::ARCPClient;
use serde_json::json;

type Client = ARCPClient<MemoryTransport>;

/// Submit a job with an optional `idempotency_key`.  Returns `job_id`.
async fn submit(
    _client: &Client,
    _tool: &str,
    _arguments: &serde_json::Value,
    _idempotency_key: Option<&str>,
) -> Result<String, ARCPError> {
    // client.request(envelope("tool.invoke", {
    //   tool: tool,
    //   arguments: arguments,
    //   idempotency_key: idempotency_key,
    // })) -> job_id from job.accepted
    todo!()
}

/// Await the terminal event (`job.completed`, `job.failed`, etc.) for `job_id`.
async fn await_terminal(_client: &Client, _job_id: &str) -> Result<(), ARCPError> {
    todo!()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client: Client = todo!(); // transport, identity, auth elided

    let key = "weekly-report-2026-W19";
    let args = json!({"week": "2026-W19"});

    // --- Submit #1 -------------------------------------------------------------
    let job1 = submit(&client, "weekly-report", &args, Some(key)).await?;
    println!("submit #1 accepted: job_id={job1}");
    await_terminal(&client, &job1).await?;
    println!("submit #1 result: completed");

    // --- Submit #2 (retry with same key + same tool) ---------------------------
    // The runtime MAY return the same job_id or accept a new one that
    // is logically deduplicated.
    let job2 = submit(&client, "weekly-report", &args, Some(key)).await?;
    println!("submit #2 accepted: job_id={job2}");
    if job2 == job1 {
        println!("idempotency confirmed: same job_id returned");
    } else {
        println!("runtime issued new job_id but deduplicated result");
    }

    // --- Submit #3 (same key, different tool) ----------------------------------
    // RFC requires the runtime to reject with DUPLICATE_KEY.
    let result = submit(&client, "different-tool", &args, Some(key)).await;
    match result {
        Err(ARCPError::Custom {
            code: ErrorCode::DuplicateKey,
            ..
        }) => {
            println!("DUPLICATE_KEY nack received as expected — done");
        }
        other => {
            return Err(format!("expected DUPLICATE_KEY but got: {other:?}").into());
        }
    }

    Ok(())
}
