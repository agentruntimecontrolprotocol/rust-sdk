//! Capability-driven peer routing with ordered fallback + cost rollup.
//!
//! Walks an ordered chain of peer runtimes per request class. Marketplace
//! fields ride on the negotiated capabilities (RFC §21 namespace) so no
//! extra round trip is needed to learn cost / latency / class. Retryable
//! errors fall through to the next peer; everything else surfaces.
//!
//! Demonstrates RFC §7, §17.3.1, §18.3.

// Examples are illustrative, not runnable: setup is elided with `todo!()` and
// the protocol shape is what the reader sees. Suppress the lints that would
// otherwise force unidiomatic skeleton code.
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

use std::collections::HashMap;

use arcp::error::ARCPError;
use arcp::messages::Capabilities;
use arcp::transport::MemoryTransport;
use arcp::{ARCPClient, Envelope, ErrorCode};
use serde_json::json;

/// Concrete transport pinned for the snippet — production uses `WebSocketTransport`
/// per peer URL.
type Client = ARCPClient<MemoryTransport>;

const PEERS: &[&str] = &[
    "anthropic-haiku",
    "anthropic-sonnet",
    "openai-4o",
    "groq-llama",
];
const COST_CEILING_USD_PER_MTOK: f64 = 8.0;
const LATENCY_CEILING_MS: u32 = 800;

fn fallback_chain(class: &str) -> &'static [&'static str] {
    match class {
        "cheap_fast" => &["groq-llama", "anthropic-haiku", "openai-4o"],
        "balanced" => &["anthropic-sonnet", "openai-4o", "anthropic-haiku"],
        "deep" => &["anthropic-sonnet"],
        _ => &[],
    }
}

#[derive(Debug, Clone, Copy)]
struct Profile {
    cost_per_mtok: f64,
    p50_latency_ms: u32,
}

/// Capabilities is `extra="allow"` at the wire — namespaced fields ride
/// alongside the core booleans (RFC §7 / §21).
fn profile_from(_caps: &Capabilities) -> Profile {
    // Read `arcpx.market.cost_per_mtok.v1`, `arcpx.market.p50_latency_ms.v1`,
    // `arcpx.market.model_class.v1` from the extension namespace.
    todo!()
}

fn candidate_chain(profiles: &HashMap<&str, Profile>, class: &str) -> Vec<&'static str> {
    fallback_chain(class)
        .iter()
        .copied()
        .filter(|name| {
            profiles.get(name).is_some_and(|p| {
                p.cost_per_mtok <= COST_CEILING_USD_PER_MTOK
                    && p.p50_latency_ms <= LATENCY_CEILING_MS
            })
        })
        .collect()
}

const fn is_retryable(code: ErrorCode) -> bool {
    matches!(
        code,
        ErrorCode::ResourceExhausted
            | ErrorCode::Unavailable
            | ErrorCode::DeadlineExceeded
            | ErrorCode::Aborted
    )
}

/// Walk `chain`. On a retryable wire error or a `tool.error` reply with a
/// retryable code, try the next peer; on a hard error, surface it. The
/// `extensions={"arcpx.market.peer.v1": <name>}` block on each invoke lets
/// downstream observers tell which peer ultimately answered.
async fn invoke_with_fallback(
    _clients: &HashMap<&str, Client>,
    chain: &[&str],
    _tool: &str,
    _arguments: serde_json::Value,
    _trace_id: &str,
) -> Result<Envelope, ARCPError> {
    let mut last: Option<ARCPError> = None;
    for _name in chain {
        let reply: Result<Envelope, ARCPError> = todo!();
        match reply {
            Ok(env) => return Ok(env),
            Err(exc) => {
                let code = exc.code();
                last = Some(exc);
                if is_retryable(code) {
                    continue;
                }
                return Err(last.expect("set above"));
            }
        }
    }
    Err(last.unwrap_or_else(|| ARCPError::Unavailable {
        detail: "no peers available".into(),
    }))
}

#[derive(Debug, Default)]
struct Usage {
    tokens_in: u64,
    tokens_out: u64,
    cost_usd: f64,
    by_peer: HashMap<String, f64>,
}

/// Subscribe to each peer's `metric` envelopes; aggregate `tokens.used`
/// (with `kind=input|output`) and `cost.usd` (with `peer=<name>`) into a
/// per-tenant rollup. Standard names from RFC §17.3.1.
fn consume_metric(_env: &Envelope, _totals: &mut HashMap<String, Usage>) {
    todo!()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut clients: HashMap<&str, Client> = HashMap::new();
    let mut profiles: HashMap<&str, Profile> = HashMap::new();
    for name in PEERS {
        let client: Client = todo!(); // transport per peer URL, identity, auth elided
        let caps: Capabilities = todo!(); // client.negotiated_capabilities()
        profiles.insert(name, profile_from(&caps));
        clients.insert(name, client);
    }

    let trace_id = "trace_<uuid>";
    let chain = candidate_chain(&profiles, "balanced");
    let _reply = invoke_with_fallback(
        &clients,
        &chain,
        "chat.completion",
        json!({"prompt": "Hello", "tenant": "acme-corp"}),
        trace_id,
    )
    .await?;
    println!("invoked balanced chain across {} peers", chain.len());
    Ok(())
}
