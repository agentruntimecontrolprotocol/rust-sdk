//! ARCP v1.1 §6.3 + §8.4 — stream-resume: client side.
//!
//! Demonstrates the disconnect → resume → reassemble flow for a chunked
//! streaming result:
//!
//!   Session 1: connect, submit a `long-form` agent job, receive some
//!              `result_chunk` events, then drop the transport without
//!              sending `session.bye`.  The session ID remains valid for
//!              the runtime's resume window.
//!
//!   Session 2: call `client.resume()` with the session ID from session 1,
//!              the single-use `resume_token` from the `session.welcome`
//!              message, and the highest `event_seq` observed so far.
//!              The runtime replays every envelope with seq > last_seq from
//!              its `EventLog` so the client receives the chunks it missed.
//!
//! Both sessions write into a shared `chunks` map keyed by `chunk_seq`.
//! Overwrites from the replay are intentional — they deduplicate the resume
//! boundary without extra bookkeeping.
//!
//! Highlights:
//!   - §6.3  session resume with `resume_token` + `last_event_seq`
//!   - §8.4  chunked `result_chunk` events and `result_size` / `result_id`
//!   - §6.3  `EventLog` server-side replay of missed envelopes
//!
//! Run (after starting the server):
//!     `cargo run --example stream-resume-client`

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

use std::collections::BTreeMap;

use arcp::error::ARCPError;
use arcp::messages::Welcome;
use arcp::transport::MemoryTransport;
use arcp::ARCPClient;
use serde_json::{json, Value};

type Client = ARCPClient<MemoryTransport>;

/// Receive `result_chunk` events and append them to `chunks`.
///
/// Updates `last_seq` to the highest `event_seq` seen.
/// Returns when the `job.result` terminal event arrives.
async fn drain_chunks(
    _client: &Client,
    _chunks: &mut BTreeMap<u64, String>,
    _last_seq: &mut u64,
) -> Result<(String, u64), ARCPError> {
    // for await env in client.events():
    //   if env.type == "job.event" && env.payload.kind == "result_chunk":
    //     chunks.insert(env.payload.body.chunk_seq, env.payload.body.data.clone());
    //     if let Some(seq) = env.event_seq { *last_seq = seq; }
    //   elif env.type == "job.result":
    //     return Ok((env.payload.result_id, env.payload.result_size))
    todo!()
}

/// Submit the `long-form` agent job and return the job ID.
async fn submit_long_form(_client: &Client, _topic: &str) -> Result<String, ARCPError> {
    // client.request(envelope("tool.invoke", {
    //   agent: "long-form",
    //   input: { topic },
    // })) -> job_id from job.accepted
    todo!()
}

/// Resume a dropped session.
///
/// `session_id`   — copied from the first session's `client.state().id`
/// `resume_token` — single-use token from the first `session.welcome`
/// `last_seq`     — highest `event_seq` observed before the drop
///
/// Returns the new `session.welcome` with a fresh (rotated) `resume_token`.
async fn resume_session(
    _client: &Client,
    _session_id: &str,
    _resume_token: &str,
    _last_seq: u64,
) -> Result<Welcome, ARCPError> {
    // client.resume(ResumeRequest {
    //   session_id,
    //   resume_token,
    //   last_event_seq: last_seq,
    // }).await
    todo!()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // One shared map accumulates chunks from both sessions.
    let mut chunks: BTreeMap<u64, String> = BTreeMap::new();
    let mut last_seq: u64 = 0;

    // ── Session 1: submit and receive a prefix of chunks ────────────────
    let client1: Client = todo!(); // transport, identity, auth elided
    let welcome1: Welcome = todo!(); // from client1.connect()

    let _job_id = submit_long_form(&client1, "urban heat islands").await?;

    // Simulate a partial receive: sleep briefly then drop the transport
    // WITHOUT sending session.bye so the session ID stays valid.
    tokio::time::sleep(std::time::Duration::from_millis(800)).await;
    // client1.transport().close_raw("simulated network drop").await?;
    let _ = drain_chunks(&client1, &mut chunks, &mut last_seq).await; // may error on drop

    println!("session 1 dropped after {} chunks (last_seq={last_seq})", chunks.len());

    // ── Session 2: resume and collect the rest ───────────────────────────
    let client2: Client = todo!(); // fresh transport, same identity + auth

    let session_id   = todo!(); // client1.state().session_id
    let resume_token = welcome1.resume_token.as_deref().unwrap_or_default();
    let _welcome2    = resume_session(&client2, session_id, resume_token, last_seq).await?;

    let (result_id, result_size) = drain_chunks(&client2, &mut chunks, &mut last_seq).await?;
    println!("result_id={result_id}  result_size={result_size}");

    // Reassemble: BTreeMap ordering by chunk_seq gives the correct order.
    // Duplicate chunks from the replay boundary are silently overwritten.
    let article: String = chunks.values().cloned().collect();
    println!("assembled {} chars", article.len());

    println!("done");
    Ok(())
}
