//! ARCP v1.1 §10 — vendor-extension job event kinds.
//!
//! Event kinds that start with `x-` are vendor-defined.  A compliant
//! client MUST NOT reject envelopes with unknown kinds; it may silently
//! ignore them or handle them if it understands the vendor namespace.
//!
//! This example demonstrates two receiver behaviours (both valid):
//!
//!   - **Naïve receiver**: only knows the reserved kinds
//!     (`status`, `log`, `thought`, `metric`, `tool_call`, `tool_result`,
//!     `artifact_ref`, `delegate`).  Unknown kinds are logged and skipped.
//!
//!   - **Acme-aware receiver**: additionally handles
//!     `x-vendor.acme.progress` — a vendor event with `{percent, eta_seconds}`.
//!
//! Both handlers run against the same stream so you can compare them.
//!
//! Run with:
//!     `cargo run --example vendor_extensions`

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
use arcp::ARCPClient;
use serde_json::{json, Value};

type Client = ARCPClient<MemoryTransport>;

/// All reserved `job.event` kinds defined by the ARCP specification.
const RESERVED_KINDS: &[&str] = &[
    "status",
    "log",
    "thought",
    "metric",
    "tool_call",
    "tool_result",
    "artifact_ref",
    "delegate",
];

/// Submit the `render-job` tool (emits `x-vendor.acme.progress` events).
/// Returns the job ID from `job.accepted`.
async fn submit_render_job(_client: &Client) -> Result<String, ARCPError> {
    // client.request(envelope("tool.invoke", {
    //   tool: "render-job",
    //   arguments: {frames: 4},
    //   lease_request: {
    //     resources: {
    //       "net.fetch": ["https://assets.example.com/**"],
    //       "x-vendor.acme.metrics": ["acme:render/*"],
    //     }
    //   },
    // })) -> job_id from job.accepted
    todo!()
}

/// Receive all events for `job_id`.  Applies both the naïve and the
/// acme-aware handler to every envelope.
///
/// Returns `(result, acme_rendered, naive_skipped)`.
async fn drain(_client: &Client, _job_id: &str) -> Result<(Value, u32, u32), ARCPError> {
    let mut acme_rendered: u32 = 0;
    let mut naive_skipped: u32 = 0;

    // for await env in client.events():
    //   if env.job_id != job_id { continue }
    //   if env.type == "job.completed" { return Ok((env.payload.result, acme_rendered, naive_skipped)) }
    //   if env.type == "job.failed"    { return Err(ARCPError::from(env.payload)) }
    //   if env.type != "job.event"     { continue }
    //
    //   let kind = env.payload.kind.as_str();
    //
    //   // ── Naïve handler ──────────────────────────────────────────────────
    //   if RESERVED_KINDS.contains(&kind) {
    //     println!("[naive] event[seq={}] {} {}", env.event_seq, kind,
    //              serde_json::to_string(&env.payload.body)?);
    //   } else {
    //     naive_skipped += 1;
    //     println!("[naive] event[seq={}] unknown kind {:?} — ignoring", env.event_seq, kind);
    //   }
    //
    //   // ── Acme-aware handler ─────────────────────────────────────────────
    //   if kind == "x-vendor.acme.progress" {
    //     let percent     = env.payload.body["percent"].as_f64().unwrap_or(0.0);
    //     let eta_seconds = env.payload.body["eta_seconds"].as_f64().unwrap_or(0.0);
    //     let filled = "#".repeat((percent / 5.0).round() as usize).into();
    //     let bar: String = format!("{:<20}", filled).replace(' ', ".");
    //     println!("[acme]  [{}] {:.0}% (eta {:.0}s)", bar, percent, eta_seconds);
    //     acme_rendered += 1;
    //   }

    todo!()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client: Client = todo!(); // transport, identity, auth elided

    let job_id = submit_render_job(&client).await?;
    println!("accepted: job_id={job_id}");

    let (result, acme_rendered, naive_skipped) = drain(&client, &job_id).await?;
    println!("result: {result}");
    println!("summary: acme events rendered={acme_rendered}, naive skipped={naive_skipped}");

    if acme_rendered == 0 {
        return Err("expected at least one vendor event".into());
    }
    if naive_skipped == 0 {
        return Err("expected the naive handler to skip at least one event".into());
    }

    println!("done");
    Ok(())
}
