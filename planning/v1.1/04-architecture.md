# Phase 4 — Architecture: Crate Layout, Type System, Concurrency

Inputs: `../spec/docs/draft-arcp-02.1.md`,
`planning/v1.1/01-spec-delta.md`,
`planning/v1.1/02-current-audit.md`,
`../typescript-sdk/packages/{core,client,runtime}/src/`.

## 1. Workspace layout

The TS SDK splits along the public-surface boundaries `@arcp/core`
(wire), `@arcp/client` (consumer), `@arcp/runtime` (producer), and the
umbrella `@arcp/sdk`. The Rust workspace mirrors this 1:1 — splitting on
the same axes is what lets `arcp-client` users avoid pulling in the
runtime's `tokio::time::interval`, `JoinSet`, and lease machinery, and
what lets `arcp-runtime` users avoid the client's reconnect/auto-ack
state. The umbrella crate `arcp` re-exports each according to features
so the common case is `cargo add arcp` and pin the feature.

| Crate          | Mirrors                  | Reason                                                                                                                  |
| -------------- | ------------------------ | ----------------------------------------------------------------------------------------------------------------------- |
| `arcp-core`    | `@arcp/core`             | Envelopes, message enum, error codes, ID newtypes, `Transport` trait, version constants. Zero `tokio`-runtime dependencies beyond `tokio::sync` primitives used by the trait. |
| `arcp-client`  | `@arcp/client`           | `Client`, `JobHandle`, `Subscription`, auto-ack scheduler, reconnect/resume. Depends on `arcp-core` + `tokio`.          |
| `arcp-runtime` | `@arcp/runtime`          | `Server`, `Job`, `Session`, `Lease`, `JobContext`. Depends on `arcp-core` + `tokio` + `tokio-util`.                     |
| `arcp`         | `@arcp/sdk`              | Umbrella, feature-gated re-exports. `default-members` excludes integration test crates.                                  |
| `arcp-otel`    | (new — not in TS)        | OTel span attributes from §11. Separate crate so `tracing-opentelemetry` is not forced on minimum installs. Optional.    |

```toml
# /Cargo.toml (workspace root)
[workspace]
resolver = "2"
members      = ["arcp-core", "arcp-client", "arcp-runtime", "arcp", "arcp-otel", "arcp-cli", "xtask"]
default-members = ["arcp-core", "arcp-client", "arcp-runtime", "arcp"]
```

