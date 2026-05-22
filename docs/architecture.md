# Architecture

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="diagrams/architecture-dark.svg">
  <img alt="ARCP Rust SDK architecture" src="diagrams/architecture-light.svg">
</picture>

The Rust SDK ships one crate, `arcp`, with both a library target and the
`arcp` CLI binary. It is not a Cargo workspace, so there is no `docs/crates/`
or module catalog to mirror.

## Layers

```
arcp
├── client      typed client session API
├── runtime     server-side runtime, tool dispatch, jobs, leases
├── messages    ARCP v1.1 payload structs and enums
├── envelope    wire envelope and metadata
├── transport   memory, WebSocket, stdio
├── auth        bearer, signed JWT, anonymous
├── store       SQLite event and credential ledgers
└── extensions  core vs. vendor-extension classification
```

Rustdoc remains the symbol-level reference at <https://docs.rs/arcp>. These
pages explain how the pieces fit together and when to use them.

## Client

`ARCPClient` owns a transport and returns a typed `Session`. The session uses
Rust type-state: `Session<Unauthenticated>` can only authenticate, while
`Session<Authenticated>` exposes protocol operations such as invocation,
artifacts, subscriptions, and close.

This turns the ARCP session boundary into a compile-time API boundary.

## Runtime

`ARCPRuntime` accepts transports and creates per-session state. It owns:

- `ToolRegistry` for application tool handlers.
- `JobRegistry` for pending, running, and terminal job state.
- `EventLog` for replay and resume.
- `ArtifactStore` for inline artifact put/fetch/release.
- `SubscriptionManager` for cross-session event fanout.
- `CredentialLedger` and optional `CredentialProvisioner` for lease-bound credentials.

Build a runtime with `ARCPRuntime::builder()`, then call
`serve_connection(transport)` for each accepted peer.

## Messages and envelopes

All standard ARCP payloads live under `arcp::messages`. `MessageType` is the
typed enum for core message variants, and `Envelope` carries the shared wire
metadata: protocol version, id, session id, job id, correlation id, event
sequence, priority, trace id, and extension object.

For forward-compatible inspection, `RawEnvelope` preserves unknown payloads and
vendor extension types.

## Transports

`arcp::transport::Transport` is the common async frame interface. Built-in
implementations:

- `MemoryTransport` for tests and in-process demos.
- `WebSocketTransport` behind `transport-ws`.
- `StdioTransport` behind `transport-stdio`.

See [transports.md](./transports.md).

## Feature flags

| Feature | Default | Effect |
| --- | --- | --- |
| `transport-ws` | yes | Enables `tokio-tungstenite` WebSocket transport and `arcp serve`. |
| `transport-stdio` | yes | Enables newline-delimited JSON stdio transport. |

## Persistence

The event log uses `rusqlite` with bundled SQLite. The runtime can use an
in-memory database for tests or a file-backed database for restart-tolerant
resume and audit trails.

## Where to read code

- Library entry point: [`src/lib.rs`](../src/lib.rs)
- Client API: [`src/client/api.rs`](../src/client/api.rs)
- Runtime: [`src/runtime/server.rs`](../src/runtime/server.rs)
- Tool context: [`src/runtime/context.rs`](../src/runtime/context.rs)
- Message payloads: [`src/messages/`](../src/messages/)
- Transports: [`src/transport/`](../src/transport/)
- Event log: [`src/store/eventlog.rs`](../src/store/eventlog.rs)
