# Conformance

This is the authoritative ARCP v1.1 conformance summary for the Rust SDK.
The docs mirror is [`docs/conformance.md`](./docs/conformance.md).

## v1.1 Coverage

| Spec section | Status | Rust SDK coverage |
| --- | --- | --- |
| §4 Transport | Partial | WebSocket, stdio, and in-memory transports are implemented. HTTP/2 and QUIC are deferred. |
| §5 Wire format | Full | `Envelope`, `RawEnvelope`, typed payloads, ids, priorities, trace metadata, and extension fields. |
| §6 Sessions | Full | Session open/accepted/rejected/unauthenticated, bye, lease, ack, heartbeat, job listing, and subscriptions. |
| §6.1 Authentication | Partial | Bearer, signed JWT, and anonymous auth are implemented. mTLS and OAuth2 are deferred to vendor or future adapters. |
| §6.3 Resume | Full | SQLite event log supports replay by session and sequence boundary; resumability examples exercise reconnect flows. |
| §6.4 Heartbeat | Full | `session.heartbeat` / `session.pong` messages and runtime examples are implemented. |
| §6.5 Ack | Full | Runtime ack window and `session.ack` back-pressure are implemented. |
| §6.6 List jobs / Subscribe | Full | `session.list_jobs`, `job.subscribe`, `job.unsubscribe`, and generic subscription fanout are implemented. |
| §7 Jobs | Full | Submit, accept, start, complete, fail, cancel, state inventory, and idempotent retry paths. |
| §7.3 State machine | Full | `JobRegistry` tracks pending, running, and terminal states. |
| §7.4 Cancellation | Full | Cooperative cancellation uses `CancellationToken` and emits cancelled terminal outcomes. |
| §7.5 Agent versions | Full | `AgentRef` parsing and version-aware resolution are covered by unit tests and examples. |
| §8 Job events | Full | Reserved event kinds plus vendor event classification are implemented. |
| §8.2.1 Progress | Full | Progress events are emitted by examples and represented in message types. |
| §8.3 Sequence numbers | Full | Event log and runtime fanout preserve per-session event sequence ordering. |
| §8.4 Result chunks | Full | `result.chunk` stream assembly and terminal result references are implemented. |
| §9 Leases | Full | Lease request, session lease, subset validation, expiration, budgets, model use, and revocation helpers. |
| §9.6 Budgets | Full | `cost.budget` tracking emits remaining metrics and returns `BUDGET_EXHAUSTED` on depletion. |
| §9.7 Model use | Full | `model.use` glob enforcement is implemented in `ToolContext`. |
| §9.8 Provisioned credentials | Full | Issue, redact, attach, revoke, and ledger outstanding credentials. |
| §10 Delegation | Full | Delegate and handoff examples cover child job requests and lease subset enforcement. |
| §11 Observability | Partial | Trace ids and metric/log event payloads are implemented. Native OpenTelemetry middleware is deferred. |
| §12 Error taxonomy | Full | Canonical codes serialize to wire strings and map to `ARCPError` variants. |
| §14 Security | Partial | Auth, lease checks, credential redaction, and back-pressure caps are implemented. Host-framework DNS-rebind helpers are out of scope for this crate. |
| §15 Vendor extensions | Full | `x-vendor.*` validation, classification, and round-trip handling are implemented. |
| §16 Artifacts | Full | Inline base64 artifact put, fetch, release, retention, and runtime dispatch are implemented. |
| §22 Standard transports | Partial | WebSocket and stdio are implemented; HTTP/2 and QUIC are deferred. |

## Test Coverage

The conformance surface is covered by:

- Unit tests under `src/` for ids, envelopes, errors, permissions, result chunks, stores, transports, and runtime helpers.
- Integration tests under [`tests/`](./tests/) for handshakes, runtime dispatch, job lifecycle, subscriptions, cancellation, artifacts, leases, budgets, credentials, resume, result chunks, and WebSocket loopback.
- Runnable examples under [`examples/`](./examples/) that exercise user-facing flows across the same v1.1 features.

Before release, run:

```sh
cargo fmt --all -- --check
cargo check --all-targets --all-features
cargo test --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo publish --dry-run
```

## Deferred Surfaces

- HTTP/2 and QUIC transports.
- Native mTLS and OAuth2 authenticators.
- Native OpenTelemetry middleware.
- Sidecar binary stream frames outside the JSON envelope path.
- Scheduled jobs, workflow orchestration, and trust elevation beyond the v1.1 core.