`arcp-cli` (a thin binary, replaces today's `src/bin/arcp.rs`) and
`xtask` are members but excluded from `default-members` so `cargo build`
at root does the libraries only — same pattern as `tokio` workspace.

## 2. Cargo features

Defined on the umbrella crate `arcp`; feature unification through the
workspace means each transitive feature gates only what it should. The
audit's lint posture (`unsafe_code = "deny"`, `unwrap_used = "deny"`)
applies to every member.

| Feature   | Default | Pulls in                                                                | Gates                                                                              |
| --------- | ------- | ----------------------------------------------------------------------- | ---------------------------------------------------------------------------------- |
| `client`  | on      | `arcp-client`                                                           | `pub use arcp_client::*` in the umbrella.                                          |
| `runtime` | on      | `arcp-runtime`                                                          | `pub use arcp_runtime::*` in the umbrella.                                         |
| `ws`      | on      | `tokio-tungstenite`, `rustls`, `tokio-rustls` in `arcp-core::transport` | `arcp_core::transport::websocket` module — mirrors current `transport-ws` feature in `Cargo.toml`. |
| `stdio`   | off     | nothing (uses `tokio::io::{Stdin, Stdout}`)                             | `arcp_core::transport::stdio` module.                                              |
| `otel`    | off     | `arcp-otel`, `tracing-opentelemetry`, `opentelemetry-sdk`               | §11 span attributes `arcp.lease.expires_at`, `arcp.budget.remaining`.              |

`heartbeat`, `ack`, `subscribe`, `list_jobs`, `cost.budget`,
`lease_expires_at`, `progress`, `result_chunk`, `agent_versions` are
*not* compile features — they are wire features (§6.2). They are
runtime-negotiated via `negotiated_features` and `has_feature`. Making
them compile features would split the type universe.

## 3. Public type model

### 3.1 Envelopes

Per §5.1, the wire shape is `{ arcp, id, type, session_id, trace_id,
job_id, event_seq, payload, extensions }` with unknown fields ignored.
The audit (`02-current-audit.md` §"What carries over") says to keep the
two-layer typed/raw split that today lives in `src/envelope.rs`. The
Rust version is two structs, not one — the raw envelope is what lets a
v1.1 client tolerate a v1.0 runtime that omits `event_seq` and what
lets future v1.2 fields round-trip through a v1.1 runtime per the §5.1
ignore-unknown rule.

```rust
// arcp-core/src/envelope.rs

/// Untyped wire envelope. Always parses; unknown fields land in `extra`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct RawEnvelope {
    pub arcp: ProtocolVersion,                 // serde(rename = "arcp"), const "1"
    pub id: MessageId,
    pub r#type: String,
    pub session_id: Option<SessionId>,
    pub trace_id: Option<TraceId>,
    pub job_id: Option<JobId>,
    pub event_seq: Option<EventSeq>,
    pub extensions: Option<BTreeMap<String, serde_json::Value>>,
    pub payload: serde_json::Value,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// Typed envelope: header fields + a typed message body.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Envelope {
    pub id: MessageId,
    pub session_id: Option<SessionId>,
    pub trace_id: Option<TraceId>,
    pub job_id: Option<JobId>,
    pub event_seq: Option<EventSeq>,
    pub extensions: Option<BTreeMap<String, serde_json::Value>>,
    #[serde(flatten)]
    pub message: Message,        // see §3.2
}
```

The pipeline is `WireBytes → RawEnvelope → Envelope` via
`TryFrom<RawEnvelope> for Envelope`. The typed layer is the
day-to-day surface; the raw layer is what the runtime dispatcher
holds onto for unknown `type` strings so it can re-emit them unchanged
under §5.1 forward-compat.

### 3.2 `Message` enum — the v1.1 taxonomy

One internally-tagged enum (`#[serde(tag = "type", content = "payload")]`)
mirrors `@arcp/core/messages/index.ts`'s `EnvelopeSchema`
discriminated union. The TS list (`SESSION_ENVELOPES` +
`EXECUTION_ENVELOPES`) gives the exact set of 18 v1.1 variants. The
v1.0 `artifact.*` and top-level `telemetry.*` envelopes from the TS
core are *not* in the v1.1 wire surface — the audit drops them — so the
Rust enum is smaller than the TS counterpart by design.

```rust
// arcp-core/src/messages/mod.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
#[non_exhaustive]
pub enum Message {
    // §6 session
    #[serde(rename = "session.hello")]      SessionHello(SessionHelloPayload),
    #[serde(rename = "session.welcome")]    SessionWelcome(SessionWelcomePayload),
    #[serde(rename = "session.error")]      SessionError(ErrorPayload),
    #[serde(rename = "session.bye")]        SessionBye(SessionByePayload),
    #[serde(rename = "session.ping")]       SessionPing(SessionPingPayload),       // §6.4
    #[serde(rename = "session.pong")]       SessionPong(SessionPongPayload),       // §6.4
    #[serde(rename = "session.ack")]        SessionAck(SessionAckPayload),         // §6.5
    #[serde(rename = "session.list_jobs")]  SessionListJobs(SessionListJobsPayload),// §6.6
    #[serde(rename = "session.jobs")]       SessionJobs(SessionJobsPayload),       // §6.6

    // §7 job
    #[serde(rename = "job.submit")]         JobSubmit(JobSubmitPayload),           // §7.1
    #[serde(rename = "job.accepted")]       JobAccepted(JobAcceptedPayload),       // §7.1
    #[serde(rename = "job.cancel")]         JobCancel(JobCancelPayload),           // §7.4
    #[serde(rename = "job.event")]          JobEvent(JobEventPayload),             // §8.1
    #[serde(rename = "job.result")]         JobResult(JobResultPayload),           // §8 terminal
    #[serde(rename = "job.error")]          JobError(JobErrorPayload),             // §8 terminal
    #[serde(rename = "job.subscribe")]      JobSubscribe(JobSubscribePayload),     // §7.6
    #[serde(rename = "job.subscribed")]     JobSubscribed(JobSubscribedPayload),   // §7.6
    #[serde(rename = "job.unsubscribe")]    JobUnsubscribe(JobUnsubscribePayload), // §7.6
}
```

Total: 18 typed variants. `#[non_exhaustive]` is set so adding a v1.2
variant downstream does not break `match` exhaustivity for callers —
the §5.1 "ignore unknown" rule applies here too.

### 3.3 Event kind taxonomy

§8.2 lists 10 reserved kinds plus an open `x-vendor.*` namespace. A
closed Rust enum would force every `x-vendor.*` user to fork the crate.
A `String` would lose the type-safety of the reserved set. The split
that matches the TS `parseJobEventBody` overload pattern is:

```rust
// arcp-core/src/messages/event.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct JobEventPayload {
    pub kind: EventKind,
    pub ts: String,                  // ISO-8601 UTC; not chrono::DateTime — preserves wire byte-equality
    pub body: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EventKind {
    Reserved(ReservedEventKind),     // enum, exhaustive on 10 kinds
    Vendor(String),                  // serde-validated: must match /^x-vendor\.[a-z0-9._-]+$/
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ReservedEventKind {
    Log, Thought, ToolCall, ToolResult, Status, Metric,
    ArtifactRef, Delegate,
    Progress, ResultChunk,           // v1.1 §8.2 additions
}
```

Body parsing is two-step: the typed envelope leaves `body` as
`serde_json::Value`; a helper `parse_body::<T>(&JobEventPayload) -> Result<T>`
narrows on demand. This mirrors the TS `parseJobEventBody` overload
that returns `ReservedEventBodyMap[K]` for reserved kinds and
`unknown` for vendor kinds. Adding a v1.2 kind appends a variant to
`ReservedEventKind` without breaking byte-level deserialization.

### 3.4 ID newtypes

`02-current-audit.md` §"What carries over" pins which IDs survive. The
`ids.rs` macro `prefixed_id!` stays. Net change vs. the current
`src/ids.rs`:

| Newtype           | Action                                                                                |
| ----------------- | ------------------------------------------------------------------------------------- |
| `SessionId`       | Keep. Prefix `sess`.                                                                  |
| `MessageId`       | Keep. Prefix `01J` ULID (no separate prefix; matches TS `MessageId` brand).           |
| `JobId`           | Keep. Prefix `job`.                                                                   |
| `TraceId`         | Keep. Tighten validation: `^[0-9a-f]{32}$` per W3C (§11). Free-form newtype today.     |
| `SpanId`          | Keep. Tighten: 16 hex chars.                                                          |
| `IdempotencyKey`  | Keep. Free-form per §7.2.                                                              |
| `EventSeq(u64)`   | **Add.** Newtype around `u64`, `Copy`. Session-scoped strictly monotonic per §8.3.    |
| `LeaseId`         | Drop. No analogue on the v1.1 wire — leases are immutable values, not entities.       |
| `ArtifactId`      | Drop. `artifact_ref` carries a `uri`; the runtime no longer stores artifacts.         |
| `SubscriptionId`  | Drop. v1.1 §7.6 keys subscriptions by `(SessionId, JobId)`, not a fresh ID.           |
| `StreamId`        | Drop. `result_chunk` is keyed by `result_id` (a plain string per §8.4), not a newtype.|

`ResumeToken(String)` is added as a non-ULID newtype to match the TS
brand and §6.3's "rotates on every successful welcome" requirement —
treating it as a newtype prevents log-leakage via `Debug` (redact in
the impl).

### 3.5 `#[non_exhaustive]` policy

| Type                                                     | Marked? | Why                                                                                              |
| -------------------------------------------------------- | :-----: | ------------------------------------------------------------------------------------------------ |
| `Envelope`, `RawEnvelope`, every payload struct          | Yes     | §5.1 ignores unknown fields and v1.2 will add fields — `#[non_exhaustive]` lets us add without a major bump. |
| `Message`, `ReservedEventKind`                           | Yes     | New variants are additive per §6.2 feature negotiation.                                          |
| `ErrorCode` (wire enum, §3.7)                            | Yes     | §12 grew from 12 → 15 between v1.0 and v1.1; will grow again.                                    |
| ID newtypes (`SessionId` etc.)                           | No      | Tuple-struct newtypes with one field; nothing to "add". `#[non_exhaustive]` is meaningless here. |
| `JobState` enum (six lifecycle states)                   | No      | §7.3 explicitly states the lifecycle has six states and v1.1 adds none. Forcing `_ => ` matches on lifecycle state would hurt the runtime FSM. |
| `Encoding` enum (`utf8` \| `base64`) from §8.4           | No      | Closed by spec.                                                                                  |

## 4. Concurrency model

### 4.1 Task topology

Per `tokio` patterns, one task per connection is too few (one slow
`Job::run` would block the recv loop) and one task per envelope is too
many (lifetime tracking across `event_seq` becomes shared-mutable).
The breakdown that maps cleanly to v1.1's session/job hierarchy:

| Task                          | Spawned by                  | Cancellation token   | Notes                                                                                                 |
| ----------------------------- | --------------------------- | -------------------- | ----------------------------------------------------------------------------------------------------- |
| `session_recv_loop`           | `Server::serve(transport)`  | `session_token`      | Owns the `Transport::recv()` loop and the inbound dispatcher.                                          |
| `session_send_loop`           | `Server::serve(transport)`  | `session_token`      | Drains the per-session `mpsc::Sender<Envelope>` to `Transport::send()`. Serializes outbound order.    |
| `session_heartbeat`           | `session_recv_loop` after `session.welcome` | `session_token` | `tokio::time::interval` with `MissedTickBehavior::Delay`. Reads an `AtomicI64 last_seen_at` updated by the recv loop. |
| `job_runner` (per job)        | `JobManager::start` inside session task | `job_token` (child of `session_token`) | One `JoinSet` per session holds all `job_runner` handles. `JoinSet::join_next()` reaps completion in the session task. |
| `job_lease_watchdog` (optional, §9.5)| `job_runner` when `lease_constraints.expires_at` is set | `job_token` | `tokio::time::sleep_until(expires_at)` then fires `LEASE_EXPIRED` on the job's emit channel. |

`tokio_util::sync::CancellationToken` gives the hierarchy via
`child_token()`. Cancelling the session cancels every job; cancelling
one job leaves the session and siblings alive. This is the exact shape
called out as **H-risk** in `02-current-audit.md` for §7.4 (the
`tokio::select!` race between the agent future and the cancel grace
deadline).

```rust
// arcp-runtime/src/job.rs (sketch only)

let agent_fut = agent.run(input, ctx);
let cancel = job_token.cancelled();
let grace = || async {
    cancel.await;
    tokio::time::sleep(opts.cancel_grace).await
};
let lease_expiry = async {
    match opts.lease_constraints.as_ref().and_then(|c| c.expires_at) {
        Some(at) => tokio::time::sleep_until(at).await,
        None     => std::future::pending().await,
    }
};

let outcome = tokio::select! {
    biased;
    res = agent_fut       => Outcome::Agent(res),
    () = grace()          => Outcome::Cancelled,           // §7.4 30s grace
    () = lease_expiry     => Outcome::LeaseExpired,        // §9.5
};
```

`biased` selects `agent_fut` first when all three are ready so a
naturally-finishing job is not mis-attributed to a near-simultaneous
cancel. This matches the v1.1 §7.4 "cancellation during grace" rule.

### 4.2 Backpressure — bounded channels

Each session owns one outbound `mpsc::channel::<Envelope>(BOUND)`. The
TS runtime's `DEFAULT_MAX_BUFFERED_EVENTS = 10_000` is the cap
(`packages/runtime/src/server.ts:91`); Rust uses **`BOUND = 1024`** —
two orders of magnitude smaller, defended as:

- 1024 covers a burst from a chatty `progress`/`result_chunk` stream
  (§8.2.1 / §8.4) at typical 1ms cadence for a full second before the
  agent feels backpressure via `mpsc::Sender::send().await`.
- `Envelope` is on the order of 1KB → ~1MB high-water per session; an
  order of magnitude below the TS 16 MiB cap.
- When the bound is hit, `mpsc::send().await` parks the *agent task*,
  not the recv loop — this is the §6.5 "slow consumer" condition the
  spec wants the runtime to detect. The runtime then emits a `status`
  event with `phase: "back_pressure"` per §6.5/§13.2 from the recv
  side, observable because the watermark `emitted_seq -
  last_processed_seq` is read on every inbound `session.ack`.

The TS runtime's resume buffer (`EventLog`) is separate from the
outbound channel — same in Rust: the channel handles in-flight
backpressure, the SQLite-backed event log handles replay-window storage
(carried over from `src/store/eventlog.rs` per the audit).

### 4.3 Subscription streams

`Client::subscribe(job_id)` returns a stream that survives the
subscribe-response round-trip and continues delivering `JobEvent`
envelopes for that `job_id`. The audit calls this **H-risk** because
the natural backing is an `mpsc::UnboundedReceiver` held inside the
`Subscription` and `Receiver: !Unpin`. Two options:

1. `Pin<Box<dyn Stream<Item = JobEventPayload> + Send + 'static>>` —
   one heap allocation per subscription, dyn dispatch on `poll_next`.
   Simple, opaque to the caller.
2. `tokio_stream::wrappers::UnboundedReceiverStream<JobEventPayload>` —
   no boxing, no dyn, `Send`. Already implements `Stream` and is
   `Unpin`. The wrapper owns the receiver, so the caller never sees
   `!Unpin`.

**Recommendation: `UnboundedReceiverStream`.** No allocation, no
indirection, and the return type is concrete (`impl Stream + Send +
Unpin`) — friendlier to downstream `.filter_map`/`.then` adapters than
a `Pin<Box<dyn Stream>>`. The only thing it costs versus option 1 is
that we expose a `tokio_stream` re-export in the public signature,
which is the deliberate trade-off (`tokio_stream` is already in the
dep tree because `Server::serve` uses `tokio_stream::StreamExt` on the
session task `JoinSet`).

```rust
// arcp-client/src/client.rs (signatures only)

pub type EventStream = UnboundedReceiverStream<JobEventPayload>;

impl Client {
    pub async fn subscribe(&self, job_id: JobId, opts: SubscribeOpts)
        -> Result<Subscription, Error>;
}

pub struct Subscription {
    pub job_id: JobId,
    pub subscribed_from: EventSeq,
    pub replayed: bool,
    events: EventStream,
}

impl Stream for Subscription {
    type Item = JobEventPayload;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // delegates to self.events
    }
}
```

`Subscription` itself is `Unpin` (all fields `Unpin`), so the public
method `Subscription::poll_next` is callable without `Box::pin`.

### 4.4 Heartbeat watchdog

§6.4 "two consecutive silent intervals MAY close the transport with
`HEARTBEAT_LOST`". The Rust shape:

- `AtomicI64 last_seen_at_millis` on `SessionState`, updated by
  `session_recv_loop` on every successful `Transport::recv()` — *any*
  inbound message, not just `session.pong`, per §6.4 ("no messages of
  any kind").
- `session_heartbeat` task ticks at `heartbeat_interval_sec` via
  `tokio::time::interval`. `MissedTickBehavior::Delay` so a paused
  runtime (e.g. test using `tokio::time::pause`) does not fire a burst.
- On each tick, compute `now - last_seen_at`. If `> 2 *
  heartbeat_interval`, cancel `session_token` and emit `session.error
  { code: HEARTBEAT_LOST }` via the outbound send loop. Jobs continue
  per §6.4 — they survive on `JobManager` and are reachable via
  `job.subscribe` from a new session.
- If `> 1 * heartbeat_interval` and we're the side that's idle, emit
  `session.ping`.

## 5. Errors

### 5.1 Two-enum split (wire vs. in-process)

The TS SDK keeps these merged because TS exceptions carry an arbitrary
`code` string. Rust must split them — the wire-level set is closed
(§12, 15 codes, will grow) and the in-process surface needs `#[from]`
variants for `serde_json::Error`, `std::io::Error`,
`tokio_tungstenite::Error`, and `rusqlite::Error` that are not wire
concepts.

Per `02-current-audit.md` §"What carries over", the current SDK
already has this exact split (`ErrorCode` + `ARCPError`). The split
survives the rewrite — only the contents change.

```rust
// arcp-core/src/error.rs

/// §12 wire codes. 15 variants; `#[non_exhaustive]` for v1.2+ growth.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[non_exhaustive]
pub enum ErrorCode {
    PermissionDenied, LeaseSubsetViolation, JobNotFound,
    DuplicateKey, AgentNotAvailable, AgentVersionNotAvailable,  // v1.1
    Cancelled, Timeout, ResumeWindowExpired, HeartbeatLost,
    LeaseExpired,                                                // v1.1
    BudgetExhausted,                                             // v1.1
    InvalidRequest, Unauthenticated, InternalError,
}

/// §12 wire payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ErrorPayload {
    pub code: ErrorCode,
    pub message: String,
    pub retryable: Option<bool>,
    pub details: Option<BTreeMap<String, serde_json::Value>>,
}
```

Each crate then defines its own `Error` enum:

```rust
// arcp-core/src/error.rs

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("ARCP error: {0:?}")]
    Wire(ErrorPayload),

    #[error("serialization: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("invalid envelope: {0}")]
    InvalidEnvelope(String),

    #[error("transport: {0}")]
    Transport(String),  // boxed underneath; concrete types live in arcp-client/runtime
}
```

```rust
// arcp-client/src/error.rs
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error(transparent)] Core(#[from] arcp_core::Error),
    #[error("handshake: {0}")] Handshake(String),
    #[error("transport closed")] TransportClosed,
    #[cfg(feature = "ws")]
    #[error("websocket: {0}")] WebSocket(#[from] tokio_tungstenite::tungstenite::Error),
    #[error("io: {0}")] Io(#[from] std::io::Error),
}
```

Same shape for `arcp-runtime::Error` plus `JobNotRegistered`,
`LeaseViolation`, `BudgetExhausted` in-process variants that map back
to wire codes via `impl From<runtime::Error> for ErrorPayload`. The
runtime's lease enforcer returns `runtime::Error::LeaseViolation`
which serializes to `ErrorCode::PermissionDenied` on the wire — the
runtime error names track Rust-side semantics, the wire codes track
§12 verbatim.

### 5.2 No `anyhow` in libraries

The lint posture from `Cargo.toml` already deny-lists `panic`,
`todo`, `unimplemented`. **Rule:** `anyhow` is also forbidden in every
library member of the workspace. The umbrella `arcp` crate is a
library and obeys this. Callers (binary crates including `arcp-cli`)
may use `anyhow` past their `main` seam — they convert via `Error -> anyhow::Error`
through `thiserror`'s `Error` impl. This rule belongs in
`/rust-sdk/CODE_GUIDELINES.md` once Phase 6 ships.

## 6. Public API sketch — types only

### 6.1 `Client`

Shape oracle: `@arcp/client/src/client.ts` (`ARCPClient` class). Rust
renames to `Client` — the `ARCP` prefix is namespacing TS lacks; Rust
has the crate name for that.

```rust
// arcp-client/src/lib.rs

