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
arcp = "1.1"
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
serde_json = "1"
```

Default features include WebSocket and stdio transports. Disable defaults if
you only need the typed protocol surface:

```toml
arcp = { version = "1.1", default-features = false }
```

## Run the CLI runtime

Start a WebSocket runtime with bearer authentication:

```sh
cargo run -- serve --bind 127.0.0.1:7777 --bearer secret-token --principal alice@example.com
```

For local demos that do not need authentication, omit `--bearer`; the runtime
advertises anonymous auth.

## Run examples

Examples are the fastest way to see client/runtime flows. Some are compact
illustrations with setup elided; the integration tests under [`tests/`](../tests/)
show fully exercised paths.

```sh
cargo run --example session_ack
cargo run --example job_subscribe
cargo run --example cost_budget
cargo run --example provisioned_credentials
cargo run --example agent_versions
```

These all use the in-memory transport so they run without a network port.
Transport-specific examples cover stdio (`stdio.rs`), Axum hosting
(`axum_server.rs`), and the canonical WebSocket flow via the CLI runtime
above. The single-file examples under [`examples/`](../examples/) (e.g.
`cancellation.rs`, `lease_expires_at.rs`) are illustrative — they elide
transport setup with `let client: Client = todo!();` and are meant to be
read alongside the runnable directory-based examples rather than executed
directly.

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
