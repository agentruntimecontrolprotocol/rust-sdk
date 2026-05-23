# ARCP Rust examples

Fourteen single-purpose codebases, each named for the protocol primitive
it demonstrates. Mirrors the Python tree at
[`python-sdk/examples/`](https://github.com/agentruntimecontrolprotocol/python-sdk/tree/main/examples).

> **Illustrative, not runnable.** Each example imports the in-repo `arcp`
> crate as if it were a published `arcp = "1"`. Setup boilerplate
> (transport URL, identity, auth) is elided with `let client: Client =
> todo!();`. LLM and framework calls live in tiny stub modules
> (`agents.rs`, `steps.rs`, `synth.rs`, ...) so the protocol code in
> `main.rs` is what you read.

## The fourteen

| Example | Demonstrates | Spec |
|---|---|---|
| [`subscriptions/`](./subscriptions) | Three Observer clients on one session, three filters, three sinks. | §5, §13 |
| [`leases/`](./leases) | Lease-gated shell agent. Read leases coarse, write leases scoped. | §15.4–§15.5 |
| [`lease_revocation/`](./lease_revocation) | Per-table leases with `lease.revoked` / `lease.extended` mid-flight. | §15.5 |
| [`permission_challenge/`](./permission_challenge) | Two-party challenge — generator asks, reviewer holds veto. | §15.4, §6.4 |
| [`delegation/`](./delegation) | `agent.delegate` fan-out + a `JobMux` over `tokio::sync::mpsc` to demux events by `job_id`. | §14, §6.4 |
| [`handoff/`](./handoff) | `agent.handoff` with transcript packed as an artifact, runtime fingerprint pinned. | §14, §16, §8.3 |
| [`heartbeats/`](./heartbeats) | Worker federation; heartbeat-loss reroute via `idempotency_key`. | §10.3, §6.4 |
| [`capability_negotiation.rs`](./capability_negotiation.rs) | Capability-driven peer routing; standard `cost.usd` rollups. | §7, §17.3.1, §18.3 |
| [`resumability/`](./resumability) | **Real crash and resume.** `std::process::exit(137)` mid-flight; second invocation picks up at the next step. | §10, §19, §6.4 |
| [`reasoning_streams/`](./reasoning_streams) | `kind: thought` stream + a peer runtime that subscribes and delegates critiques back. | §11.4, §13, §14 |
| [`extensions.rs`](./extensions.rs) | Custom `arcpx.sdr.*.v1` extension namespace + unknown-message handling. | §21 |
| [`cancellation.rs`](./cancellation.rs) | Cooperative `cancel` (terminate) vs `interrupt` (pause and ask). | §10.4–§10.5 |
| [`mcp/`](./mcp) | ARCP runtime fronting an MCP server: `tool.invoke` → MCP `call_tool`. | §20 |

## Conventions

- Rust 2021, formatted with `cargo fmt`, clippy-clean under
  `cargo clippy --examples -- -D warnings`.
- Each example is one `main.rs` (the protocol code) + 0–2 stub modules
  named for what they elide (`agents.rs`, `steps.rs`, `cheap.rs`,
  `synth.rs`, `work.rs`, `channels.rs`, `sql.rs`, `upstream.rs`).
- Single-file examples sit at `examples/<name>.rs` and run with
  `cargo run --example <name>`. Multi-file examples live at
  `examples/<name>/main.rs` with stub siblings; declared as
  `[[example]]` in the workspace `Cargo.toml`.
- `let client: Client = todo!();` literally — transport, identity, auth
  blocks are setup noise, not the point. Each example's allow header
  permits the lints (`clippy::todo`, `unused_variables`, ...) that
  illustration-grade code would otherwise trip.
- Envelopes match RFC-0001 v2 exactly. Custom message types follow
  §21.1 `arcpx.<domain>.<name>.v<n>` naming.

## What's where in the SDK

- `arcp::ARCPClient<T: Transport>` — handshake driver.
- `arcp::Envelope`, `arcp::ErrorCode`, `arcp::error::ARCPError` — wire
  primitives.
- `arcp::messages::Capabilities` — negotiated capability bag.
- `arcp::transport::{paired, MemoryTransport, WebSocketTransport,
  StdioTransport}` — connection-time scaffolding.
- `arcp::store::eventlog` — SQLite schema reused by `subscriptions`.

## Reading order

For a brisk tour: `subscriptions`, `leases`, `delegation`,
`resumability` (this one actually crashes and recovers), `cancellation`,
`extensions`, `mcp`. These seven exercise the bulk of the protocol.

## Numbered companions

`01_minimal_session.rs` and `02_tool_invoke.rs` predate this index;
they're a runnable end-to-end against the in-process runtime + memory
transport. The fourteen above are illustrative.