pub struct Client { /* fields private */ }

impl Client {
    pub fn builder() -> ClientBuilder;

    pub async fn connect(self, transport: Box<dyn Transport>)
        -> Result<ConnectedClient, Error>;
}

pub struct ConnectedClient { /* fields private */ }

impl ConnectedClient {
    pub fn welcome(&self) -> &SessionWelcomePayload;
    pub fn negotiated_features(&self) -> &[String];
    pub fn has_feature(&self, name: &str) -> bool;

    pub async fn submit(&self, opts: SubmitOptions) -> Result<JobHandle, Error>;
    pub async fn cancel(&self, job_id: &JobId, reason: Option<&str>) -> Result<(), Error>;

    // §6.5; errors if `ack` feature not negotiated
    pub async fn ack(&self, seq: EventSeq) -> Result<(), Error>;

    // §6.6
    pub async fn list_jobs(&self, filter: Option<ListJobsFilter>, page: PageOpts)
        -> Result<JobsPage, Error>;

    // §7.6
    pub async fn subscribe(&self, job_id: JobId, opts: SubscribeOpts)
        -> Result<Subscription, Error>;

    /// All inbound envelopes for this session, post-welcome.
    pub fn events(&self) -> impl Stream<Item = Envelope> + Send + Unpin;

    pub async fn close(self, reason: Option<&str>) -> Result<(), Error>;
}
```

### 6.2 `Session<S>` type-state — decision

The current SDK has `Session<Unauthenticated>` → `Session<Authenticated>`.
v1.1 collapses the 4-step handshake to 2 steps (§6.2 `session.hello` →
`session.welcome`), so the "before bearer is presented" state is
unreachable from outside the crate — the `Client::connect` future
either resolves with a `ConnectedClient` or returns
`UnauthenticatedError`.

**Recommendation: drop the type-state on the public surface.** The
two public types are `Client` (unconnected) and `ConnectedClient`
(post-welcome). The type-state pattern continues internally on the
runtime side (`SessionState::Pending` → `::Accepted` inside
`arcp-runtime`) where it gates dispatch but is not exposed.

### 6.3 `JobHandle`

Shape oracle: `@arcp/client/src/types.ts::JobHandle`. The TS handle
exposes `id`, `lease`, `done`, `events`, `cancel()`. Rust:

```rust
pub struct JobHandle { /* fields private */ }

