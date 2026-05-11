//! Fan `human.input.request` across channels; resolve on first.
//!
//! Phone, email, Slack — first answer wins, losers told to settle.
//! Deadline elapsed → translate to `human.input.cancelled` (RFC §12.4).

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

mod channels;

use std::time::Duration;

use arcp::error::ARCPError;
use arcp::transport::MemoryTransport;
use arcp::{ARCPClient, Envelope};
use chrono::{DateTime, Utc};
use futures::future::{select_all, FutureExt};
use serde_json::Value;

use crate::channels::dispatch;

type Client = ARCPClient<MemoryTransport>;

const DESTINATIONS: &[&str] = &["ntfy:phone", "email:oncall", "slack:ops"];

fn parse_iso(_v: &str) -> DateTime<Utc> {
    todo!()
}

async fn fan_out(_client: &Client, request: Envelope) -> Result<(), ARCPError> {
    // Pull `prompt`, `response_schema`, `expires_at` from request.payload.
    let prompt = "<prompt>";
    let schema: Value = todo!();
    let expires_at: DateTime<Utc> = todo!();
    let timeout = (expires_at - Utc::now())
        .to_std()
        .unwrap_or(Duration::from_millis(0));

    let mut tasks: Vec<_> = DESTINATIONS
        .iter()
        .map(|d| {
            let d = (*d).to_string();
            let s = schema.clone();
            tokio::spawn(async move {
                let v = dispatch(&d, prompt, &s).await;
                (d, v)
            })
            .boxed()
        })
        .collect();

    let winner = match tokio::time::timeout(timeout, select_all(&mut tasks)).await {
        Ok((res, _idx, _rest)) => res.ok(),
        Err(_) => None,
    };

    let _request_id = request.id;
    match winner {
        None => {
            // Deadline elapsed; translate to cancelled-input shape (RFC §12.4).
            // client.send(envelope("human.input.cancelled",
            //   correlation_id=request.id,
            //   payload={code: "DEADLINE_EXCEEDED",
            //     message: "no channel responded before expires_at"}))
            todo!()
        }
        Some((responded_by, value)) => {
            // client.send(envelope("human.input.response",
            //   correlation_id=request.id,
            //   payload={value, responded_by, responded_at}))
            //
            // Tell losers the question is settled.
            // client.send(envelope("human.input.cancelled",
            //   correlation_id=request.id,
            //   payload={code: "OK", message: "answered elsewhere",
            //     channels: losers}))
            let _ = (responded_by, value);
            Ok(())
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _client: Client = todo!(); // transport, identity, auth elided
                                   // for await env in client.events():
                                   //   if env.type == "human.input.request":
                                   //     spawn fan_out(client, env)
    todo!()
}
