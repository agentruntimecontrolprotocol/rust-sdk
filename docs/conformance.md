# Conformance

This page mirrors the authoritative root
[`CONFORMANCE.md`](../CONFORMANCE.md). Keep detailed release decisions there;
use this page as the docs index entry for spec coverage.

## Summary

The Rust SDK implements the primary ARCP v1.1 client/runtime surfaces:

- Wire envelopes, ids, typed payloads, and vendor extension handling.
- Session authentication, ack, heartbeat, listing, subscriptions, and resume.
- Job lifecycle, cancellation, result chunks, artifacts, and event fanout.
- Leases, budgets, model-use constraints, provisioned credentials, and subset validation.
- WebSocket, stdio, and in-memory transports.

Deferred surfaces are HTTP/2, QUIC, native mTLS/OAuth2 authenticators, native
OpenTelemetry middleware, and larger orchestration primitives outside the v1.1
core.

## Verification

Run the release gate locally:

```sh
cargo fmt --all -- --check
cargo check --all-targets --all-features
cargo test --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo publish --dry-run
```

Report conformance deviations with the spec section, expected behavior,
observed behavior, and a minimal Rust reproducer.
