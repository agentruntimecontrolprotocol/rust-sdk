//! Boot three Observer clients on a single producing session.
//!
//! Three filters, three sinks. Demonstrates RFC §5 Observer role and
//! §13 subscriptions / filters / unsubscribe.

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

mod sinks;

use arcp::error::ARCPError;
use arcp::transport::MemoryTransport;
use arcp::{ARCPClient, Envelope};

use crate::sinks::{OtlpSink, SqliteSink, StdoutSink};

type Client = ARCPClient<MemoryTransport>;

const STDOUT_TYPES: &[&str] = &[
    "log",
    "job.started",
    "job.progress",
    "job.completed",
    "job.failed",
    "tool.error",
];
const OTLP_TYPES: &[&str] = &["metric", "trace.span"];

/// Build a `subscribe` envelope with `filter={"session_id": [target],
/// "types": types}`, send it, await `subscribe.accepted`, return the
/// runtime-issued `subscription_id`.
async fn subscribe(
    _client: &Client,
    _session_id: &str,
    _types: Option<&[&str]>,
) -> Result<String, ARCPError> {
    todo!()
}

async fn unsubscribe(_client: &Client, _subscription_id: &str) -> Result<(), ARCPError> {
    // client.send(envelope("unsubscribe", subscription_id=...))
    todo!()
}

/// Return the inner envelope from a `subscribe.event`, or `None` to skip.
fn unwrap_event(_envelope: &Envelope) -> Option<Envelope> {
    // env.payload["event"] -> Envelope::from_wire(...)
    todo!()
}

/// Open one observer client; subscribe; pump inner envelopes to `handler`
/// until the stream closes; tear down cleanly.
async fn attach<F, Fut>(_types: Option<&[&str]>, _handler: F) -> Result<(), ARCPError>
where
    F: Fn(Envelope) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let client: Client = todo!(); // transport, identity, auth elided
    let target_session = "<producer-session-id>";
    let sub_id = subscribe(&client, target_session, _types).await?;
    // for await env in client.events():
    //     if let Some(inner) = unwrap_event(&env) { _handler(inner).await }
    unsubscribe(&client, &sub_id).await
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let stdout = StdoutSink;
    let sqlite = SqliteSink {
        path: "replay.sqlite".into(),
    };
    let otlp = OtlpSink {
        endpoint: "<otlp-endpoint>".into(),
    };

    let (a, b, c) = tokio::join!(
        attach(Some(STDOUT_TYPES), |e| async { stdout.handle(e).await }),
        attach(None, |e| async { sqlite.handle(e).await }),
        attach(Some(OTLP_TYPES), |e| async { otlp.handle(e).await }),
    );
    a?;
    b?;
    c?;
    Ok(())
}
