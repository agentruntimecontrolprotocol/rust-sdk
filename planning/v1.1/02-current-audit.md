# Phase 2 — Current SDK Audit

## TL;DR

**This is not a v1.0 → v1.1 additive bump. The Rust SDK targets a
different protocol from the spec.**

The crate's lib doc, `README.md`, and module docstrings reference
[`RFC-0001-v2.md`](../../RFC-0001-v2.md) — a parallel ARCP draft with a
different wire surface from `../spec/docs/draft-arcp-02.md` (v1.0) and
`draft-arcp-02.1.md` (v1.1) which the TypeScript reference implements.
`src/lib.rs:54` pins `PROTOCOL_VERSION = "1.0"`; the v1.1 envelope rule
is `"arcp": "1"`. `CONFORMANCE.md` and `PLAN.md` are 5-line stubs.
v1.0 conformance against `draft-arcp-02.md` is effectively **0%** at
the wire level.

The migration therefore is a wholesale re-implementation of the wire
surface, the error taxonomy, the session handshake, the job lifecycle,
the lease/permission model, the subscription model, the artifact model,
and the streaming model. A small core of infrastructure carries over:
the ULID-backed `ids` newtype machinery, the SQLite event log
scaffolding, the WebSocket / stdio / Memory transport trait, and the
type-state `Session<S>` discipline. Everything else is a rewrite.

## v1.0 conformance against the spec

Measured against `../spec/docs/draft-arcp-02.md` (the actual v1.0,
which the TS SDK lists in `../typescript-sdk/CONFORMANCE.md`):

| Spec §                          | Required surface                                                                                 | Rust status | Why                                                                                                                                  |
| ------------------------------- | ------------------------------------------------------------------------------------------------ | ----------- | ------------------------------------------------------------------------------------------------------------------------------------ |
| §4 Transport                    | WebSocket, stdio                                                                                 | Adjacent    | WS via `tokio-tungstenite` (`src/transport/websocket.rs`) and stdio (`src/transport/stdio.rs`) exist but were built against `RFC-0001-v2.md`. |
| §5 Wire format                  | `arcp:"1"`, `id` ULID/UUIDv7, `type`, `session_id`, `event_seq`, unknown fields ignored          | Wrong       | `PROTOCOL_VERSION = "1.0"` (`src/lib.rs:54`). Envelope has `arcp: "1.0"`, no `event_seq`, lots of extra fields (`source`, `target`, `stream_id`, `subscription_id`, `causation_id`, etc.) (`src/envelope.rs:56-127`). |
| §6 Sessions                     | `session.hello` / `session.welcome` 2-step handshake; bearer auth                                | Wrong       | 4-step handshake `session.open` → `session.challenge` → `session.authenticate` → `session.accepted` (`src/messages/session.rs`).     |
| §6.3 Resume                     | `session.hello.payload.resume = { session_id, resume_token, last_event_seq }`                    | Missing     | A `Resume(ResumePayload)` exists in `src/messages/control.rs` but the shape is different and the resume-window machinery in `src/runtime/server.rs` is incomplete. |
| §7.1 Job submit                 | `job.submit { agent, input, lease_request?, lease_constraints?, idempotency_key?, max_runtime_sec? }` | Missing | No `job.submit` envelope exists. Rust uses `tool.invoke` as the primary execution command (`src/messages/execution.rs:10`).         |
| §7.1 Job accepted               | `job.accepted { job_id, lease, accepted_at, parent_job_id?, delegate_id?, trace_id? }`           | Wrong       | `JobAcceptedPayload` contains only `{ job_id }` (`src/messages/execution.rs:74`).                                                    |
| §7.3 Lifecycle                  | Single `job.event` stream + terminal `job.result` or `job.error`                                 | Wrong       | Rust emits `job.started`/`job.progress`/`job.heartbeat`/`job.completed`/`job.failed`/`job.cancelled` as separate envelopes (`MessageType` in `src/messages/mod.rs:184`). |
| §7.4 Cancellation               | `job.cancel { reason }` → `job.error { final_status: "cancelled" }` within grace                 | Adjacent    | Rust uses `cancel { target, target_id, reason, deadline_ms }` (`src/messages/control.rs`).                                           |
| §8 Job events                   | `job.event { kind, ts, body }`; kinds `log`/`thought`/`tool_call`/`tool_result`/`status`/`metric`/`artifact_ref`/`delegate` | Missing | Rust has no `job.event` envelope. The eight v1.0 kinds map onto separate top-level messages (`log`, `metric`, `tool.invoke`, ...) — a different model. |
| §8.3 Sequence numbers           | Session-scoped, strictly monotonic, gap-free across reconnects                                   | Partial     | `event_seq` is missing from the envelope. `src/store/eventlog.rs` numbers rows but the sequence is per-event-log row, not per session. |
| §9 Leases                       | Immutable capability map granted at `job.accepted`; namespaces `fs.read`, `fs.write`, `net.fetch`, `tool.call`, `agent.delegate`, `cost.budget` | Missing | Rust has dynamic `permission.request`/`permission.grant`/`permission.deny` flow plus `lease.granted`/`lease.refresh`/`lease.extended`/`lease.revoked` (`src/messages/permissions.rs`). Entirely different authority model. |
| §10 Delegation                  | `delegate` event kind on parent → child `job.accepted` with `parent_job_id`/`delegate_id`; lease subsetting | Missing | `agent.delegate` and `agent.handoff` payloads exist as v0.2 stubs (`src/messages/execution.rs:164-183`).                             |
| §11 Trace propagation           | W3C 32-hex `trace_id`; runtime mints if absent                                                   | Adjacent    | `TraceId` newtype is free-form (`src/ids.rs`); no 32-hex validation; no auto-mint logic.                                             |
| §12 Errors                      | 12 domain codes (`PERMISSION_DENIED`, `LEASE_SUBSET_VIOLATION`, `JOB_NOT_FOUND`, `DUPLICATE_KEY`, `AGENT_NOT_AVAILABLE`, `CANCELLED`, `TIMEOUT`, `RESUME_WINDOW_EXPIRED`, `HEARTBEAT_LOST`, `INVALID_REQUEST`, `UNAUTHENTICATED`, `INTERNAL_ERROR`) | Wrong | Rust uses gRPC-style canonical codes (`OK`, `INVALID_ARGUMENT`, `RESOURCE_EXHAUSTED`, `DEADLINE_EXCEEDED`, ...) in `src/error.rs:32-96`. Only `PERMISSION_DENIED`, `UNAUTHENTICATED`, `CANCELLED`, `HEARTBEAT_LOST` exist with the same name. |
| §15 Extension namespace         | `x-vendor.*` only                                                                                | Adjacent    | `src/extensions.rs` implements `arcpx.*` namespace per `RFC-0001-v2.md`. Wrong prefix.                                               |

