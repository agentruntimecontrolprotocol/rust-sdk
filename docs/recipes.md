# Recipes

Copy-paste solutions to common Rust SDK tasks. Complete runnable versions live
in [`examples/`](../examples/).

## In-process runtime for tests

```rust
use arcp::auth::NoneAuthenticator;
use arcp::messages::Capabilities;
use arcp::runtime::ARCPRuntime;
use arcp::transport::paired;

# async fn make_pair() -> Result<(), Box<dyn std::error::Error>> {
let caps = Capabilities {
    anonymous: Some(true),
    ..Default::default()
};
let runtime = ARCPRuntime::builder()
    .with_authenticator(Box::new(NoneAuthenticator::new()))
    .with_capabilities(caps)
    .build()
    .await?;

let (client_transport, runtime_transport) = paired();
let _server = runtime.serve_connection(runtime_transport);
# let _ = client_transport;
# Ok(())
# }
```

## Run a local WebSocket runtime

```sh
cargo run -- serve --bind 127.0.0.1:7777 --bearer tok --principal me
```

Pair it with a client or one of the WebSocket examples.

## Enforce a budget in tool code

```rust
# use arcp::runtime::context::ToolContext;
# async fn call(ctx: &ToolContext) -> Result<(), arcp::ARCPError> {
ctx.charge("cost.llm", 0.03, "USD").await?;
# Ok(())
# }
```

When the matching `cost.budget` counter is exhausted, the helper returns
`BUDGET_EXHAUSTED` and the runtime can surface a terminal job failure.

## Enforce model use

```rust
# use arcp::runtime::context::ToolContext;
# async fn call(ctx: &ToolContext) -> Result<(), arcp::ARCPError> {
ctx.enforce_model_use("tier-fast/small").await?;
# Ok(())
# }
```

Use this before calling an LLM gateway when the job lease includes `model.use`.

## Issue lease-bound credentials

Implement `CredentialProvisioner` when an upstream service needs a short-lived
credential derived from the accepted lease. The runtime redacts credential
values from debug output, subscriptions, and job inventory responses, then
revokes outstanding ids on job termination.

See [`examples/provisioned_credentials/`](../examples/provisioned_credentials/).

## Recover after reconnect

Use a file-backed `EventLog` and persist `(session_id, resume_token,
last_event_seq)` in your application state. On reconnect, pass that tuple in
the resume handshake and replay events after `last_event_seq`.

See [`examples/resumability/`](../examples/resumability/).
