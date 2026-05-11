//! Durable research job with real crash and resume.
//!
//! ```text
//! # First call: crash after `synthesize`. Prints the resume token.
//! CRASH_AFTER_STEP=synthesize \
//!   cargo run --example resumability
//!
//! # Second call: pick up from the printed checkpoint.
//! RESUME_JOB_ID=...  RESUME_AFTER_MSG_ID=...  RESUME_CHECKPOINT_ID=... \
//!   cargo run --example resumability
//! ```

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

mod steps;

use std::env;
use std::process;

use arcp::error::ARCPError;
use arcp::transport::MemoryTransport;
use arcp::{ARCPClient, ErrorCode};
use serde_json::{json, Value};

use crate::steps::run_step;

type Client = ARCPClient<MemoryTransport>;

const STEPS: &[&str] = &["plan", "gather", "synthesize", "critique", "finalize"];

/// Deterministic per-step idempotency key (RFC §6.4). Re-issuing the
/// same step with the same input returns the prior outcome instead of
/// re-running the LLM.
fn step_key(_job_id: &str, _step: &str, _salt: &str) -> String {
    // sha256(job_id || step || salt) -> "research:{job_id}:{step}:{hex16}"
    todo!()
}

async fn emit_progress(_client: &Client, _job_id: &str, _step: &str) -> Result<(), ARCPError> {
    // pct = 100 * (idx+1)/len(STEPS)
    // client.send(envelope("job.progress", job_id, payload={percent, message: step}))
    todo!()
}

async fn emit_checkpoint(_client: &Client, _job_id: &str, step: &str) -> Result<String, ARCPError> {
    let chk = format!("chk_{step}_<job_suffix>");
    // client.send(envelope("job.checkpoint", job_id,
    //   payload={checkpoint_id: chk, label: step}))
    Ok(chk)
}

async fn execute_steps(
    client: &Client,
    job_id: &str,
    request: Value,
    starting_at: &str,
    crash_after: Option<&str>,
) -> Result<Value, ARCPError> {
    let start_idx = STEPS.iter().position(|s| *s == starting_at).unwrap_or(0);
    let mut output = request;
    for (i, step) in STEPS.iter().enumerate().skip(start_idx) {
        let _key = step_key(job_id, step, &output.to_string());
        emit_progress(client, job_id, step).await?;
        // output = run_step(client, job_id, step,
        //   inputs={prior: output, idempotency_key: key}).await?
        output = json!({"step": step, "i": i});
        let _chk = emit_checkpoint(client, job_id, step).await?;
        if crash_after == Some(*step) {
            // The whole point of durable jobs: process death is fine.
            // Runtime kept every envelope; resume picks it up.
            println!(
                "[crash after {step}; resume with RESUME_JOB_ID={job_id} \
                 RESUME_CHECKPOINT_ID=chk_{step}_<job_suffix> \
                 RESUME_AFTER_MSG_ID=<last id from your event log>]"
            );
            process::exit(137);
        }
    }
    Ok(output)
}

/// Replay envelopes; return the last checkpoint label, or `None` if the
/// job already terminated during replay.
async fn issue_resume(
    _client: &Client,
    _job_id: &str,
    _after_message_id: &str,
    _checkpoint_id: Option<&str>,
) -> Result<Option<String>, ARCPError> {
    // payload = {after_message_id, include_open_streams: true}
    // if checkpoint_id: payload["checkpoint_id"] = checkpoint_id
    // client.send(envelope("resume", job_id, payload))
    //
    // for await env in client.events():
    //   if env.job_id != job_id: continue
    //   if env.type == "tool.error" and code == DATA_LOSS: Err(DataLoss)
    //   if env.type == "job.checkpoint": last = env.payload["label"]
    //   if env.type in TERMINAL: return None
    //   if env.type == "event.emit" and name == "subscription.backfill_complete":
    //       return Some(last)
    todo!()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client: Client = todo!();

    let resume_job = env::var("RESUME_JOB_ID").ok();
    let resume_after = env::var("RESUME_AFTER_MSG_ID").ok();
    if let (Some(rj_id), Some(rj_after)) = (resume_job, resume_after) {
        let last = issue_resume(
            &client,
            &rj_id,
            &rj_after,
            env::var("RESUME_CHECKPOINT_ID").ok().as_deref(),
        )
        .await?;
        match last {
            None => println!("already terminal during replay"),
            Some(label) => {
                let next = STEPS.iter().position(|s| **s == label).unwrap_or(0) + 1;
                if next >= STEPS.len() {
                    println!("nothing to resume");
                } else {
                    println!("[resuming at {}]", STEPS[next]);
                    let final_ =
                        execute_steps(&client, &rj_id, json!("<replayed>"), STEPS[next], None)
                            .await?;
                    // client.send(envelope("job.completed", job_id=rj_id,
                    //   payload={result: final_}))
                    println!("done: {final_}");
                }
            }
        }
    } else {
        let job_id = "job_<uuid>";
        let request = "Survey CRDT-based collaborative editing in 2026.";
        // client.send(envelope("workflow.start", job_id,
        //   payload={workflow: "research.v1", arguments: {request}}))
        let final_ = execute_steps(
            &client,
            job_id,
            json!(request),
            STEPS[0],
            env::var("CRASH_AFTER_STEP").ok().as_deref(),
        )
        .await?;
        // client.send(envelope("job.completed", job_id, payload={result: final_}))
        println!("job_id={job_id}\n{final_}");
    }
    Ok(())
}
