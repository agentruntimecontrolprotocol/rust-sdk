# Phase 6 — Examples

Source of truth: the 18 examples in
[`typescript-sdk/examples/`](../../../typescript-sdk/examples/) — nine
v1.0 core, nine v1.1 feature. The four host-integration examples
(`tracing/`, `express/`, `fastify/`, `bun/`) live under Phase 5 and are
listed in the "See also" footer for parallels only.

Spec reference: [`spec/docs/draft-arcp-02.1.md`](../../../spec/docs/draft-arcp-02.1.md).
Current-SDK example tree to delete:
[`rust-sdk/examples/`](../../examples/) — 14 directories/files mapped to
the wrong protocol (see Phase 2 audit
[`02-current-audit.md`](./02-current-audit.md) §"Crate layout" /
`examples/`).

## 1. Mapping table — 18 rows

Each TS example is a directory of `server.ts` + `client.ts` + `README.md`.
The Rust mirror is either a single-file `examples/<name>.rs` (when the
TS pair fits cleanly into one process — only `stdio` qualifies) or a
multi-file `examples/<name>/{server.rs,client.rs}` requiring `[[example]]`
entries in `Cargo.toml`. Layout choice is driven by whether the TS demo
runs as two processes (see TS
[`examples/README.md`](../../../typescript-sdk/examples/README.md#running)).

### v1.0 core (9 rows)

| TS name              | Rust example name      | Layout                                                                 | Spec §             | Rust idiom shown off                                                                                                                              |
| -------------------- | ---------------------- | ---------------------------------------------------------------------- | ------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| `submit-and-stream`  | `submit-and-stream`    | Multi-file: `examples/submit-and-stream/{server.rs, client.rs}`        | §13.1, §7.1, §8.2  | `JobContext::emit` takes `JobEventBody` enum; client iterates `handle.events()` as a `Pin<Box<dyn Stream<Item = JobEvent> + Send>>`.              |
| `delegate`           | `delegate`             | Multi-file: `examples/delegate/{server.rs, client.rs}`                 | §13.2, §10         | Child job inherits `trace_id` via `JobContext::delegate(&ChildRequest)`; lease subset checked at compile-time via `Lease::subset_of(&parent)` returning `Result<Lease, LeaseSubsetViolation>`. |
| `resume`             | `resume`               | Multi-file: `examples/resume/{server.rs, client.rs}`                   | §13.3, §6.3        | Drop the `WebSocketTransport` (`transport.close().await`), then reconnect with `Client::resume(ResumeRequest { session_id, resume_token, last_event_seq })`; resume token rotates per Phase 2 §6.3 row. |
| `idempotent-retry`   | `idempotent-retry`     | Multi-file: `examples/idempotent-retry/{server.rs, client.rs}`         | §13.5, §7.2        | `IdempotencyKey` newtype (carried over from `src/ids.rs` per Phase 2 "What carries over"); `DUPLICATE_KEY` matched on a `arcp_core::error::ErrorCode` non-exhaustive enum.                  |
| `lease-violation`    | `lease-violation`      | Multi-file: `examples/lease-violation/{server.rs, client.rs}`          | §13.4, §9.3        | Out-of-lease tool call returns `tool_result { error: ToolError { code: ErrorCode::PermissionDenied, .. } }`; agent observes via `Result<ToolResult, ToolError>` and continues. |
| `cancel`             | `cancel`               | Multi-file: `examples/cancel/{server.rs, client.rs}`                   | §7.4               | Agent races `tokio::select! { _ = ctx.signal.cancelled() => …, _ = work => … }`; runtime grace timer is a separate `tokio::time::sleep_until` per Phase 2 §7.4 friction note.                |
| `stdio`              | `stdio`                | Single-file: `examples/stdio.rs`                                       | §4.2, §22          | Client spawns the runtime via `tokio::process::Command::new(std::env::current_exe()).arg("--child-runtime")` and wires `StdioTransport::from_child(&mut child)` to the child's piped stdin/stdout. |
| `vendor-extensions`  | `vendor-extensions`    | Multi-file: `examples/vendor-extensions/{server.rs, client.rs}`        | §8.2, §9.2, §15    | Unknown `kind` decodes through the raw layer (`RawEnvelope`, Phase 2 "What carries over") into `serde_json::Value`; the vendor-aware handler downcasts via `serde_json::from_value::<AcmeProgress>`. |
| `custom-auth`        | `custom-auth`          | Multi-file: `examples/custom-auth/{server.rs, client.rs}`              | §6.1               | Implement `arcp_core::auth::BearerVerifier` (async trait, object-safe via `async_trait`) for an HMAC-signed token type; bad token → `Err(AuthError::Unauthenticated)` at handshake. |

### v1.1 features (9 rows)

| TS name              | Rust example name      | Layout                                                                 | Spec §        | Rust idiom shown off                                                                                                                                          |
| -------------------- | ---------------------- | ---------------------------------------------------------------------- | ------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `heartbeat`          | `heartbeat`            | Multi-file: `examples/heartbeat/{server.rs, client.rs}`                | §6.4          | `tokio::time::interval(heartbeat_interval)` per session; watchdog reads an `AtomicI64` `last_seen_at` updated in the recv hot path (Phase 2 §6.4 friction note). |
| `ack-backpressure`   | `ack-backpressure`     | Multi-file: `examples/ack-backpressure/{server.rs, client.rs}`         | §6.5, §8.2    | Client auto-ack debouncer via `tokio::sync::Notify`; runtime detects lag by comparing `emitted_seq.load(Relaxed) - last_processed_seq.load(Relaxed) > threshold`. |
| `list-jobs`          | `list-jobs`            | Multi-file: `examples/list-jobs/{server.rs, client.rs}`                | §6.6          | `next_cursor: Option<String>` opaque string; `JobListEntry` is `#[serde(deny_unknown_fields)]` so the runtime contract is enforced by the deserializer.       |
| `subscribe`          | `subscribe`            | Multi-file: `examples/subscribe/{server_rs, clientA.rs, clientB.rs}` (two client binaries) | §7.6, §6.6 | `Client::subscribe(&JobId) -> Pin<Box<dyn Stream<Item = JobEvent> + Send>>` per Phase 2 §7.6 H-risk row; deny-cancel from B surfaces as `ErrorCode::PermissionDenied`. |
| `agent-versions`     | `agent-versions`       | Multi-file: `examples/agent-versions/{server.rs, client.rs}`           | §7.5, §12     | `Agent`-spec parser: custom `impl<'de> Deserialize<'de> for AgentRef` validates `name "@" version` against the §7.5 grammar; bare name resolves via runtime default. |
| `lease-expires-at`   | `lease-expires-at`     | Multi-file: `examples/lease-expires-at/{server.rs, client.rs}`         | §9.5, §12     | `Clock` trait injection (Phase 2 §9.5 H-risk row) — example uses `SystemClock` but the test variant of the same module uses `tokio::time::pause()` + `MockClock`.       |
| `cost-budget`        | `cost-budget`          | Multi-file: `examples/cost-budget/{server.rs, client.rs}`              | §9.6, §12     | Per-currency counters held as `DashMap<CurrencyCode, AtomicI64>`; agent emits `Metric { name: "cost.usd", value: 0.05, unit: "USD" }` and the runtime decrements via `fetch_sub(cents, Relaxed)`. |
| `progress`           | `progress`             | Multi-file: `examples/progress/{server.rs, client.rs}`                 | §8.2.1        | `ctx.progress(Progress { current: i, total: Some(total), units: Some("rows"), message: None })`; client renders a textual bar using `indicatif::ProgressBar` via the binary only (not a dep of the lib). |
| `result-chunk`       | `result-chunk`         | Multi-file: `examples/result-chunk/{server.rs, client.rs}`             | §8.4          | `ctx.stream_result()` returns a `#[must_use] ResultWriter` newtype (Phase 2 §8.4 H-risk row); `JobHandle::collect_chunks() -> Vec<u8>` drains an `mpsc::UnboundedReceiver<ResultChunk>`. |

## 2. Run shape

- Each example runs via `cargo run --example <name>` and exits 0 on
  success. Default profile is `dev`; no extra features required at the
  top level (`transport-ws`, `transport-stdio` are on by default per the
  current `Cargo.toml:39`).
- The 17 multi-file examples need `[[example]] name="…" path="…"`
  entries in `Cargo.toml`. The seventeen names: `submit-and-stream`,
  `delegate`, `resume`, `idempotent-retry`, `lease-violation`, `cancel`,
  `vendor-extensions`, `custom-auth`, `heartbeat`, `ack-backpressure`,
  `list-jobs`, `subscribe` (three entries: `subscribe-server`,
  `subscribe-client-a`, `subscribe-client-b` per the two-observer demo),
  `agent-versions`, `lease-expires-at`, `cost-budget`, `progress`,
  `result-chunk`. Each contributes two `[[example]]` entries
  (`<name>-server` and `<name>-client`); `subscribe` contributes three.
  That is **35 `[[example]]` blocks** for the multi-file set. The one
  single-file example (`stdio`) contributes one block: `name = "stdio"`,
  `path = "examples/stdio.rs"`. Total: **36 `[[example]]` blocks**.
- Env vars match the TS names byte-for-byte so the user-facing surface
  is identical across SDKs (TS
  [`examples/submit-and-stream/README.md`](../../../typescript-sdk/examples/submit-and-stream/README.md#configuration)):
  - `RUST_LOG` — `tracing-subscriber` env filter (Rust-specific, not in TS).
  - `ARCP_DEMO_PORT` — server listen port. Defaults vary per example
    (e.g. `submit-and-stream` 7879, `subscribe` 7888, `result-chunk`
    7893) so multiple examples can run simultaneously, matching TS.
  - `ARCP_DEMO_URL` — `ws://127.0.0.1:<port>/arcp`, used by clients.
  - `ARCP_DEMO_TOKEN` — bearer token (default `demo-token`).
- Two-process examples: two `cargo run --example <name>-server` and
  `cargo run --example <name>-client` invocations in separate terminals
  — option (a), for parity with TS
  [`examples/README.md` §"Running"](../../../typescript-sdk/examples/README.md#running).
  `stdio` is the exception: a single `cargo run --example stdio` that
  spawns its own child via `tokio::process::Command` per Phase 2 row
  for `src/transport/stdio.rs`.

## 3. Common harness

A reader who reads `submit-and-stream` should predict the shape of the
other seventeen. Each example follows the same skeleton.

**Tracing init** — every `main` opens with:

```rust
tracing_subscriber::fmt()
    .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
    .with_target(false)
    .init();
```

Driven by `RUST_LOG` (e.g. `RUST_LOG=arcp=debug,info`). Dependency is
the `tracing` + `tracing-subscriber` pair, already in the current
`Cargo.toml` per the workspace audit.

**Address parsing** — server `main`:

```rust
let port: u16 = std::env::var("ARCP_DEMO_PORT")
    .ok()
    .and_then(|s| s.parse().ok())
    .unwrap_or(7879); // example-specific default
let addr: std::net::SocketAddr = ([127, 0, 0, 1], port).into();
```

Client `main`:

```rust
let url = std::env::var("ARCP_DEMO_URL")
    .unwrap_or_else(|_| format!("ws://127.0.0.1:{port}/arcp"));
```

**Shared config struct, kept local.** Each example directory has a
local `config.rs` module declared `mod config;` from both `server.rs`
and `client.rs` — port default, token default, agent name. **No shared
fixture crate** — examples are self-contained, mirroring TS
[`examples/README.md` §"Conventions"](../../../typescript-sdk/examples/README.md#conventions).
Re-typing five lines of constants beats a fixture crate that obscures
the contract under test.

**Assertion shape.** Every example ends with at least one `assert_eq!`
or an explicit `panic!("unexpected event: {event:?}")` arm in a `match`
on `JobEvent`. The TS examples print and exit 0; Rust examples both
print *and* assert because `cargo run --example` exits non-zero on
panic, which gives CI a real signal. Concrete shapes per example:

- `submit-and-stream`: `assert_eq!(observed_kinds, expected_seven_kinds)`.
- `result-chunk`: `assert_eq!(assembled.len() as u64, result.result_size.unwrap())`.
- `cancel`: `assert_eq!(error.final_status, FinalStatus::Cancelled)`.
- `subscribe`: `assert!(replayed > 0 && live > 0)` plus
  `assert_eq!(cancel_from_b.unwrap_err().code(), ErrorCode::PermissionDenied)`.
- `idempotent-retry`: `assert_eq!(first.job_id, second.job_id)`.

## 4. Per-example notes — the five harder ones

### `result-chunk`

The chunk encoder/decoder split (`encoding ∈ {utf8, base64}`, spec
§8.4) and the client-side accumulator are the demonstration. Server
side: `ctx.stream_result()` returns a `#[must_use] ResultWriter`
(Phase 2 §8.4 H-risk row — `ResultMode { None, Inline, Chunked }` on
the `Job` FSM prevents inline+chunked mixing at compile time).
Writer methods: `write(&mut self, data: impl Into<ChunkData>)` and
`finalize(self, last: impl Into<ChunkData>, ChunkFinalize { summary,
result_size })`. `ChunkData::Text(String)` serialises with
`encoding: "utf8"`; `ChunkData::Binary(Bytes)` with `encoding:
"base64"` using the `base64` crate's `STANDARD` engine. Client side:
`JobHandle::collect_chunks() -> impl Future<Output = Result<Vec<u8>,
ChunkError>>` drains a `tokio::sync::mpsc::UnboundedReceiver<ResultChunk>`
that the `JobHandle` accumulator filled as chunks arrived on the event
stream; back-pressure is preserved because the receiver is bounded
upstream of the WS recv loop (no separate buffer). Each chunk's `data`
is decoded once, appended to a `Vec<u8>`, and the receiver closes when
the loop observes `more: false`.

### `subscribe`

Two clients, same principal, one submits and one observes (TS
[`subscribe/client.ts`](../../../typescript-sdk/examples/subscribe/client.ts)).
In Rust this is three example binaries (`subscribe-server`,
`subscribe-client-a`, `subscribe-client-b`) so each is a normal
`cargo run --example` invocation, and the README sequences them.
Client B discovers the job via `Client::list_jobs(ListJobsRequest {
status: vec![JobStatus::Running], .. })`, then calls
`Client::subscribe(&job_id, SubscribeRequest { history: true,
from_event_seq: None })` which returns a `Pin<Box<dyn Stream<Item =
JobEvent> + Send>>` per Phase 2 §7.6 H-risk row. The deny-cancel case
is explicit: `client_b.cancel_job(&job_id,
CancelRequest::default()).await` returns `Err(ARCPError::Protocol {
code: ErrorCode::PermissionDenied, .. })` and the example asserts on
`code()`. Don't paper over this with a "the runtime silently drops the
message" branch — assert the error.

### `resume`

The example demonstrates the §6.3 reconnect path. The submitter
connects, observes a few events from the `timer`-style agent, then
**drops** the `WebSocketTransport` mid-stream (`drop(transport)` —
this is the closest thing in Rust to TS's `wss.terminate()`). After a
`tokio::time::sleep(Duration::from_millis(200)).await` the client
re-connects on a fresh transport and calls `Client::resume(ResumeRequest
{ session_id, resume_token, last_event_seq })`. The runtime replays
events with `event_seq > last_event_seq`; the example asserts
`replayed_first_seq == last_event_seq + 1` and that
`resume.resume_token != initial.resume_token` (rotation per spec §6.3).
The mid-stream disconnect is the load-bearing bit: it must drop the
WS, not call a graceful `close()` — graceful close commits a `bye` and
the resume window starts ticking; an abrupt drop simulates network
loss, which is what the spec language addresses (Phase 1 §6.3).

### `cost-budget`

Inside the agent: emit `Metric { name: "cost.usd", value: 0.10, unit:
"USD" }` events while doing work. The runtime decrements the
per-currency `AtomicI64` counter held on the `Job` (stored in cents, so
`fetch_sub((value * 100.0) as i64, Relaxed)` — the conversion is part
of the demonstration). When the counter reaches ≤ 0, the next agent
tool call surfaces a `tool_result { error: { code:
ErrorCode::BudgetExhausted, .. } }` inline. The example agent makes
**six** tool calls priced at $0.20 each with a `cost.budget` lease of
`USD:1.00` so the sixth call deterministically trips
`BUDGET_EXHAUSTED`. The runtime emits debounced
`metric { name: "cost.budget.remaining", value: <USD>, unit: "USD" }`
events between calls (spec §9.6) and the client renders them. The
client asserts the final `tool_result` has `code ==
ErrorCode::BudgetExhausted` and that
`metric.cost.budget.remaining` was observed at least once.

### `lease-expires-at`

Set `lease_constraints.expires_at = chrono::Utc::now() +
chrono::Duration::seconds(2)` at submit. The agent's loop calls a
tool every 500 ms; the fifth call lands at `t ≥ 2s` and the runtime's
`validate_lease_op` returns `Err(LeaseError::Expired)`, which surfaces
as a `tool_result { error: { code: ErrorCode::LeaseExpired } }`. The
agent observes via `Result<ToolResult, ToolError>` and returns; the
runtime watchdog (a `tokio::time::sleep_until(expires_at_instant)`
fired alongside the agent future via `tokio::select!`) emits the final
`job.error { final_status: "error", code: "LEASE_EXPIRED" }` per spec
§9.5. The example uses `SystemClock` and a real 2-second wait — no
`tokio::time::pause()` here; that's for tests. Phase 2 §9.5 H-risk row
calls out the `Clock` trait — this example is where authors *see* it
without using its test variant.

## 5. What to drop from the current `examples/` tree

The current `rust-sdk/examples/` (per Phase 2 audit row for `examples/`)
holds **14** entries mapped to the wrong protocol (`RFC-0001-v2.md`,
not the actual spec). Migration deletion list — every one of these is
removed (some have direct v1.1 replacements; most do not):

| Current example                       | Action  | Reason                                                                                                                |
| ------------------------------------- | ------- | --------------------------------------------------------------------------------------------------------------------- |
| `examples/cancellation.rs`            | replace | with `examples/cancel/{server.rs,client.rs}` (TS `cancel/`, §7.4).                                                    |
| `examples/capability_negotiation.rs`  | drop    | v1.1 feature negotiation is implicit in every example's handshake; no standalone demo in the TS 18.                  |
| `examples/extensions.rs`              | replace | with `examples/vendor-extensions/{server.rs,client.rs}` (TS `vendor-extensions/`, §15 — `x-vendor.*` prefix).         |
| `examples/subscriptions/`             | replace | with `examples/subscribe/{server.rs,client-a.rs,client-b.rs}` (TS `subscribe/`, §7.6) — different model, same name minus the 's'. |
| `examples/leases/`                    | replace | by three: `lease-violation/` (§9.3), `lease-expires-at/` (§9.5), `cost-budget/` (§9.6). The old single example collapses three concerns. |
| `examples/lease_revocation/`          | drop    | Dynamic lease revocation has no v1.1 analogue (Phase 2 §9 row — leases immutable at acceptance).                     |
| `examples/delegation/`                | replace | with `examples/delegate/{server.rs,client.rs}` (TS `delegate/`, §10).                                                |
| `examples/handoff/`                   | drop    | `agent.handoff` is v0.2-stub-only, no v1.1 wire surface (Phase 2 `src/messages/execution.rs` row).                   |
| `examples/heartbeats/`                | replace | with `examples/heartbeat/{server.rs,client.rs}` (TS `heartbeat/`, §6.4) — singular per TS.                            |
| `examples/resumability/`              | replace | with `examples/resume/{server.rs,client.rs}` (TS `resume/`, §6.3).                                                    |
| `examples/reasoning_streams/`         | drop    | Top-level `stream.*` envelopes are dropped (Phase 2 `src/messages/streaming.rs` row); the only v1.1 streaming is `result-chunk`. |
| `examples/human_input/`               | drop    | HITL is out of v1.1 scope per spec §1.2.                                                                              |
| `examples/mcp/`                       | drop    | Tool dispatch is MCP's job, not the runtime's (Phase 2 `src/runtime/tools.rs` row).                                  |
| `examples/permission_challenge/`      | drop    | `permission.{request,grant,deny}` family dropped (Phase 2 `src/messages/permissions.rs` row).                        |

After deletion the directory is empty, then repopulated with the 18 v1.1
examples. The eleven `[[example]]` entries in the current
`Cargo.toml:80-122` (per `grep` above) are removed and replaced by the
36 blocks listed in §2.

## See also — host integrations (Phase 5, not Phase 6)

The TS [`tracing/`](../../../typescript-sdk/examples/tracing/),
[`express/`](../../../typescript-sdk/examples/express/),
[`fastify/`](../../../typescript-sdk/examples/fastify/), and
[`bun/`](../../../typescript-sdk/examples/bun/) examples sit at the
host-integration boundary and belong to Phase 5 (middleware crates).
For Rust the parallels are: `arcp-otel` (tracing — W3C trace context
via `tracing-opentelemetry`), an `axum`/`hyper` integration analogous
to Express/Fastify (one HTTP server serving both routes and the ARCP
WS upgrade at `/arcp`, with `allowedHosts` DNS-rebind protection
implemented as a tower layer), and there is no `bun` analogue —
`tokio-tungstenite` covers both `axum` and standalone deployment.
Phase 5 owns those; do not add them to `examples/` here.
