//! ARCP v1.1 — structured tracing with the `tracing` crate.
//!
//! Instruments the ARCP client with `tracing` spans so that every
//! `tool.invoke`, `job.accepted`, and streaming `job.event` envelope
//! is recorded in the trace tree.  Trace context propagates to the
//! runtime via the `extensions["x.otel"]` field (W3C traceparent).
//!
//! This example wires a `tracing_subscriber` fmt layer that writes
//! JSON-structured spans to stdout — swap in an OTLP exporter for
//! production.
//!
//! Run with:
//!     `cargo run --example tracing`
//!
//! Prerequisites in Cargo.toml (illustrative — adapt to actual crate names):
//! ```toml
//! tracing = "0.1"
//! tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
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

use arcp::error::ARCPError;
use arcp::transport::MemoryTransport;
use arcp::{ARCPClient, Envelope};
use serde_json::json;

type Client = ARCPClient<MemoryTransport>;

/// Initialise a `tracing_subscriber` that writes JSON spans to stdout.
///
/// In production, replace `fmt::layer()` with an OTLP exporter:
/// ```rust
/// use opentelemetry_otlp::WithExportConfig;
/// let exporter = opentelemetry_otlp::new_exporter().tonic().with_endpoint("...");
/// ```
fn init_tracing() {
    // tracing_subscriber::registry()
    //   .with(EnvFilter::from_default_env())
    //   .with(fmt::layer().json())
    //   .init();
    //
    // The ARCP SDK picks up the global subscriber automatically.
    // All span/event fields are mirrored into the envelope's
    // `extensions["x.otel"]` when the runtime supports it.
    todo!()
}

/// Submit the `parent` agent job and return its job ID.
#[tracing::instrument(skip(_client))]
async fn submit_parent(_client: &Client) -> Result<String, ARCPError> {
    // client.request(envelope("tool.invoke", {
    //   tool: "parent",
    //   arguments: {item: "widget-42"},
    //   lease_request: {resources: {"agent.delegate": ["child"]}},
    // })) -> job_id from job.accepted
    //
    // The SDK injects the current span's traceparent into
    // extensions["x.otel"] so the runtime can link child spans.
    todo!()
}

/// Drain events for `job_id` and print each one with its trace/job IDs.
#[tracing::instrument(skip(_client))]
async fn drain(_client: &Client, _job_id: &str) -> Result<serde_json::Value, ARCPError> {
    // for await env in client.events():
    //   if env.job_id != job_id { continue }
    //   let span = tracing::info_span!("job.event",
    //     event_seq = env.event_seq,
    //     job_id    = %env.job_id.as_deref().unwrap_or(""),
    //     trace_id  = %env.trace_id.as_deref().unwrap_or("<none>"),
    //     kind      = %env.payload.kind,
    //   );
    //   let _guard = span.enter();
    //   tracing::info!("received event");
    //   if env.type == "job.completed" { return Ok(env.payload.result) }
    //   if env.type == "job.failed"    { return Err(ARCPError::from(env.payload)) }
    todo!()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_tracing();

    let client: Client = todo!(); // transport, identity, auth elided

    let job_id = submit_parent(&client).await?;
    println!("accepted: job_id={job_id}");

    let result = drain(&client, &job_id).await?;
    println!("result: {result}");

    // Allow any trailing child-job events to flush before shutdown.
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    println!("done");
    Ok(())
}