impl JobHandle {
    pub fn job_id(&self) -> &JobId;
    pub fn lease(&self) -> &Lease;
    pub fn lease_constraints(&self) -> Option<&LeaseConstraints>;
    pub fn budget(&self) -> &BTreeMap<String, f64>;
    pub fn trace_id(&self) -> Option<&TraceId>;

    /// Stream of events for this job (excluding terminal job.result/job.error).
    pub fn events(&self) -> impl Stream<Item = JobEventPayload> + Send + Unpin;

    /// Stream of `result_chunk` bodies grouped by `result_id` (§8.4).
    pub fn collect_chunks(&self) -> impl Stream<Item = ResultChunkBody> + Send + Unpin;

    /// Resolves with the terminal `job.result`. After this, `events()` is closed.
    pub async fn result(self) -> Result<JobResultPayload, Error>;

    pub async fn cancel(&self, reason: Option<&str>) -> Result<(), Error>;
}
```

`result(self)` consumes the handle — terminal events are exactly-once
per §7.3. `events()` and `collect_chunks()` return `&self`-borrowed
streams driven by the same underlying `mpsc::Receiver` that
`Subscription` uses (§4.3). Same `UnboundedReceiverStream` rationale.

### 6.4 `Server` / `Runtime`

Shape oracle: `@arcp/runtime/src/server.ts::ARCPServer`. Rust naming:
`Server` is the public type; the module is `arcp_runtime`. "Runtime"
is overloaded in Rust (tokio::runtime). The current SDK uses
`ARCPRuntime` — rename for consistency with Server-side terminology.

```rust
// arcp-runtime/src/lib.rs

