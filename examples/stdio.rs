//! ARCP v1.1 §5.2 — `StdioTransport`: parent process spawns child runtime.
//!
//! The parent process spawns a child that embeds a minimal ARCP runtime.
//! Communication happens over the child's stdin/stdout; stderr is
//! inherited so child log lines appear in the terminal.
//!
//! Flow:
//!   1. Parent spawns child process with `stdio: pipe, pipe, inherit`.
//!   2. Parent wraps the child's I/O in `StdioTransport`.
//!   3. Full ARCP handshake runs over the pipes.
//!   4. Parent submits an `echo` job and awaits the result.
//!   5. Parent closes the transport; the resulting EOF on the child's
//!      stdin causes the child to shut down gracefully.
//!
//! Run with:
//!     `cargo run --example stdio`

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

use std::process::Stdio;

use arcp::error::ARCPError;
use arcp::transport::{MemoryTransport, StdioTransport};
use arcp::ARCPClient;
use serde_json::json;
use tokio::process::Command;

// For the in-process demo we reuse MemoryTransport; the type alias for
// the stdio-backed variant is shown as a comment.
type Client = ARCPClient<MemoryTransport>;
// type StdioClient = ARCPClient<StdioTransport>;

/// Spawn the child runtime process and return the transport wrapping it.
///
/// In a real deployment the child binary might be `./arcp-server` or a
/// language-specific helper binary; here we use the same `arcp` binary
/// with a `--serve-stdio` flag for illustration.
async fn spawn_child() -> Result<StdioTransport, std::io::Error> {
    let mut child = Command::new("./arcp-server")
        .arg("--serve-stdio")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;

    let stdin = child.stdin.take().expect("piped stdin");
    let stdout = child.stdout.take().expect("piped stdout");

    // StdioTransport::from_child(stdin, stdout, child)
    todo!()
}

/// Submit an `echo` job and await its result.
async fn run_echo(_client: &Client) -> Result<serde_json::Value, ARCPError> {
    // client.request(envelope("tool.invoke", {
    //   tool: "echo",
    //   arguments: {message: "hello from the parent"},
    // })) -> await job.completed -> result
    todo!()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // In a full implementation:
    //   let transport = spawn_child().await?;
    //   let client: StdioClient = ARCPClient::new(transport, identity, auth)?;
    //   let welcome = client.connect().await?;
    //   println!("welcome: session={} runtime={}", client.session_id(), welcome.runtime.name);

    let client: Client = todo!(); // transport, identity, auth elided

    let job_id: String = todo!(); // job_id from job.accepted

    println!("accepted: job_id={job_id}");

    let result = run_echo(&client).await?;
    println!("result: {result}");

    // client.close().await?;
    // child waits for EOF on stdin — the transport close propagates it.

    println!("done");
    Ok(())
}
