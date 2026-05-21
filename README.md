# arcp

Rust reference implementation of the **Agent Runtime Control Protocol (ARCP)** v1.1.

The protocol is defined in [the ARCP spec](https://github.com/agentruntimecontrolprotocol/spec/blob/main/docs/draft-arcp-1.1.md).
The crate's job is to make that document executable.

> **Status:** v0.1, built across seven hard-gated phases (see the
> `phase N: ...` commits). Per-section spec status lives in
> [`CONFORMANCE.md`](./CONFORMANCE.md).

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
- Subscriptions wired through the runtime: `Session::subscribe(filter)`
  returns a `SubscriptionHandle` that yields live envelopes from any
  session sharing the runtime; cross-connection delivery is verified.
- Artifacts wired through the runtime: `Session::put_artifact` /
  `fetch_artifact` / `release_artifact` round-trip end-to-end.
- 134 tests, all five gates clean across feature-flag combinations
- 85% line coverage via `cargo llvm-cov` — see [`scripts/coverage.sh`](./scripts/coverage.sh)

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

# Coverage report (requires rustup + llvm-tools-preview component)
cargo install cargo-llvm-cov
rustup component add llvm-tools-preview
scripts/coverage.sh                 # human-readable summary
scripts/coverage.sh --html          # HTML report under target/llvm-cov
```

## Architecture

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/diagrams/architecture-dark.svg">
  <img alt="ARCP Rust SDK architecture — arcp::client and arcp::runtime exchange tagged-enum messages (arcp::messages) over arcp::transport (websocket/stdio/memory); arcp::runtime is backed by the rusqlite event log arcp::store" src="docs/diagrams/architecture-light.svg">
</picture>

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
