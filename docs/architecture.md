# Architecture

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="diagrams/architecture-dark.svg">
  <img alt="ARCP Rust SDK architecture" src="diagrams/architecture-light.svg">
</picture>

The Rust SDK is a Cargo workspace. The umbrella `arcp` crate re-exports the
protocol core (`arcp-core`), the typed client (`arcp-client`), and the
server-side runtime (`arcp-runtime`); the `arcp` CLI binary ships from
`arcp-runtime`. Reservation stubs (`arcp-tower`, `arcp-axum`,
`arcp-actix-web`, `arcp-otel`) are published at `0.1.0-alpha.0` for the
forthcoming middleware integrations and currently re-export `arcp-core` only.

## Workspace layout

```
arcp/                  umbrella — re-exports core + client + runtime
└── crates/
    ├── arcp-core/     wire types, IDs, envelope, transports, error taxonomy,
    │                  Authenticator trait, extensions registry
    ├── arcp-client/   ARCPClient, type-state Session, JobHandle
    ├── arcp-runtime/  ARCPRuntime, tool dispatch, JobRegistry, EventLog
    │                  (SQLite), SubscriptionManager, CredentialLedger, CLI
    ├── arcp/          umbrella — re-exports the three above
    ├── arcp-tower/    reservation stub (Tower middleware, deferred)
    ├── arcp-axum/     reservation stub (Axum middleware, deferred)
    ├── arcp-actix-web/reservation stub (actix-web middleware, deferred)
    └── arcp-otel/     reservation stub (OpenTelemetry middleware, deferred)
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

Features are declared on the umbrella `arcp` crate. Each feature gates a
re-export of one workspace member or one transport in `arcp-core`.

| Feature | Default | Effect |
| --- | --- | --- |
| `client` | yes | Re-exports `arcp-client` (`ARCPClient`, `Session`). |
| `runtime` | yes | Re-exports `arcp-runtime` (`ARCPRuntime`, event log, CLI). |
| `transport-ws` | yes | Enables the `tokio-tungstenite` WebSocket transport in `arcp-core`. |
| `transport-stdio` | yes | Enables the newline-delimited JSON stdio transport in `arcp-core`. |

Disable defaults to keep only the typed protocol core (`arcp-core`) and the
in-memory transport.

## Persistence

The SQLite event log ships in `arcp-runtime` (via `rusqlite` with bundled
SQLite) and is reached through the umbrella crate's `runtime` feature.
Consumers who pull only `arcp-client` do not get persistence. The runtime
can be configured with an in-memory database for tests or a file-backed
database for restart-tolerant resume and audit trails.

## Where to read code

- Umbrella entry point: [`crates/arcp/src/lib.rs`](../crates/arcp/src/lib.rs)
- Client API: [`crates/arcp-client/src/api.rs`](../crates/arcp-client/src/api.rs)
- Runtime: [`crates/arcp-runtime/src/runtime/server.rs`](../crates/arcp-runtime/src/runtime/server.rs)
- Tool context: [`crates/arcp-runtime/src/runtime/context.rs`](../crates/arcp-runtime/src/runtime/context.rs)
- Message payloads: [`crates/arcp-core/src/messages/`](../crates/arcp-core/src/messages/)
- Transports: [`crates/arcp-core/src/transport/`](../crates/arcp-core/src/transport/)
- Event log: [`crates/arcp-runtime/src/store/eventlog.rs`](../crates/arcp-runtime/src/store/eventlog.rs)
- CLI: [`crates/arcp-runtime/src/bin/arcp.rs`](../crates/arcp-runtime/src/bin/arcp.rs)