pub struct Server { /* fields private */ }

impl Server {
    pub fn builder() -> ServerBuilder;
}

pub struct ServerBuilder { /* fields private */ }

impl ServerBuilder {
    pub fn runtime_identity(self, ident: RuntimeIdentity) -> Self;
    pub fn bearer<V: BearerVerifier + 'static>(self, v: V) -> Self;
    pub fn heartbeat_interval(self, dur: Duration) -> Self;
    pub fn resume_window(self, dur: Duration) -> Self;
    pub fn cancel_grace(self, dur: Duration) -> Self;
    pub fn features<I: IntoIterator<Item = String>>(self, features: I) -> Self;
    pub fn back_pressure_threshold(self, n: usize) -> Self;
    pub fn event_log<L: EventLog + 'static>(self, log: L) -> Self;

    // Agent registration. §7.5 versioning is opt-in via register_agent_version.
    pub fn register_agent<A: Agent + 'static>(self, name: &str, agent: A) -> Self;
    pub fn register_agent_version<A: Agent + 'static>(
        self, name: &str, version: &str, agent: A) -> Self;
    pub fn set_default_agent_version(self, name: &str, version: &str) -> Self;

    pub fn build(self) -> Result<Server, Error>;
}

impl Server {
    /// Drive one transport for the lifetime of one session.
    /// Spawns the recv/send/heartbeat tasks and returns when the session ends.
    pub async fn serve<T: Transport + 'static>(&self, transport: T) -> Result<(), Error>;
}
```

### 6.5 `Transport` trait

Audit says keep the existing trait shape (`src/transport/mod.rs`).
Method signatures are unchanged in spirit; one revision is owner-vs-shared
state. Today's trait uses `&self` recv; that requires interior
mutability in implementors. The MSRV pin (§7) is `1.88` per
`Cargo.toml:5`, so `impl Trait in trait` (stabilized in 1.75) is
available. **But:** `Transport` must stay object-safe — `Server::serve`
takes `Box<dyn Transport>` to support hetero transport selection per
deployment. Object-safety + `async fn in trait` is fine on 1.75+ as
long as the trait does not return `impl Trait`. We keep `async_trait`
**only** if the dyn-call overhead on the recv hot path matters; the
modern path is:

```rust
// arcp-core/src/transport/mod.rs

