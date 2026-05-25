# arcp-core

[![crates.io](https://img.shields.io/crates/v/arcp-core.svg)](https://crates.io/crates/arcp-core)
[![docs.rs](https://docs.rs/arcp-core/badge.svg)](https://docs.rs/arcp-core)

Shared protocol primitives for the **Agent Runtime Control Protocol (ARCP) v1.1**.

This crate ships the parts of ARCP both sides of the wire depend on:

- `envelope` — canonical envelope (RFC §6.1).
- `messages` — payload structs and `MessageType` enum (RFC §6.2).
- `error` — canonical error taxonomy.
- `ids` — strongly-typed opaque IDs (`JobId`, `SessionId`, …).
- `extensions` — extension namespace registry.
- `transport` — `Transport` trait + in-memory transport. WebSocket and stdio transports gated behind features.
- `auth` — `Authenticator` trait. Concrete validators live in [`arcp-runtime`](https://crates.io/crates/arcp-runtime).

```toml
[dependencies]
arcp-core = "2"
```

Most users should depend on the umbrella [`arcp`](https://crates.io/crates/arcp) crate instead. Pull `arcp-core` in directly only when building an alternative client or runtime that needs the protocol primitives without the reference implementations.

## License

Licensed under either of [Apache-2.0](../../LICENSE-APACHE) or [MIT](../../LICENSE-MIT) at your option.
