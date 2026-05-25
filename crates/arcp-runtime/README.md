# arcp-runtime

[![crates.io](https://img.shields.io/crates/v/arcp-runtime.svg)](https://crates.io/crates/arcp-runtime)
[![docs.rs](https://docs.rs/arcp-runtime/badge.svg)](https://docs.rs/arcp-runtime)

Reference runtime (server side) for the **Agent Runtime Control Protocol (ARCP) v1.1**.

Ships the production-ready runtime — the part that accepts sessions, dispatches messages, runs tools, manages jobs / streams / permissions / leases / subscriptions, and persists what needs persisting:

- `runtime::ARCPRuntime` — main entrypoint.
- `store` — SQLite-backed append-only event log + credential ledger.
- `auth::{BearerAuthenticator, SignedJwtAuthenticator, NoneAuthenticator}` — RFC §8.2 validators.
- `arcp` binary — configurable demo runtime over stdio or WebSockets.

```toml
[dependencies]
arcp-runtime = "2"
arcp-core = "2"
```

Most users should depend on the umbrella [`arcp`](https://crates.io/crates/arcp) crate which bundles client + runtime + core. Pull `arcp-runtime` in directly when hosting agents (you don't need the client side).

## License

Licensed under either of [Apache-2.0](../../LICENSE-APACHE) or [MIT](../../LICENSE-MIT) at your option.