pub trait Transport: Send + Sync + 'static {
    fn send(&self, env: Envelope) -> impl Future<Output = Result<(), Error>> + Send;
    fn recv(&mut self) -> impl Future<Output = Result<Option<Envelope>, Error>> + Send;
    fn close(self: Box<Self>) -> impl Future<Output = Result<(), Error>> + Send;
}
```

This is **not object-safe** because of `impl Future`. Resolution:
expose a parallel `DynTransport` trait that uses `BoxFuture` for dyn
dispatch sites. Inside `Server`, generic over `T: Transport` keeps the
hot path mono; the public `serve<T>` signature already takes the
generic. Only `Server::serve<T>` and `Client::connect<T>` need
generics; nothing else.

**Recommendation:** native `impl Trait in trait` (no `async_trait`) on
`Transport`, generic in `Server::serve` / `Client::connect`. Drop dyn
dispatch entirely. This is one place where the 1.88 MSRV pays off.

### 6.6 `Agent` trait

Shape oracle: `@arcp/runtime/src/types.ts`'s
`AgentHandler<Input, Result> = (input, ctx) => Promise<Result>`.

Two Rust shapes were considered:

1. `Agent { async fn run(&self, input: serde_json::Value, ctx: JobContext) -> Result<serde_json::Value, Error> }`
   — single type, no generics, easy to register in a heterogenous map.
2. `Agent { type Input: DeserializeOwned; type Output: Serialize; async fn run(&self, input: Self::Input, ctx: JobContext) -> Result<Self::Output, Error> }`
   — typed inputs/outputs; needs a `BoxedAgent` adapter to erase the
   associated types for storage in `HashMap<String, Box<dyn ErasedAgent>>`.

**Recommendation: option 2 with a private `ErasedAgent` wrapper.** This
is the same trade-off the audit's H-risk note for §7.1 calls out
(`The JobSubmit payload is generic over input: serde_json::Value;
Agent trait must accept a serde_json::Value and not impose a struct on
agent authors`). The reconciliation is to keep the wire generic
(`serde_json::Value`) and the *user-facing* `Agent` trait typed,
mediated by an `ErasedAgent` that handles the `Value → T` deserialize
at dispatch time. TS does this naturally with type erasure on
`AgentHandler<unknown, unknown>`; Rust does it explicitly.

```rust
// arcp-runtime/src/agent.rs

