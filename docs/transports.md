# Transports

ARCP transports carry ordered envelopes between a client and a runtime. The
Rust SDK includes in-memory, WebSocket, and stdio implementations.

## In-memory

`arcp::transport::paired()` returns two connected `MemoryTransport` halves:
sending on one arrives on the other.

```rust
use arcp::transport::paired;

let (client_side, runtime_side) = paired();
```

Use it for:

- Unit and integration tests.
- Single-process examples.
- Embedding a runtime beside a client without serialization overhead.

It is the fastest path for application tests, but it does not exercise JSON
round-tripping or network behavior.

## WebSocket

WebSocket is the production-friendly default for networked runtimes. It is
enabled by the `transport-ws` feature.

The CLI hosts a basic runtime:

```sh
cargo run -- serve --bind 127.0.0.1:7777 --bearer secret-token
```

Programmatic hosts accept an upgraded stream and wrap it with
`WebSocketTransport::accept_stream`. See [`src/bin/arcp.rs`](../src/bin/arcp.rs)
and [`examples/axum_server.rs`](../examples/axum_server.rs).

Use WebSocket for:

- Remote runtimes.
- Long-lived jobs with streaming events.
- Resume and reconnect behavior under real network failure.

## stdio

Stdio is newline-delimited JSON over an async reader/writer. It is enabled by
the `transport-stdio` feature and is useful when a parent process starts a
runtime subprocess.

Use stdio for:

- Local agents spawned by editors or CLIs.
- Operating-system sandbox boundaries.
- Tests that need process isolation without a TCP listener.

Keep logs on stderr. Anything written to stdout that is not an ARCP envelope can
corrupt the channel.

## Choosing a transport

| Need | Transport |
| --- | --- |
| Fast in-process tests | In-memory |
| Remote runtime over TLS | WebSocket |
| Parent process supervising a child runtime | stdio |
| Host-framework integration | WebSocket adapter in the host |

## Custom transports

Implement the `Transport` trait when you need a host-specific channel. Preserve
frame order, propagate close, and return transport errors as `ARCPError`
variants at the protocol boundary.
