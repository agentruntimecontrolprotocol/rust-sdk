# arcp

Rust reference implementation of the **Agent Runtime Control Protocol (ARCP)** v1.0.

The protocol is defined in [`RFC-0001-v2.md`](./RFC-0001-v2.md), which ships
inside the crate. The crate's job is to make that document executable.

> **Status:** Phase 0 (skeleton). The crate compiles and runs but no protocol
> surfaces are implemented. See [`PLAN.md`](./PLAN.md) for the build roadmap
> and [`CONFORMANCE.md`](./CONFORMANCE.md) for what is actually wired up.

## Quickstart

```bash
cargo build
cargo run -- --help
```

## Architecture (in flight)

```
+---------------------------+
|       arcp::client        |   ARCPClient — type-state Session<S>
+---------------------------+
|       arcp::runtime       |   ARCPRuntime — server, jobs, streams,
|                           |   subscriptions, leases, artifacts, pending registry
+---------------------------+
|       arcp::messages      |   tagged-enum MessageType + per-domain payloads
+---------------------------+
|  arcp::transport (trait)  |   websocket | stdio | memory (test)
+---------------------------+
|       arcp::store         |   rusqlite-backed event log (idempotency, replay)
+---------------------------+
```

## Features

| Feature           | Default | Notes                                              |
| ----------------- | ------- | -------------------------------------------------- |
| `transport-ws`    | yes     | WebSocket transport via `tokio-tungstenite`        |
| `transport-stdio` | yes     | Newline-delimited JSON over `tokio::io::stdin/out` |

## License

MIT OR Apache-2.0