pub trait Agent: Send + Sync + 'static {
    type Input: DeserializeOwned + Send + 'static;
    type Output: Serialize + Send + 'static;

    fn run(&self, input: Self::Input, ctx: JobContext)
        -> impl Future<Output = Result<Self::Output, Error>> + Send;
}

// Private to arcp-runtime; users only see `register_agent<A: Agent>`.
trait ErasedAgent: Send + Sync {
    fn run_erased<'a>(&'a self, input: serde_json::Value, ctx: JobContext)
        -> BoxFuture<'a, Result<serde_json::Value, Error>>;
}
```

`Agent::Output = ()` for void-returning agents. The result-streaming
case (§8.4) is handled by emitting `result_chunk` events through
`JobContext::stream_result()` and returning `()` from `run`.

### 6.7 `JobContext`

Shape oracle: `@arcp/runtime/src/types.ts::JobContext`. Direct port,
adjusted for Rust idioms (returning `&CancellationToken` rather than
the JS `AbortSignal`):

```rust
// arcp-runtime/src/context.rs

pub struct JobContext { /* fields private */ }

impl JobContext {
    pub fn job_id(&self) -> &JobId;
    pub fn session_id(&self) -> &SessionId;
    pub fn agent(&self) -> &str;
    pub fn agent_version(&self) -> Option<&str>;
    pub fn lease(&self) -> &Lease;
    pub fn lease_constraints(&self) -> Option<&LeaseConstraints>;
    pub fn budget(&self) -> &BTreeMap<String, f64>;
    pub fn trace_id(&self) -> Option<&TraceId>;
    pub fn signal(&self) -> &CancellationToken;

