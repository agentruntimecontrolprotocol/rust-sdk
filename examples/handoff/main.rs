//! Cheap-tier first; escalate to deep tier via agent.handoff.
//!
//! Transcript travels as an artifact (RFC §16); `agent.handoff` carries
//! the runtime fingerprint pinned (RFC §8.3). RFC §14.

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

mod cheap;

use arcp::error::ARCPError;
use arcp::transport::MemoryTransport;
use arcp::{ARCPClient, ErrorCode};
use serde_json::{json, Value};

use crate::cheap::attempt;

type Client = ARCPClient<MemoryTransport>;

const CONFIDENCE_THRESHOLD: f64 = 0.65;
const CHEAP_URL: &str = "wss://haiku-pool.tier1.internal";
const DEEP_URL: &str = "wss://opus-pool.tier3.internal";
const DEEP_KIND: &str = "arcp-opus-pool";
const DEEP_FINGERPRINT: &str = "sha256:0a37bf7d61cca21f00...";

async fn package_context(_client: &Client, _transcript: Value) -> Result<Value, ARCPError> {
    // body = canonical_json(transcript)
    // reply = client.request(envelope("artifact.put",
    //   payload={artifact_id, media_type: "application/json",
    //     size, sha256, data: base64(body)}), timeout=15s)
    // if reply.type != "artifact.ref": Err(Internal)
    // -> reply.payload (the artifact ref)
    todo!()
}

async fn emit_handoff(
    _client: &Client,
    _artifact_ref: Value,
    _trace_id: &str,
) -> Result<(), ARCPError> {
    // client.send(envelope("agent.handoff", trace_id,
    //   payload={target_runtime: {url: DEEP_URL, kind: DEEP_KIND,
    //     fingerprint: DEEP_FINGERPRINT}, session_id, shared_memory_ref}))
    todo!()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // transport=WebSocketTransport(CHEAP_URL), pinned to arcp-haiku-pool.
    let cheap: Client = todo!();
    // accepted = cheap.open()
    // if accepted.runtime.kind != "arcp-haiku-pool": Err(Unauthenticated)

    let request = "what does CRDT stand for?";
    let trace_id = "trace_<uuid>";

    let (answer, confidence) = attempt(request).await;
    if confidence >= CONFIDENCE_THRESHOLD {
        println!("{answer}");
    } else {
        let artifact = package_context(
            &cheap,
            json!({
                "user_request": request,
                "transcript": [
                    {"role": "user", "content": request},
                    {"role": "assistant", "content": answer},
                ],
                "cheap_confidence": confidence,
            }),
        )
        .await?;
        emit_handoff(&cheap, artifact, trace_id).await?;
        println!("[handed off to {DEEP_KIND} (URL={DEEP_URL}) trace_id={trace_id}]");
    }
    Ok(())
}
