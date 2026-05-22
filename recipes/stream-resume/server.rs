//! ARCP v1.1 §6.3 + §8.4 — stream-resume: runtime / server side.
//!
//! A long-form writer agent streams a generated article through ARCP's
//! chunked-result primitive.  The runtime persists every emitted envelope
//! in an `EventLog` under the session's monotonic `event_seq`, which lets
//! a client reconnect after a transport drop and replay the chunks it
//! missed (see the companion `client.rs` for the resume flow).
//!
//! The writer batches LLM token deltas into ~200-character chunks before
//! calling `stream.write()`, keeping the `EventLog` readable without
//! flooding it with single-token envelopes.  `stream.finalize()` emits
//! the terminal `job.result` with `result_id` and `result_size`; inline
//! `result` MUST NOT be used in chunked mode.
//!
//! Highlights:
//!   - §8.4  `ctx.stream_result()` → `write()` per batch → `finalize()`
//!   - §6.3  `EventLog` + `resume_window_seconds` wiring for resumable sessions
//!   - §6.3  transport drop leaves session ID valid within the resume window
//!
//! Run:
//!     `cargo run --example stream-resume-server`

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
use arcp::{ARCPRuntime, EventLog, JobContext};
use serde_json::{json, Value};

type Runtime = ARCPRuntime<MemoryTransport>;

/// Flush threshold: emit one `result_chunk` envelope per ~200 characters.
///
/// Smaller values produce finer-grained replay at the cost of a larger
/// `EventLog`; larger values reduce envelope count but increase the amount
/// replayed after a resume.
const FLUSH_BYTES: usize = 200;

/// Long-form writer: stream a generated article in `result_chunk` batches.
///
/// Uses GLM-5 (or any OpenAI-compatible streaming endpoint) to produce
/// the article, batches the deltas, and calls `stream.write()` for each
/// batch so the `EventLog` records every chunk with a monotonic `event_seq`.
/// The final batch is passed to `stream.finalize()` which emits the
/// terminating `job.result` carrying `result_id` and `result_size`.
async fn long_form_agent(
    _input: &Value,
    _ctx: &mut JobContext<'_>,
) -> Result<(), ARCPError> {
    // let topic = _input["topic"].as_str().unwrap_or("general interest");
    // let stream = _ctx.stream_result();
    // let mut buf = String::new();
    //
    // // GLM-5 via z.ai OpenAI-compatible API — swap baseURL for other GLM
    // // providers; the streaming API shape stays the same.
    // let glm = openai::Client::new_with_base_url("https://api.z.ai/api/paas/v4/");
    // let completion = glm.chat()
    //   .model("glm-5")
    //   .stream(true)
    //   .message("user", format!("Write a 2000-word article on: {topic}"))
    //   .send_stream().await?;
    //
    // for await chunk in completion {
    //   let delta = chunk.choices[0].delta.content.unwrap_or_default();
    //   if delta.is_empty() { continue; }
    //   buf.push_str(&delta);
    //   if buf.len() >= FLUSH_BYTES {
    //     stream.write(&buf).await?;
    //     buf.clear();
    //   }
    // }
    //
    // // finalize emits job.result with result_id + result_size;
    // // never call stream.write() after finalize.
    // stream.finalize(&buf, json!({ "summary": format!("Article on {topic}") })).await?;
    todo!()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // EventLog + resume_window_seconds are required for session resume.
    // Without them a dropped transport is treated as a closed session.
    let event_log = EventLog::new();

    let runtime: Runtime = todo!(); // transport, identity, auth; EventLog + window elided

    // runtime.set_event_log(event_log);
    // runtime.set_resume_window(std::time::Duration::from_secs(60));
    // runtime.register_agent("long-form", |input, ctx| Box::pin(long_form_agent(input, ctx)));
    // runtime.serve("127.0.0.1:7901").await?;

    println!("stream-resume runtime ready");
    Ok(())
}
