# arcp

[![crates.io](https://img.shields.io/crates/v/arcp.svg)](https://crates.io/crates/arcp)
[![docs.rs](https://docs.rs/arcp/badge.svg)](https://docs.rs/arcp)

Rust reference implementation of the **Agent Runtime Control Protocol (ARCP) v1.1**.

`arcp` is the **umbrella crate** that bundles the three primary crates in this workspace:

| Crate | What it ships |
| --- | --- |
| [`arcp-core`](https://crates.io/crates/arcp-core) | Wire-format envelopes, message payloads, errors, IDs, transport trait, authenticator trait. |
| [`arcp-client`](https://crates.io/crates/arcp-client) | `ARCPClient` + type-state `Session` for the consumer side. |
| [`arcp-runtime`](https://crates.io/crates/arcp-runtime) | `ARCPRuntime` + job machinery, SQLite store, bearer / JWT / none validators, `arcp` CLI. |

Most users want this crate — it gives you both sides of the wire with a single dependency.

```toml
[dependencies]
arcp = "2"
```

To slim builds, opt out of the side you don't need:

```toml
arcp = { version = "2", default-features = false, features = ["client", "transport-ws"] }
```

See the [workspace README](https://github.com/agentruntimecontrolprotocol/rust-sdk#readme) for quick-start examples, recipes, and conformance notes.

## License

Licensed under either of [Apache-2.0](../../LICENSE-APACHE) or [MIT](../../LICENSE-MIT) at your option.
