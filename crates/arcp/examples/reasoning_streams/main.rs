//! Primary emits reasoning; mirror peer subscribes, critiques back.
//!
//! The mirror is a peer runtime, NOT a pure observer — it reads the
//! `kind: thought` stream AND delegates critique events back via
//! `agent.delegate`. RFC §11.4 / §13 / §14.

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

mod agents;

use arcp::error::ARCPError;
use arcp::transport::MemoryTransport;
use arcp::{ARCPClient, Envelope};
use serde_json::Value;
use tokio::sync::mpsc;

use crate::agents::{critique_thought, primary_step};

type Client = ARCPClient<MemoryTransport>;

const MAX_DEPTH: u32 = 3;
const TOKEN_BUDGET: u64 = 8_000;

// Primary side ---------------------------------------------------------

async fn run_primary(
    _client: &Client,
    request: &str,
    mut inbound_critiques: mpsc::Receiver<Value>,
) -> String {
    let stream_id = "str_<uuid>";
    // client.send(envelope("stream.open", stream_id, payload={kind: "thought"}))

    let mut last: Option<Value> = None;
    let mut answer = String::new();
    for step in 0..MAX_DEPTH {
        answer = primary_step(request, last.as_ref()).await;
        // client.send(envelope("stream.chunk", stream_id,
        //   payload={sequence: step, kind: "thought",
        //     role: "assistant_thought", content: answer}))
        let _ = (stream_id, step);
        match tokio::time::timeout(std::time::Duration::from_secs(5), inbound_critiques.recv())
            .await
        {
            Ok(Some(crit)) => {
                if crit.get("severity").and_then(Value::as_str) == Some("halt") {
                    break;
                }
                last = Some(crit);
            }
            _ => last = None,
        }
    }
    answer
}

// Mirror side ----------------------------------------------------------

async fn subscribe_thoughts(
    _mirror: &Client,
    _target_session_id: &str,
) -> Result<String, ARCPError> {
    // accepted = mirror.request(envelope("subscribe", payload={
    //   filter: {session_id: [target], types: ["stream.chunk"]}}), timeout=10s)
    // -> accepted.payload["subscription_id"]
    todo!()
}

fn is_thought(env: &Envelope) -> bool {
    // env.type == "stream.chunk" and (kind == "thought" or role == "assistant_thought")
    let _ = env;
    todo!()
}

async fn run_mirror(_mirror: &Client, _target_session_id: &str) {
    // sub_id = subscribe_thoughts(...)
    // for await env in mirror.events():
    //   inner = env.payload["event"] -> Envelope::from_wire
    //   if !is_thought(inner): continue
    //   if spent >= TOKEN_BUDGET:
    //     mirror.send(envelope("unsubscribe", subscription_id=sub_id))
    //     return
    //   crit = critique_thought(inner.content).await
    //   mirror.send(envelope("agent.delegate", target=target_session_id,
    //     payload={target: "primary", task: "consume_critique",
    //       context: {critique: {target_thought_sequence, severity,
    //                            summary, suggestion, consumed_tokens}}}))
    todo!()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let primary: Client = todo!();
    let mirror: Client = todo!();

    let (tx, rx) = mpsc::channel::<Value>(32);

    // Route inbound `agent.delegate` critiques into `tx`.
    tokio::spawn(async move {
        let _tx = tx;
        // for await env in primary.events():
        //   if env.type == "agent.delegate":
        //     critique = env.payload["context"]["critique"]
        //     if isinstance(critique, dict): tx.send(critique).await
    });

    // Mirror runs for main()'s lifetime.
    tokio::spawn(async move {
        run_mirror(&mirror, "<primary-session-id>").await;
    });

    let answer = run_primary(
        &primary,
        "Argue both sides: serializable vs snapshot iso?",
        rx,
    )
    .await;
    println!("{answer}");
    Ok(())
}
