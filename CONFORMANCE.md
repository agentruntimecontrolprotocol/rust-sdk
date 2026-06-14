# Conformance

This is the authoritative ARCP v1.1 conformance summary for the Rust SDK.
The docs mirror is [`docs/conformance.md`](./docs/conformance.md).

## v1.1 Coverage

| Spec section | Status | Rust SDK coverage |
| --- | --- | --- |
| §4 Transport | Partial | WebSocket, stdio, and in-memory transports are implemented. HTTP/2 and QUIC are deferred. |
| §5 Wire format | Full | `Envelope`, `RawEnvelope`, typed payloads, ids, priorities, trace metadata, and extension fields. |
| §6 Sessions | Full | `session.open` / `accepted` / `rejected` / `unauthenticated` / `close`, lease, ack, heartbeat, job listing, and subscriptions. |
| §6.1 Authentication | Partial | Bearer, signed JWT, and anonymous auth are implemented. mTLS and OAuth2 are deferred to vendor or future adapters. |
| §6.3 Resume | Full | `session.accepted` issues a rotating `resume_token`; `session.resume {resume_token, last_event_seq}` reattaches the session, replays buffered events with `seq > last_event_seq` from the SQLite event log, rotates the token, and acks `session.resumed`. Stale tokens and uncovered sequences return `RESUME_WINDOW_EXPIRED`. |
| §6.4 Heartbeat | Full | `session.ping` / `session.pong` messages and runtime examples are implemented. |
| §6.5 Ack | Full | Runtime ack window and `session.ack` back-pressure are implemented. |
| §6.6 List jobs / Subscribe | Partial | Runtime: `session.list_jobs`, `job.subscribe`, `job.unsubscribe`, and generic subscription fanout are implemented. Client API does not yet expose `list_jobs`/`job.subscribe` as first-class `Session` methods. |
| §7 Jobs | Full | Submit, accept, start, complete, fail, cancel, state inventory, and idempotent retry paths. |
| §7.3 State machine | Full | `JobRegistry` tracks pending, running, and terminal states. |
| §7.4 Cancellation | Full | Cooperative cancellation uses `CancellationToken` and emits cancelled terminal outcomes. |
| §7.5 Agent versions | Full | `AgentRef` parsing and version-aware resolution are covered by unit tests and examples. |
| §8 Job events | Full | Reserved event kinds plus vendor event classification are implemented. |
| §8.2.1 Progress | Full | `JobProgressPayload` carries the spec body `{ current, total?, units?, message? }` with validation rejecting negative/non-finite `current`, negative `total`, and `current > total`. |
| §8.3 Sequence numbers | Full | Event log and runtime fanout preserve per-session event sequence ordering. |
| §8.4 Result chunks | Full | The runtime enforces the §8.4 invariants: monotonic `chunk_seq` from 0 per `result_id`, no chunk after the terminal (`more:false`) chunk, per-chunk/total size caps, and a terminal `job.completed` that MUST carry the matching `result_id` (no inline result after streaming). |
| §9 Leases | Full | Lease request, session lease, subset validation, budgets, model use, and revocation helpers. |
| §9.5 Lease expiration | Full | `lease.expires_at` is validated as future-at-submission (`INVALID_REQUEST` otherwise) and enforced during execution: a job still running at `expires_at` is preempted with `LEASE_EXPIRED` (`retryable: false`). Renewal is not supported. |
| §9.6 Budgets | Full | `cost.budget` tracking emits remaining metrics and returns `BUDGET_EXHAUSTED` on depletion. |
| §9.7 Model use | Full | `model.use` glob enforcement is implemented in `ToolContext`. |
| §9.8 Provisioned credentials | Full | Issue, redact, attach, revoke, and ledger outstanding credentials. |
| §10 Delegation | Full | Delegate and handoff examples cover child job requests and lease subset enforcement. |
| §11 Observability | Partial | Trace ids and metric/log event payloads are implemented. Native OpenTelemetry middleware is deferred. |
| §12 Error taxonomy | Full | Canonical codes serialize to wire strings and map to `ARCPError` variants, including the §12 wire codes `DUPLICATE_KEY`, `AGENT_NOT_AVAILABLE`, `JOB_NOT_FOUND`, `TIMEOUT`, `RESUME_WINDOW_EXPIRED`, and `INVALID_REQUEST`. The runtime emits `DUPLICATE_KEY` on idempotency conflict and `AGENT_NOT_AVAILABLE` for unregistered agents. |
| §14 Security | Partial | Auth, lease checks, credential redaction, back-pressure caps, and §14 same-principal subscription scoping (generic `subscribe` defaults to the caller's principal; cross-principal filters return `PERMISSION_DENIED`) are implemented. Host-framework DNS-rebind helpers are out of scope for this crate. |
| §15 IANA / extensions | Full | `arcpx.*` and reverse-DNS extension namespace validation, advertised-vs-unknown classification, and round-trip handling are implemented in `ExtensionRegistry`. |
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
cargo check --workspace --all-targets --all-features
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
# Publish dry-run iterates the workspace in dependency order:
for crate in arcp-core arcp-client arcp-runtime arcp arcp-tower arcp-axum arcp-actix-web arcp-otel; do
    cargo publish --dry-run -p "$crate"
done
```

## Deferred Surfaces

- HTTP/2 and QUIC transports.
- Native mTLS and OAuth2 authenticators.
- Native OpenTelemetry middleware.
- Sidecar binary stream frames outside the JSON envelope path.
- Scheduled jobs, workflow orchestration, and trust elevation beyond the v1.1 core.
