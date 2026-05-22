# arcp

Rust reference SDK for the **Agent Runtime Control Protocol (ARCP)** v1.1.

[![Crates.io](https://img.shields.io/crates/v/arcp.svg)](https://crates.io/crates/arcp)
[![Docs.rs](https://docs.rs/arcp/badge.svg)](https://docs.rs/arcp)

The protocol is defined in the
[ARCP v1.1 specification](https://github.com/agentruntimecontrolprotocol/spec/blob/main/docs/draft-arcp-1.1.md).
This crate turns that wire contract into typed Rust APIs for clients,
runtimes, transports, persistence, permissions, and testable in-process
workflows.

## Install

```toml
[dependencies]
arcp = "1.1"
```

Default features include WebSocket and stdio transports:

```toml
arcp = { version = "1.1", default-features = false, features = ["transport-ws"] }
```

## Quickstart

Run a local runtime over WebSocket:

```sh
cargo run -- serve --bearer secret-token --principal alice@example.com
```

In another process, run one of the end-to-end examples:

```sh
cargo run --example submit_and_stream
cargo run --example resumability
cargo run --example delegation
```

Use the CLI to confirm the crate and wire versions:

```sh
cargo run -- version
```

## Crate Layout

- `arcp::client` - typed client session APIs.
- `arcp::runtime` - server-side runtime, tool dispatch, jobs, leases, artifacts, subscriptions, and credentials.
- `arcp::messages` - ARCP v1.1 message payloads and capability structures.
- `arcp::envelope` - wire envelope, raw envelope, priority, and metadata.
- `arcp::transport` - in-memory, WebSocket, and stdio transport implementations.
- `arcp::auth` - bearer, signed JWT, and anonymous authenticators.
- `arcp::store` - SQLite-backed event and credential ledgers.
- `arcp::extensions` - core vs. vendor-extension namespace validation.

## Documentation

- [Getting started](./docs/getting-started.md) - install, run, and wire a minimal client/runtime pair.
- [Architecture](./docs/architecture.md) - how the crate modules fit together.
- [Transports](./docs/transports.md) - WebSocket, stdio, and in-memory selection guide.
- [CLI](./docs/cli.md) - `arcp version`, `arcp serve`, and current CLI limits.
- [Guides](./docs/README.md#guides-one-per-spec-section) - spec-aligned guides for sessions, jobs, leases, delegation, errors, and extensions.
- [Conformance](./CONFORMANCE.md) - section-by-section ARCP v1.1 coverage.

## Examples

Runnable examples live in [`examples/`](./examples/):

- `submit_and_stream` - submit a job and consume lifecycle events.
- `resumability` - replay events after reconnecting.
- `job_subscribe` - subscribe to a job from another session.
- `cost_budget` - enforce v1.1 lease budgets.
- `provisioned_credentials` - issue and revoke lease-bound credentials.
- `delegation` and `handoff` - spawn or transfer work across agents.
- `stdio` and `axum_server` - host the runtime over different integration paths.

## Feature Flags

| Feature | Default | Purpose |
| --- | --- | --- |
| `transport-ws` | yes | WebSocket transport via `tokio-tungstenite`. |
| `transport-stdio` | yes | Newline-delimited JSON over `tokio::io`. |

## Conformance

The SDK implements the core ARCP v1.1 surfaces for envelopes, sessions,
authentication, jobs, event replay, subscriptions, cancellation, artifacts,
leases, budgets, provisioned credentials, vendor extensions, and WebSocket /
stdio / in-memory transports. Deferred surfaces are tracked in
[`CONFORMANCE.md`](./CONFORMANCE.md).

## Development

```sh
cargo fmt --all -- --check
cargo check --all-targets --all-features
cargo test --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo publish --dry-run
```

Coverage uses `cargo-llvm-cov`:

```sh
cargo install cargo-llvm-cov
rustup component add llvm-tools-preview
scripts/coverage.sh
```

## License

MIT OR Apache-2.0