    pub async fn emit(&self, kind: EventKind, body: serde_json::Value) -> Result<(), Error>;
    pub async fn log(&self, level: LogLevel, message: &str) -> Result<(), Error>;
    pub async fn thought(&self, text: &str) -> Result<(), Error>;
    pub async fn status(&self, phase: &str, message: Option<&str>) -> Result<(), Error>;
    pub async fn metric(&self, m: MetricBody) -> Result<(), Error>;
    pub async fn tool_call(&self, body: ToolCallBody) -> Result<(), Error>;
    pub async fn tool_result(&self, body: ToolResultBody) -> Result<(), Error>;
    pub async fn artifact_ref(&self, body: ArtifactRefBody) -> Result<(), Error>;
    pub async fn delegate(&self, body: DelegateBody) -> Result<JobHandle, Error>;

    // §8.2.1
    pub async fn progress(&self, current: f64,
        total: Option<f64>, units: Option<&str>, message: Option<&str>) -> Result<(), Error>;

    // §8.4
    pub fn stream_result(&self) -> ResultWriter;
}

/// `#[must_use]` because `finalize` is the only way to emit the terminating
/// `job.result` referencing the chunks. Dropping without finalize is a runtime
/// error.
#[must_use = "ResultWriter must be finalized to emit job.result"]
pub struct ResultWriter { /* fields private; enforces monotone chunk_seq */ }

impl ResultWriter {
    pub fn result_id(&self) -> &str;
    pub async fn write_utf8(&mut self, chunk: &str) -> Result<(), Error>;
    pub async fn write_base64(&mut self, bytes: &[u8]) -> Result<(), Error>;
    pub async fn finalize(self, summary: Option<&str>) -> Result<(), Error>;
}
```

`ResultWriter` is the §8.4 H-risk safeguard the audit calls out: it
holds the per-job `result_id` and a `chunk_seq` counter, so each
`write_*` increments and stamps in one place. The enum-state-machine
`ResultMode { None, Inline, Chunked { id, next_seq } }` lives inside
`Job` and is consulted on every emit; attempting `Job::emit_result`
while `Chunked` is active returns `Error::InternalError("cannot mix
inline result with result_chunk")`.

### 6.8 Bounds

Public surface commitments:

- Every future returned from the public API: `+ Send + 'static`.
  Required because `tokio::spawn` is the dominant idiom and forcing
  `?Send` would block `tokio::task::spawn` at every call site.
- Agent state (`A: Agent`): `Send + Sync + 'static`. `Sync` because
  `Server::serve` may dispatch multiple jobs to the same `Agent`
  concurrently from the per-session `JoinSet`. Trade-off: `&self`
  agents must use interior mutability for any mutable state; this
  matches the TS `AgentHandler` shape where the function is implicitly
  re-entrant.
- `Transport: Send + Sync + 'static`.

## 7. Hard rules

| Rule                                                       | Source / enforcement                                                                              |
| ---------------------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| `#![deny(unsafe_code)]`                                    | Current `Cargo.toml:125`. Apply to every workspace member.                                        |
| `#![deny(missing_docs)]` on every published lib            | Current `Cargo.toml:126`. Apply to `arcp-core`, `arcp-client`, `arcp-runtime`, `arcp`, `arcp-otel`. |
| `clippy::unwrap_used = "deny"`, `panic = "deny"`           | Current `Cargo.toml:132-136`. No panics, `unwrap()`, `todo!()`, or `unimplemented!()` in library code. |
| `anyhow` is forbidden in every workspace library member    | Phase 4 decision. Callers may use `anyhow` past `main`. Enforced via `cargo deny` in CI.          |
| MSRV pinned to **`1.88`**                                  | Current `Cargo.toml:5` (`rust-version = "1.88"`) and `rust-toolchain.toml`. Quoted forward in each crate's `Cargo.toml`. |
| `Transport`, `Agent`, `BearerVerifier`, `EventLog` are *sealed* | Private supertrait technique: `pub trait Transport: Send + Sync + 'static + private::Sealed { ... }` with `mod private { pub trait Sealed {} }`. Downstream cannot add impls; we add new methods without a major bump. The exception is `Agent`, which **must** be open (users implement it) — that one stays unsealed. |
| Every public struct gets `#[non_exhaustive]` unless explicitly closed by spec | §3.5 table. The closed cases are `JobState`, `Encoding`, ID newtypes. |

Sealing technique applies to `Transport`, `BearerVerifier`, `EventLog`,
`Clock` (the test-injection seam called out as H-risk for §9.5
enforcement). It does *not* apply to `Agent`. Documented at the trait
definition: `// Sealed: users do not implement this trait.`

---

End of architecture. Phase 5 picks up the SQLite schema for the
session-scoped event log; Phase 6 wires the v1.1 example set.