Net: there is no point publishing a v1.0 conformance row-by-row against
the current crate. Phase 4 / Phase 10 take this as the starting point.

## Crate layout

Single crate, no workspace. 32 source files, ~8 273 lines total
(`find src -name '*.rs' | xargs wc -l`).

| Module                       | Purpose (per docstring)                                                                                                 | v1.1 keep / rewrite |
| ---------------------------- | ----------------------------------------------------------------------------------------------------------------------- | ------------------- |
| `src/lib.rs`                 | Crate root; re-exports `ARCPClient`, `ARCPRuntime`, `MessageType`, `Envelope`, `ARCPError`, `ErrorCode`, etc.            | Rewrite re-exports. |
| `src/envelope.rs`            | `Envelope` (typed) + `RawEnvelope` (untyped passthrough). 17 envelope fields incl. `stream_id`, `subscription_id`, `parent_span_id`, `causation_id`, `priority`. | Rewrite — drop fields not in spec §5.1; add `event_seq`; pin `arcp` to `"1"`. The two-layer typed/raw split is worth keeping. |
| `src/error.rs`               | `ErrorCode` (gRPC-style canonical 21 codes) + `ARCPError` (`thiserror` enum, `#[non_exhaustive]`).                       | Rewrite — replace `ErrorCode` with the 15-code v1.1 taxonomy; keep `thiserror` + `#[non_exhaustive]` discipline; keep `From<serde_json::Error>` / `From<rusqlite::Error>` patterns. |
| `src/extensions.rs`          | `ExtensionRegistry` for `arcpx.*` vendor types.                                                                          | Rewrite — switch prefix to `x-vendor.*`; reduce surface (the v1.1 rule is "unknown types ignored", §5.1). |
| `src/ids.rs`                 | `SessionId`, `MessageId`, `JobId`, `LeaseId`, `ArtifactId`, `SubscriptionId`, `StreamId`, `TraceId`, `SpanId`, `IdempotencyKey` newtypes; ULID-backed `::new()`. | Keep. Add `EventSeq(u64)` newtype. Drop `LeaseId`, `ArtifactId`, `SubscriptionId`, `StreamId` (no analogues in v1.1 wire surface). Tighten `TraceId` to W3C 32-hex. |
| `src/messages/mod.rs`        | `MessageType` (58-variant tagged enum) + `Capabilities` (boolean caps).                                                  | Rewrite — replace with the v1.1 message set (~18 envelopes) and the v1.1 feature-flag capability model. Keep the `#[serde(tag = "type", content = "payload")]` + `#[non_exhaustive]` discipline. |
| `src/messages/session.rs`    | `session.open/challenge/authenticate/accepted/unauthenticated/rejected/refresh/evicted/close`.                           | Rewrite — `session.hello/welcome/ping/pong/ack/list_jobs/jobs/bye/error`. |
| `src/messages/control.rs`    | `ping`, `pong`, `ack`, `nack`, `cancel{.accepted,.refused}`, `interrupt`, `resume`, `backpressure`.                      | Rewrite — `session.ping/pong/ack` migrate into `session.rs`; `job.cancel` replaces `cancel`; rest are dropped. |
| `src/messages/execution.rs`  | `tool.{invoke,result,error}`, `job.{accepted,started,progress,heartbeat,completed,failed,cancelled,checkpoint,schedule}`, `agent.{delegate,handoff}`, `workflow.{start,complete}`. | Rewrite — replace with `job.{submit,accepted,cancel,event,result,error,subscribe,subscribed,unsubscribe}` and the `kind` taxonomy. |
| `src/messages/streaming.rs`  | `stream.{open,chunk,close,error}`.                                                                                       | Drop. Streaming becomes `result_chunk` event kind (§8.4). |
| `src/messages/subscriptions.rs` | Multi-axis filtered `subscribe{.accepted,.event,.closed}` / `unsubscribe`.                                            | Drop. Subscriptions become `job.subscribe/subscribed/unsubscribe` (§7.6) attached to a specific `job_id`. |
| `src/messages/permissions.rs` | `permission.{request,grant,deny}` and `lease.{granted,refresh,extended,revoked}`.                                       | Drop. The dynamic lease lifecycle has no v1.1 analogue; leases are immutable at acceptance with optional `lease_constraints.expires_at`. |
| `src/messages/human.rs`      | `human.{input,choice}.{request,response,cancelled}`.                                                                     | Drop. HITL is not in ARCP v1.1 (§1.2 non-goal).                                                                                       |
| `src/messages/artifacts.rs`  | `artifact.{put,fetch,ref,release}`.                                                                                      | Drop the message family. `artifact_ref` becomes a `job.event` body shape. |
| `src/messages/telemetry.rs`  | `trace.span`, `log`, `metric` (top-level).                                                                               | Drop the top-level envelopes. `log`/`metric` become `job.event` kinds; `trace.span` is replaced by W3C trace context propagation in the envelope. |
| `src/auth/mod.rs`            | `Authenticator` trait + `bearer`, `signed_jwt`, `none` impls.                                                            | Slim down — v1.1 §6.1 has only bearer. Keep the trait shape so deployers can ship their own (e.g., JWKS) verifier; drop `signed_jwt` and `none` from the SDK proper (move `none` to the test fixtures). |
| `src/auth/bearer.rs`         | Static bearer-token validation.                                                                                          | Keep, simplified.                                                                                                                     |
| `src/auth/jwt.rs`            | `signed_jwt` verifier using `jsonwebtoken` crate.                                                                        | Drop from the SDK; show as an external integration example.                                                                          |
| `src/auth/none.rs`           | Anonymous auth (gated by `capabilities.anonymous`).                                                                      | Drop — anonymous is not a v1.1 feature.                                                                                              |
| `src/store/eventlog.rs`      | SQLite-backed event log with `INSERT OR IGNORE` idempotency + replay reads.                                              | Keep the SQL + connection-pool scaffolding. Rewrite the schema for v1.1 (`event_seq INTEGER`, session-scoped). Idempotency key store moves to a separate table. |
| `src/store/schema.sql`       | DDL.                                                                                                                     | Rewrite around session-scoped `event_seq`. |
| `src/transport/mod.rs`       | `Transport` trait: `send(Envelope) / recv() / close()`.                                                                 | Keep the trait. Rewrite once `Envelope` is the v1.1 shape.                                                                            |
| `src/transport/memory.rs`    | In-process `MemoryTransport` for tests.                                                                                  | Keep.                                                                                                                                 |
| `src/transport/stdio.rs`     | NDJSON over `AsyncRead`/`AsyncWrite`.                                                                                    | Keep, minor.                                                                                                                          |
| `src/transport/websocket.rs` | WS via `tokio-tungstenite`; text frames only.                                                                            | Keep, minor (DNS-rebind / Host-header protection lives in middleware per Phase 5).                                                    |
| `src/runtime/server.rs`      | `ARCPRuntime` — drives 4-step handshake, dispatches by `MessageType`. 877 lines.                                          | Rewrite — 2-step handshake, dispatch on the v1.1 message set, feature-negotiated handlers (`heartbeat`, `ack`, `list_jobs`, etc.).    |
| `src/runtime/session.rs`     | `SessionState` per session.                                                                                              | Rewrite around session-scoped `event_seq` counter + ack tracking + heartbeat ticker.                                                  |
| `src/runtime/job.rs`         | `Job` state machine.                                                                                                     | Rewrite — states from §7.3 (`pending`/`running`/`success`/`error`/`cancelled`/`timed_out`); add `lease_constraints` watchdog, `cost.budget` counters. |
| `src/runtime/context.rs`     | `ToolContext` with cancel token, human-input round-trips.                                                                | Replace with `JobContext` from §13 examples: `emit`, `progress`, `streamResult`, `delegate`, `lease`, `budget`, `signal` (`CancellationToken`). Drop HITL. |
| `src/runtime/tools.rs`       | `ToolHandler` trait, registry.                                                                                           | Drop. Tool dispatch is out of scope for ARCP (MCP's job). The runtime registers *agents*, not tools.                                  |
| `src/runtime/subscription.rs` | Filter-engine subscription manager.                                                                                     | Rewrite — replace with per-job subscriber registry keyed on `job_id`; reuse the resume-buffer for `history: true` replay.             |
| `src/runtime/artifact.rs`    | Inline base64 artifact store with retention sweep.                                                                       | Drop. Artifacts in v1.1 are external references inside a `job.event` body — the runtime does not store them.                         |
| `src/client/api.rs`          | Type-state `Session<Unauthenticated>` → `Session<Authenticated>`; `JobHandle`, `SubscriptionHandle`. 722 lines.          | Rewrite — keep the `Session<S>` discipline; add `submit`, `cancel`, `subscribe`, `listJobs`, `ack`, `events` stream, `JobHandle::collectChunks`. |
| `src/client/handlers.rs`     | HITL hooks.                                                                                                              | Drop.                                                                                                                                 |
| `src/bin/arcp.rs`            | CLI: `serve`, `version`, etc.                                                                                            | Rewrite for the v1.1 wire surface. Keep `clap`-derive structure.                                                                      |
| `tests/`                     | 14 integration test files; `tests/common/` fixtures; `tests/snapshots/` (insta).                                         | Rewrite as Phase 7 specifies.                                                                                                         |
| `examples/`                  | 14 examples (`cancellation`, `extensions`, `subscriptions/`, `leases/`, `lease_revocation/`, `delegation/`, `handoff/`, `heartbeats/`, `resumability/`, `reasoning_streams/`, `human_input/`, `mcp/`, `permission_challenge/`, `capability_negotiation`). | Rewrite/replace as Phase 6 specifies. The new example set is the TS 18.                                                              |

## Gap matrix — v1.1 features

`state` ∈ `Missing` / `Partial` / `Adjacent` (right shape, wrong wire) /
`Present (wrong protocol)`. `target_module` is the post-Phase-4 home.
`risk` ∈ L/M/H. H-risk gets a Rust-specific friction note.

| v1.1 feature                                  | state                       | target module                         | risk | Rust-specific friction (H-risk only) |
| --------------------------------------------- | --------------------------- | ------------------------------------- | ---- | ------------------------------------ |
| `arcp` field = `"1"` + envelope §5.1 shape    | Present (wrong protocol)    | `arcp-core::envelope`                 | L    |                                      |
| `event_seq` on `job.event` / `job.result` / `job.error` | Missing             | `arcp-core::envelope` + `arcp-runtime::session` | M | |
| 15-code error taxonomy (§12)                  | Wrong                       | `arcp-core::error`                    | L    |                                      |
| `session.hello` / `session.welcome` handshake | Missing                     | `arcp-core::messages::session`        | L    |                                      |
| Bearer auth on `session.hello.payload.auth.token` | Adjacent                | `arcp-core::auth` + `arcp-runtime`    | L    |                                      |
| Capability `features: string[]` + intersection | Missing                    | `arcp-core::version` + `arcp-client` / `arcp-runtime` | L | |
| §6.3 Resume (rotating `resume_token`, replay by `last_event_seq`) | Partial    | `arcp-runtime::session` + `arcp-core::store` | M | Replay must be gap-free across reconnect — the store needs to expose a range read `since: EventSeq` and the in-memory session counter must reset to the highest replayed value. |
| §6.4 Heartbeats (`session.ping`/`pong`, two-interval close) | Missing       | `arcp-runtime::session` + `arcp-client` | M | A `tokio::time::interval` per session is cheap, but the watchdog must observe *any* inbound traffic (not just pongs) — needs an `AtomicI64` `last_seen_at` updated in the recv hot path. |
| §6.5 Ack + back_pressure (`session.ack`)      | Missing                     | `arcp-runtime::session` + `arcp-client::autoAck` | L | |
| §6.6 List jobs (`session.list_jobs` / `session.jobs`) | Missing             | `arcp-runtime::server` + `arcp-client` | L  |                                      |
| §7.1 Job submit (`agent`, `input`, `lease_request`, `lease_constraints`, `idempotency_key`, `max_runtime_sec`) | Missing | `arcp-core::messages::execution` + `arcp-runtime::server` | M | The `JobSubmit` payload is generic over `input: serde_json::Value`; `Agent` trait must accept a `serde_json::Value` and not impose a struct on agent authors. |
| §7.2 Idempotency (logical, by `(principal, idempotency_key)` ≈ 24h) | Adjacent  | `arcp-runtime::server` + `arcp-core::store` | L | |
| §7.3 Lifecycle FSM (6 states; terminal `success`/`error`/`cancelled`/`timed_out`) | Missing | `arcp-runtime::job`                   | L    |                                      |
| §7.4 Cancellation (`job.cancel`, 30s grace, `final_status: "cancelled"`) | Adjacent | `arcp-runtime::server`                | M    | Grace timer + `CancellationToken::cancelled().await` race: needs `tokio::select!` over the agent future and the grace deadline. |
| §7.5 Agent versioning (`name@version`, `AGENT_VERSION_NOT_AVAILABLE`) | Missing  | `arcp-core::messages::execution` + `arcp-runtime::server` | L | |
| §7.6 Subscription (`job.subscribe/subscribed/unsubscribe`, history replay) | Missing | `arcp-runtime::subscription`           | **H** | `Client::subscribe(job_id)` should return a `Stream<Item = Event>` that survives a session hand-off. The natural shape — an `impl Stream` backed by an `mpsc::UnboundedReceiver` held inside a `JobHandle` — pins the receiver across `await` points. Pin/projection through the public API is non-trivial; the pragmatic shape is `Pin<Box<dyn Stream<Item = Event> + Send>>` returned from `subscribe()`. |
| §8.1 Single `job.event` envelope with `kind` + `body` | Missing             | `arcp-core::messages::execution`      | L    |                                      |
| §8.2 Eight v1.0 kinds + 2 new v1.1 kinds      | Missing                     | `arcp-core::messages::execution`      | L    |                                      |
| §8.2.1 `progress` body                        | Missing                     | `arcp-core::messages::execution`      | L    |                                      |
| §8.3 Session-scoped strictly monotonic seq    | Partial                     | `arcp-runtime::session`               | M    |                                      |
| §8.4 `result_chunk` streaming (assemble by `result_id`) | Missing           | `arcp-core::messages::execution` + `arcp-client::JobHandle` | **H** | `JobContext::stream_result()` is the natural writer API but it needs to enforce monotone `chunk_seq` AND coordinate with `Job::emit_result` so they cannot interleave. An `enum ResultMode { None, Inline, Chunked { id, next_seq } }` on the job FSM with `#[must_use]` `ResultWriter` newtypes is the safest shape. The client side needs an async chunk accumulator that decodes both `utf8` and `base64` — a `tokio::sync::mpsc::UnboundedReceiver<ResultChunk>` driven from the event stream, terminated by `more: false`. |
| §9.1–§9.4 Immutable lease + glob enforcement (`fs.read`, `fs.write`, `net.fetch`, `tool.call`, `agent.delegate`, `cost.budget`) | Missing | `arcp-runtime::lease`                  | M    |                                      |
| §9.4 Lease subsetting on delegation           | Missing                     | `arcp-runtime::lease`                 | L    |                                      |
| §9.5 `lease_constraints.expires_at`           | Missing                     | `arcp-runtime::lease` + `arcp-runtime::job` | **H** | Two enforcement seams: (a) `validate_lease_op(lease, capability, target, &LeaseOpContext { now, constraints, budget })` per agent call, and (b) a session-level watchdog timer that fires `LEASE_EXPIRED` when `now ≥ expires_at` and the job is still active. The watchdog and the validator share a clock — `Instant`-based monotonic is wrong because the wire format is `DateTime<Utc>`; we need a `Clock` trait so tests can inject. |
| §9.6 `cost.budget` counters + decrement on `cost.*` metric | Missing        | `arcp-runtime::lease` + `arcp-runtime::job` | M    |                                      |
| §10 Delegation (`delegate` event kind on parent + child `job.accepted` w/ `parent_job_id`/`delegate_id`) | Missing | `arcp-runtime::server`                | M    |                                      |
| §11 OTel attrs `arcp.lease.expires_at`, `arcp.budget.remaining` | Missing       | `arcp-otel` middleware                | L    |                                      |
| 15-error taxonomy incl. 3 v1.1 codes          | Missing                     | `arcp-core::error`                    | L    |                                      |
| `x-vendor.*` extension namespace              | Adjacent (`arcpx.*`)        | `arcp-core::extensions`               | L    |                                      |
| `MemoryTransport`, `WebSocketTransport`, `StdioTransport` | Adjacent           | `arcp-core::transport`                | L    |                                      |

## What carries over

Treat these as the foundation:

- The `ids` module: `MessageId`, `SessionId`, `JobId`, `TraceId`,
  `SpanId`, `IdempotencyKey` newtype machinery and ULID generator.
  Add `EventSeq(u64)`. Drop `LeaseId`, `ArtifactId`, `SubscriptionId`,
  `StreamId`.
- The `Transport` trait (`src/transport/mod.rs`) — three async methods,
  object-safe via `async_trait`. Switch to `impl Trait in trait` once
  MSRV is set.
- The two-layer typed/raw envelope split (`Envelope`, `RawEnvelope`).
  The raw layer is what lets §5.1's "ignore unknown top-level fields"
  pay off without losing forward-compat.
- The lint posture in `Cargo.toml` (lines 124–140): `unsafe_code` deny,
  `missing_docs` deny, clippy pedantic + nursery + `unwrap_used` deny,
  `panic`/`todo`/`unimplemented` deny. Keep verbatim — this is the
  bar for v1.1.
- `rustfmt.toml`, `clippy.toml`, `rust-toolchain.toml` — stable channel,
  `max_width = 100`, threshold knobs.
- The SQLite event log scaffolding (`src/store/eventlog.rs`) — the
  schema is wrong but the connection-pool + `INSERT OR IGNORE`
  patterns transfer.
- The `insta` snapshot-test fixtures under `tests/snapshots/` (the
  patterns, not the recorded envelopes — those are stale).

## What gets dropped

In addition to the wholesale wire rewrite:

- Human-in-the-loop (`src/messages/human.rs`, `src/runtime/context.rs`
  human-input round-trips, `src/client/handlers.rs`, `examples/human_input/`)
  — out of ARCP v1.1 scope per §1.2.
- Built-in tool registry (`src/runtime/tools.rs`) — tool exposure is
  MCP's job. The runtime registers *agents* (per the §6.2 inventory),
  not tools.
- Artifact store (`src/runtime/artifact.rs`) — v1.1 carries
  `artifact_ref` only as a `job.event` body shape; the runtime does
  not store artifacts.
- Top-level `stream.*` envelopes — `result_chunk` covers the only
  bulk-payload need in v1.1.
- Top-level `permission.*` / `lease.*` envelopes — leases are
  immutable at job acceptance.
- The `arcpx.*` extension namespace — switch to `x-vendor.*`.
- `signed_jwt` and `none` auth schemes from the SDK proper — bearer is
  the only v1.1-mandated scheme.
