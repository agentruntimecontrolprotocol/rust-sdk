# arcp

Rust reference implementation of the **Agent Runtime Control Protocol (ARCP)** v1.0.

The protocol is defined in [`RFC-0001-v2.md`](./RFC-0001-v2.md), which ships
inside the crate. The crate's job is to make that document executable.

> **Status:** v0.1, built across seven hard-gated phases (see
> [`PLAN.md`](./PLAN.md) and the `phase N: ...` commits). Per-section RFC
> status lives in [`CONFORMANCE.md`](./CONFORMANCE.md).

## What works today

- Envelope and message-type surface (RFC §6, §7) for every in-scope variant
- Four-step authenticated handshake (§8.1) with `bearer`, `signed_jwt`,
  and `none` schemes
- Capability negotiation (intersection on accept)
- Tool dispatch through a [`ToolHandler`](src/runtime/tools.rs) trait
  that receives a [`ToolContext`](src/runtime/context.rs) carrying the
  cancel token plus `request_human_input` / `request_human_choice`
  round-trips (§10, §12)
- Cooperative cancellation via `tokio_util::sync::CancellationToken`,
  surfaced as `job.cancelled` on the wire when the handler honours it
- SQLite event log (§6.4 idempotency, §13.3 backfill, §19 resume groundwork)
- Artifact store (§16) with inline base64 put/fetch + retention sweep
- Subscription manager (§13) with filter engine (session/trace/job/stream/
  type/min-priority)
- Two real transports (§22): WebSocket via `tokio-tungstenite` and stdio
  via newline-delimited JSON over `AsyncRead` / `AsyncWrite`
- 86 tests, all five gates clean across feature-flag combinations

## Quickstart

```bash
cargo build
cargo run -- version

# Run an example
cargo run --example 01_minimal_session
cargo run --example 02_tool_invoke

# Run a server (defaults: 127.0.0.1:7777, anonymous)
cargo run -- serve
# Or with a bearer token:
cargo run -- serve --bearer secret-token --principal alice@example.com
```

## Architecture

```
+---------------------------+
|       arcp::client        |   ARCPClient — type-state Session<S>
+---------------------------+
|       arcp::runtime       |   ARCPRuntime — server, ToolContext, jobs,
|                           |   subscriptions, leases (typed), artifacts,
|                           |   pending registry
+---------------------------+
|       arcp::messages      |   tagged-enum MessageType + per-domain payloads
+---------------------------+
|  arcp::transport (trait)  |   websocket | stdio | memory (test)
+---------------------------+
|       arcp::store         |   rusqlite-backed event log (idempotency, replay)
+---------------------------+
```

## Crate features

| Feature           | Default | Notes                                              |
| ----------------- | ------- | -------------------------------------------------- |
| `transport-ws`    | yes     | WebSocket transport via `tokio-tungstenite`        |
| `transport-stdio` | yes     | Newline-delimited JSON over `tokio::io::stdin/out` |

## Type-system invariants

The crate leans on Rust's type system to lift protocol invariants out of
the runtime:

- `Session<Unauthenticated>` cannot send protocol traffic; only
  `Session<Authenticated>` (returned by `.authenticate()`) exposes
  `.invoke()` etc. — RFC §4.6 ("authenticated by default") is a compile error.
- `MessageType` is `#[serde(tag = "type", content = "payload")]`, so
  `cargo build` enforces an exhaustive match on dispatch.
- IDs are newtypes (`SessionId`, `MessageId`, `JobId`, …) that cannot be
  mixed at the call site.

## What's intentionally deferred to v0.2

See [`CONFORMANCE.md`](./CONFORMANCE.md) for the full per-section status.
The big items: HTTP/2 + QUIC transports, mTLS + OAuth2 auth schemes,
sidecar binary stream frames, scheduled jobs, multi-agent delegation /
handoff / workflow primitives, trust elevation, checkpoint-based resume,
heartbeat watchdog, hard-kill cancel escalation.

## License

MIT OR Apache-2.0
