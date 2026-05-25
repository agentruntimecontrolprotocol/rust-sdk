# Getting started

This walks through a minimal Rust client and runtime. By the end you can run a
local ARCP runtime, submit a job, and inspect streamed events.

## Prerequisites

- Rust **1.88** or newer.
- A Tokio async runtime. The crate uses Tokio for transports, runtime tasks, and
  examples.

## Install

```toml
[dependencies]
arcp = "2"
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
serde_json = "1"
```

Default features include WebSocket and stdio transports plus the `client` and
`runtime` re-exports. Disable defaults if you only need the typed protocol
core (`arcp-core`) and the in-memory transport:

```toml
arcp = { version = "2", default-features = false }
```

## Run the CLI runtime

Start a WebSocket runtime with bearer authentication:

```sh
cargo run -- serve --bind 127.0.0.1:7777 --bearer secret-token --principal alice@example.com
```

For local demos that do not need authentication, omit `--bearer`; the runtime
advertises anonymous auth.

## Run examples

The example tree at [`crates/arcp/examples/`](../crates/arcp/examples/) is
illustrative, not runnable as-is — every example stamps
`#![allow(clippy::todo, ...)]` and uses `let client: Client = todo!();` to
elide transport setup so the code reads as documentation. Running
`cargo run --example <name>` will panic at the first `todo!()`. Use the
examples as annotated walkthroughs paired with the integration tests under
[`crates/arcp/tests/`](../crates/arcp/tests/), which exercise the
end-to-end flows.

The integration tests are the runnable equivalents:

```sh
cargo test --workspace --all-features
```

## Minimal shape

The SDK has two halves:

- `ARCPRuntime` accepts a `Transport`, authenticates the session, and dispatches
  tool invocations.
- `ARCPClient` creates a typed session over a `Transport` and submits work.

The in-memory transport is the simplest test fixture:

```rust
use std::sync::Arc;

use arcp::auth::BearerAuthenticator;
use arcp::messages::{AuthScheme, Capabilities, ClientIdentity, Credentials};
use arcp::runtime::{ARCPRuntime, ToolContext, ToolHandler, ToolRegistryBuilder};
use arcp::transport::paired;
use arcp::ARCPClient;
use async_trait::async_trait;

# async fn demo() -> Result<(), Box<dyn std::error::Error>> {
struct Echo;

#[async_trait]
impl ToolHandler for Echo {
    fn name(&self) -> &'static str {
        "echo"
    }

    async fn invoke(
        &self,
        input: serde_json::Value,
        _ctx: ToolContext,
    ) -> Result<serde_json::Value, arcp::ARCPError> {
        Ok(input)
    }
}

let tools = ToolRegistryBuilder::new().with(Arc::new(Echo)).build();
let runtime = ARCPRuntime::builder()
    .with_authenticator(Box::new(BearerAuthenticator::new().with_token("tok", "alice")))
    .with_capabilities(Capabilities {
        streaming: Some(true),
        ..Default::default()
    })
    .with_tools(tools)
    .build()
    .await?;

let (runtime_transport, client_transport) = paired();
let _server = runtime.serve_connection(runtime_transport);

let client = ARCPClient::new(client_transport);
let session = client
    .open()?
    .authenticate(
        Credentials {
            scheme: AuthScheme::Bearer,
            token: Some("tok".into()),
        },
        ClientIdentity {
            kind: "demo-client".into(),
            version: "1.0.0".into(),
            fingerprint: None,
            principal: None,
        },
        Capabilities::default(),
    )
    .await?;
let job = session
    .invoke("echo", serde_json::json!({ "hello": "arcp" }))
    .await?;
let result = job.join().await?;

assert_eq!(result["hello"], "arcp");
# Ok(())
# }
```

For larger examples, see [`examples/`](../examples/) and the
[jobs guide](./guides/jobs.md).

## Next steps

- [Architecture](./architecture.md) - module map and feature flags.
- [Sessions](./guides/sessions.md) - handshake, auth, ack, heartbeat, and close.
- [Jobs](./guides/jobs.md) - submit, cancel, list, subscribe, and retry.
- [Leases](./guides/leases.md) - budgets, model use, credentials, and subset validation.
