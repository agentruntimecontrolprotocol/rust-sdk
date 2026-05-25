# arcp-client

[![crates.io](https://img.shields.io/crates/v/arcp-client.svg)](https://crates.io/crates/arcp-client)
[![docs.rs](https://docs.rs/arcp-client/badge.svg)](https://docs.rs/arcp-client)

Reference client (consumer side) for the **Agent Runtime Control Protocol (ARCP) v1.1**.

Ships:

- `ARCPClient` — opens sessions over any [`Transport`](https://docs.rs/arcp-core/latest/arcp_core/transport/trait.Transport.html).
- Type-state `Session<Unauthenticated>` / `Session<Authenticated>`.
- `JobHandle`, `SubscriptionHandle` for driving long-running work.

```toml
[dependencies]
arcp-client = "2"
arcp-core = "2"
```

Most users should depend on the umbrella [`arcp`](https://crates.io/crates/arcp) crate which bundles client + runtime + core. Pull `arcp-client` in directly when you only need the consumer side (e.g. CLI tools, agent observers).

## License

Licensed under either of [Apache-2.0](../../LICENSE-APACHE) or [MIT](../../LICENSE-MIT) at your option.
