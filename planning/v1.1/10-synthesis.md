# Phase 10 — Synthesis

## Executive summary

This is **not** a v1.0 → v1.1 additive migration. The Rust SDK was built
against `RFC-0001-v2.md`, a parallel ARCP draft whose wire surface
disagrees with `spec/docs/draft-arcp-02.md` (v1.0) and
`draft-arcp-02.1.md` (v1.1) at every load-bearing axis: envelope shape
(`arcp: "1.0"` vs `"1"`, no `event_seq`), handshake (4-step
`session.open` chain vs 2-step `session.hello`/`session.welcome`),
error taxonomy (gRPC-style 21-code canonical vs spec's 15 domain
codes), job lifecycle (per-phase envelopes vs unified `job.event` +
`kind` taxonomy), authority model (dynamic
`permission.*`/`lease.*` flow vs immutable lease + delegated subset),
and several whole envelope families that the spec does not carry
(HITL, top-level `stream.*`, top-level `artifact.*`). The current
`CONFORMANCE.md` and `PLAN.md` are 5-line stubs;
`rust-sdk/README.md:36` reports 134 tests + 85% line coverage — all
against the wrong wire.

The work is a **rewrite of the wire layer and the runtime** on top of
a thin keeper of infrastructure: the ULID-backed `ids` newtype
machinery, the two-layer typed/raw envelope split, the `Transport`
trait shape, the SQLite event-log scaffolding, and the lint posture
in `Cargo.toml:124-140`. The TypeScript SDK at `typescript-sdk/` is
already on v1.1 and is the reference for both wire-byte parity
(`CONFORMANCE.md`, 18 examples) and public-API shape oracle.

Phase 3 sizes the dep tree (`tokio` + `serde` + `thiserror` + `ulid`,
`tokio-tungstenite` 0.24, MSRV 1.82); Phase 4 lays out the workspace
(`arcp-core`, `arcp-client`, `arcp-runtime`, `arcp`, `arcp-otel`,
`arcp-cli`) and the type system (18-variant `Message` enum,
`UnboundedReceiverStream` for `subscribe`, `ResultWriter` FSM for
`result_chunk`, dropped `Session<S>` type-state on the public
surface); Phase 5 adds host adapters (`arcp-axum`, `arcp-hyper`,
`arcp-tokio-tungstenite`) with deny-all `HostAllowlist`; Phase 6 maps
the 18 TS examples 1-for-1; Phase 7 raises the coverage floor to 87%
lines AND regions and adds a scoped `cargo-mutants` nightly; Phase 8
plans the `docs/` tree (33 pages + migration page) with a shared
frontmatter schema; Phase 9 commits six diagrams under
`rust-sdk/diagrams/` matching TS's light/dark `<picture>` pattern.

The rewrite is roughly 18 PR-sized milestones (§4 below). M0–M3 land
the wire surface and transports; M4–M10 are the runtime; M11 is the
client; M12 is the example set; M13–M14 are host adapters and OTel;
M15–M18 are docs, diagrams, conformance, and the README.

## Cross-phase seams resolved

| #  | Seam                                            | Phase A position                                                                | Phase B position                                                          | Resolution                                                                                                          |
| -- | ----------------------------------------------- | ------------------------------------------------------------------------------- | ------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------- |
| 1  | **MSRV**                                        | Phase 3: `1.82` (covers `&raw const`, precise-capturing for `subscribe()`)      | Phase 4: `1.88` to drop `async_trait` cleanly                              | **Take 1.82.** AFIT (`async fn in trait`) has been stable since 1.75; `impl Trait in trait` since 1.75; precise-capturing since 1.82. Nothing Phase 4 actually needs requires 1.88. The cost of forcing every downstream consumer onto a toolchain younger than the OTel 0.27 release window wins this. |
| 2  | **`cargo-mutants`**                             | Phase 3: reject — 20–60 min wall time on GH free-tier                           | Phase 7: accept SCOPED to `arcp-core/src/messages/` + `arcp-runtime/src/lease.rs`, nightly, non-blocking | **Take Phase 7's scoped form.** Phase 3's CI-cost blocker is dissolved by Phase 7's two-file allowlist (~6 min) plus the nightly-cron non-blocking job. The two modules are the security-critical paths; surviving mutants there are real bugs. |
| 3  | **Diagrams path**                               | Phase 8 / BOOTSTRAP wording: `docs/diagrams/*.dot`                              | Phase 9: `rust-sdk/diagrams/` to match `typescript-sdk/diagrams/`         | **Take Phase 9's path.** Cross-SDK alignment + GitHub `<picture>` pattern require the top-level `diagrams/` directory. Phase 8's `docs/` pages reference them as `../diagrams/<name>-{light,dark}.svg`. Phase 8's plan needs a 1-line correction when implemented; nothing semantic changes. |
| 4  | **Workspace member count**                      | Phase 3: 5 crates (CLI bundled into `arcp` facade)                              | Phase 4: 6 crates (`arcp-cli` separate from `arcp` facade) + `xtask`     | **Take Phase 4's split.** Library facade and CLI binary should not share a Cargo unit — bundling forces CLI deps (`clap`) onto every library consumer. `xtask` adopts the `tokio` pattern. |
| 5  | **Middleware crate count**                      | Phase 3: names only `arcp-otel`                                                 | Phase 5: `arcp-axum`, `arcp-hyper`, `arcp-tokio-tungstenite`, `arcp-otel` (4) | **Take Phase 5's full set.** Phase 3's Cargo.toml fragment must be extended to list all four middleware crates as members. None ship in M0 — they land in M13/M14. |
| 6  | **`subscribe()` return type**                   | Phase 2 H-risk note / Phase 6 / Phase 8: `Pin<Box<dyn Stream<Item = Event> + Send>>` | Phase 4: `tokio_stream::wrappers::UnboundedReceiverStream`                | **Take Phase 4's recommendation.** Concrete return type, no allocation, `Send + Unpin`, friendlier to downstream `.filter_map`/`.then`. Cost: a public re-export of `tokio_stream` in the signature, which we already have transitively. Phase 6 and Phase 8 wording is implementer-corrected during M11/M15. |
| 7  | **`clippy::expect_used` warn vs deny**          | Phase 3: tighten warn → deny once rewrite lands                                 | Current `Cargo.toml:133`: warn                                            | **Stay at warn through M0–M10; flip to deny in M11.** Tightening before the rewrite has rebuilt every module would flood every WIP commit; flipping at M11 (when the public client surface is green) is the right gate. |
| 8  | **`async_trait` survival**                      | Phase 4: drop entirely; generic `Server::serve<T: Transport>`                    | Phase 6 wording for `BearerVerifier`: "async trait, object-safe via `async_trait`" | **Take Phase 4.** `BearerVerifier` is registered generically in `ServerBuilder::bearer<V: BearerVerifier + 'static>` — no dyn site needs `async_trait`. Phase 6's `custom-auth` example wording is corrected during M11. |
| 9  | **Doctest distribution**                        | Phase 8: only `arcp` umbrella ships doctests                                    | (no contradiction)                                                        | Confirmed. Inner crates carry `#![deny(missing_docs)]` rustdoc but no `cargo test --doc` cases. The README quickstart + `docs/01-quickstart.md` are the only end-to-end excerpts and they live in the umbrella. |
| 10 | **What `examples/` directory holds today**      | Phase 6 lists 14 wrong-protocol examples for deletion                            | (no contradiction; Phase 2 audit names them too)                          | Confirmed delete list. Replaced by 18 new examples in M12 (`[[example]]` count grows from 11 to 36 in `Cargo.toml`). |

## Milestones — ordered, PR-sized

Each row is one mergeable PR. "Files" lists the directories/files
created or substantively modified. "Spec §" lists what the PR
makes conformant. Tests must be green before the next PR starts.

| #   | Milestone                                            | Files added / modified                                                                                                                  | Spec § landed                       | Depends on |
| --- | ---------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------- | ---------- |
| M0  | Workspace skeleton                                   | `Cargo.toml` (root → `[workspace]`); `rust-toolchain.toml` (channel `1.82`); `clippy.toml`, `rustfmt.toml`; empty `crates/{arcp-core,arcp-client,arcp-runtime,arcp,arcp-otel,arcp-cli,xtask}/Cargo.toml`; archive current `src/` under git tag `pre-v1.1`; `.github/workflows/ci.yml` matrix from Phase 7 §4. | none yet                            | —          |
| M1  | `arcp-core::{envelope, ids, error, messages}`        | `crates/arcp-core/src/{envelope,ids,error,messages/{mod,session,execution}}.rs`; `crates/arcp-core/tests/envelope_snapshots.rs` + `tests/snapshots/`; new `ErrorCode` (15 codes); new `Message` enum (18 variants); `EventSeq(u64)` newtype; `ResumeToken(String)`; W3C-32-hex `TraceId`. | §5, §6 (types), §7 (types), §8 (types), §12 | M0         |
| M2  | `arcp-core::transport` + bare WS client + stdio      | `crates/arcp-core/src/transport/{mod,memory,stdio,websocket}.rs`; native `impl Trait in trait` on `Transport`; drop `async_trait` from dep tree; `cargo features: ws (default-on), stdio (default-on)`. | §4 (Transport)                      | M1         |
| M3  | `arcp-core::store` (session-scoped event log)        | `crates/arcp-core/src/store/{mod,eventlog,schema.sql}.rs`; new schema `(session_id, event_seq, type, payload_json, ts)` keyed by `(session_id, event_seq)`; idempotency table separate. `tempfile`-backed integration tests. | §6.3 (storage primitive), §8.3 (sequence storage) | M1         |
| M4  | `arcp-runtime::server` handshake (hello/welcome/bye) | `crates/arcp-runtime/src/{server,session,auth/bearer}.rs`; `ServerBuilder`; `Session::Pending → ::Accepted` internal type-state; capability negotiation (`features` intersection); `BearerVerifier` trait + default static-token impl. | §6.1, §6.2, §6.7                    | M2, M3     |
| M5  | `arcp-runtime` resume (§6.3)                         | `crates/arcp-runtime/src/server.rs::handleResume`; rotating `resume_token` (32 random bytes); replay via `EventLog::read_since_seq`; `RESUME_WINDOW_EXPIRED` error path. Integration test `resume_after_drop`. | §6.3                                | M4         |
| M6  | `arcp-runtime` heartbeat + ack + list_jobs           | `crates/arcp-runtime/src/session.rs::{startHeartbeat, recordAck, handleListJobs}`; `AtomicI64 last_seen_at_millis`; `tokio::time::interval` with `MissedTickBehavior::Delay`; back-pressure threshold knob. | §6.4, §6.5, §6.6                    | M5         |
| M7  | `arcp-runtime::job` FSM + agent dispatch             | `crates/arcp-runtime/src/{job,context,agent}.rs`; `Job` FSM (six §7.3 states); `JobContext`; `Agent` trait with `Input`/`Output` assoc types + private `ErasedAgent`; idempotency cache (in-memory, ~24 h TTL). Tests: §7.3 FSM `proptest`. | §7.1, §7.2, §7.3, §7.4              | M6         |
| M8  | `arcp-runtime::job` events (§8)                      | `crates/arcp-runtime/src/job.rs::{emit, emitProgress, emitBody, applyCostMetric}`; all 10 reserved `EventKind` body parsers; `event_seq` allocator on `SessionState`; `progress` body (§8.2.1). | §8.1, §8.2, §8.2.1, §8.3            | M7         |
| M9  | `arcp-runtime::lease` (§9)                           | `crates/arcp-runtime/src/lease.rs`; glob compile/match; lease subset; `Clock` trait + `SystemClock`/`TestClock`; `validate_lease_op` with `LeaseOpContext { now, constraints, budget }`; `lease_constraints.expires_at` watchdog (`tokio::time::sleep_until`); `cost.budget` counters with per-currency `AtomicI64`; debounced `cost.budget.remaining` metric. | §9.1–§9.6, §10 (lease subset on delegate) | M8         |
| M10 | `arcp-runtime::subscription` + `result_chunk`        | `crates/arcp-runtime/src/{subscription,job}.rs::makeResultStream`; subscription registry keyed `(SessionId, JobId)`; `history: true` replay via `EventLog`; `ResultWriter` with `#[must_use]` enforcing monotone `chunk_seq` and `ResultMode { None, Inline, Chunked }` on `Job`; deny-cancel-from-subscriber. | §7.6, §8.4                          | M9         |
| M11 | `arcp-client` complete                               | `crates/arcp-client/src/{lib,api,reconnect}.rs`; `Client`/`ConnectedClient` (dropped public `Session<S>` per Phase 4 §6.2); `submit`, `cancel`, `subscribe` returning `Subscription: Stream` backed by `UnboundedReceiverStream`; `ack` (gated on negotiation); `list_jobs`; `JobHandle::collect_chunks` accumulator; auto-ack scheduler. Flip `clippy::expect_used = "deny"`. | §6.2, §6.5, §6.6, §7.1, §7.4, §7.6, §8.4 (client) | M10        |
| M12 | Example set rewrite (18 examples)                    | Delete `examples/*` entries from `Cargo.toml:80-122` and the 14 directory/file targets; add 36 `[[example]]` blocks per Phase 6 §2; create `examples/{submit-and-stream,delegate,resume,idempotent-retry,lease-violation,cancel,stdio,vendor-extensions,custom-auth,heartbeat,ack-backpressure,list-jobs,subscribe,agent-versions,lease-expires-at,cost-budget,progress,result-chunk}/`. CI gate: `cargo check --examples` and per-example smoke runs in CI. | All §13.x example flows             | M11        |
| M13 | Host adapters                                        | `crates/arcp-axum/`, `crates/arcp-hyper/`, `crates/arcp-tokio-tungstenite/`; per Phase 5 §1 + §3; `HostAllowlist::default()` is deny-all-except-loopback. | §4.1 (host attach), §14 (DNS-rebind) | M11        |
| M14 | `arcp-otel`                                          | `crates/arcp-otel/`; `TracedTransport<T>` decorator; W3C trace-context extract/inject in `extensions["x-vendor.opentelemetry.tracecontext"]`; attribute names byte-identical to TS (`extractAttributes` parity); `opentelemetry = "=0.27"` exact-minor pin. | §11                                 | M11        |
| M15 | `docs/` site source                                  | `docs/{00-overview, 01-quickstart, 02-concepts}.md`, `docs/03-features/<9>.md`, `docs/04-examples/<19>.md`, `docs/05-reference/<5>.md`, `docs/06-conformance.md`, `docs/99-migration-from-rfc-0001.md`; frontmatter schema validator (`scripts/check-frontmatter.sh`); anti-slop hook (`scripts/anti-slop.sh`). | none (docs)                         | M11, M12, M13, M14 |
| M16 | Diagrams                                             | `diagrams/{crates,session-fsm,job-fsm,capability-negotiation,heartbeat-ack,result-chunk}-{light,dark}.dot` (12 sources); paired SVGs checked in; `Makefile :: diagrams` target; CI `make diagrams && git diff --exit-code diagrams/`. | none (diagrams)                     | M15        |
| M17 | `CONFORMANCE.md` rewrite + conformance harness       | Rewrite `CONFORMANCE.md` mirroring `typescript-sdk/CONFORMANCE.md` row-for-row with `crates/*/src/**` file:line cites; `tests/conformance.rs` asserts every row (negotiable / constructible / round-trippable). | All §4–§15 conformance              | M11–M14    |
| M18 | `README.md` rewrite + crates.io publish prep         | Replace `README.md` per Phase 8 §4 outline (~9 sections); update `CHANGELOG.md` with the v0.1 → v1.1 wholesale-rewrite note; verify per-crate `Cargo.toml` metadata; tag and dry-run `cargo publish --dry-run` on every crate in dependency order. | none (publish)                      | M15–M17    |

## Risks — concrete, Rust-specific

| Risk                                                                                                  | Why it bites in Rust                                                                                                                                                                | Mitigation                                                                                                                                            |
| ----------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| `ResultMode` FSM on `Job` mis-races with the agent's `JobContext::emit` calls                          | The agent calls `ctx.emit(JobEvent::ResultChunk { .. })` and `ctx.emit_result(..)` from the agent task; the FSM lives on `Job` owned by the session task. Cross-task state requires `Arc<Mutex>` or an `mpsc` ordered queue. | Single `mpsc::Sender<JobOutbound>` from `JobContext` to `Job`; `Job` is the only owner of `ResultMode`. Compile-time prevention by `#[must_use] ResultWriter` consuming `self` on `finalize`. Test: `proptest` over interleaved `write_*` and `emit_result` rejects every illegal interleave. |
| §7.6 subscription stream pins inner `mpsc` across `await`                                              | `tokio::sync::mpsc::UnboundedReceiver` is `!Unpin`; calling it from within a `select!` arm without pinning fails to compile or requires `Pin<Box<_>>`.                              | `tokio_stream::wrappers::UnboundedReceiverStream` wraps the receiver and is `Unpin`. The public `Subscription` type holds the wrapper, not the raw receiver. Per Phase 4 §4.3 — no `Box::pin` allocation. |
| §9.5 lease-expiry watchdog clock races the FSM                                                        | The watchdog is `tokio::time::sleep_until(expires_at)` racing the agent future; the system clock is wall-time, but `tokio::time::sleep_until` uses `tokio::time::Instant` (monotonic-ish under the runtime). Test injection must mock both. | `Clock` trait (one method, `now() -> DateTime<Utc>`) + `TestClock` (`Arc<Mutex<DateTime<Utc>>>`). The watchdog uses `tokio::time::pause()` + `advance()`. Convert wall-time `expires_at` to `tokio::time::Instant` via `Instant::now() + duration_until(expires_at, clock.now())`. |
| `cargo-llvm-cov` undercount on `async fn` returning `impl Future`                                     | The compiler lowers `async fn` to a state-machine type; LLVM coverage attributes each branch to the state-machine's transition table, not to the source line. Some branches read as 0%-covered when they're actually exercised. | Phase 7 §1 dual-gate (lines AND regions); document branch-percentage as advisory only. If a specific module dips, switch that module to explicit `fn -> impl Future` (where lowering is more transparent). |
| OTel ecosystem 0.27 → 0.28 cadence breaks `arcp-otel`                                                  | `opentelemetry` and `tracing-opentelemetry` ship every 2–3 months and break public API roughly every minor (Phase 3 §"Tracing & OTel"). Pinned exact-minor in `arcp-otel`, but a downstream user pinning a different minor will get `cargo deny` errors. | Pin exact-minor (`= "=0.27"`) in `arcp-otel` only; document the bump cadence in `arcp-otel/README.md`; gate upgrades on `tracing-opentelemetry` release notes; `arcp-core` stays free of OTel deps. |
| `rusqlite`-bundled SQLite link failure on macOS arm64                                                  | The `bundled` feature compiles SQLite via `cc` against the host's clang; on macOS arm64 self-hosted runners we've seen `_sqlite3_*` undefined symbols when the linker picks a stale system library. | Phase 7 CI matrix includes `test-macos` (ubuntu + macos-latest). The macos job runs `cargo test -p arcp-core --features store` early so link errors surface at the test-binary build step, not at deploy time. |
| `proptest` shrinking with `compile_glob` pathological backtracking                                     | A randomly-generated lease pattern can drive `compile_glob` into superlinear backtracking; under `proptest`'s default 256 cases × per-case shrink, a single bad case stalls the test. | `proptest!` config `#![cases = 256]` per test; per-case `tokio::time::timeout(Duration::from_millis(50))` on `validate_lease_op`; the proptest itself uses bounded depth glob strategies (no `(\w+/)*` patterns). |
| §6.5 back-pressure detection deadlock                                                                  | If the per-session outbound `mpsc::channel(1024)` fills, the agent task parks at `Sender::send().await`; the session's send loop has the only `Receiver` and will only drain if the WS peer reads. A pathological slow consumer can deadlock the entire session task pipeline. | The receiver in the send loop polls with `tokio::select!` against `session_token.cancelled()`. If the bound holds for > 5 s (a knob), the runtime emits a `status { phase: "back_pressure" }` event (§6.5 SHOULD) and the agent task's `Sender::send().await` parks — but the recv loop keeps reading inbound and processing acks. The dispatcher and outbound queue are on separate tasks; they cannot deadlock each other. |

## Non-goals

Strictly out of scope for v1.1; do not land any of these without a
separate spec amendment.

- **Human-in-the-loop.** Spec §1.2 non-goal. The current `human.*`
  envelope family, `HumanInputHandler` trait, and `examples/human_input/`
  do not return.
- **Built-in tool registry.** Tool dispatch is MCP's job. The runtime
  registers *agents*, not tools. `ToolHandler`, `ToolRegistry`, and
  `examples/mcp/` are dropped.
- **Artifact storage at the runtime.** v1.1 carries `artifact_ref` as a
  job-event body shape only. The current `ArtifactStore`, retention
  sweep, and `artifact.*` envelopes are dropped.
- **HTTP/2, QUIC, MQ transports.** Spec §4 lists them as MAY; out of
  scope for v1.1 GA.
- **mTLS, OAuth2.** Spec §6.1 mandates only bearer. The current
  `AuthScheme::Mtls` and `Oauth2` variants are dropped from the SDK.
- **`signed_jwt` and `none` auth schemes in the SDK proper.** Bearer
  is the only v1.1-mandated scheme. JWKS-fetching verifiers are
  deployer-side per Phase 3 §"Errors" / §"HTTP".
- **Persistent idempotency store across restart.** TS reference
  defers it (`typescript-sdk/CONFORMANCE.md:371-376`); Rust does
  likewise. In-memory with ~24 h TTL is acceptable. Deployers swap
  the trait impl.
- **Job pause/unpause, scheduling, federation across runtimes,
  streaming-token surface for LLM outputs.** Spec §"Not in v1.1
  (deferred)" — all four.

## Open questions for the human reviewer

1. **MSRV = 1.82 confirmed?** Phase 3 picks 1.82 against Phase 4's
   1.88. The synthesis takes 1.82. If 1.88 carries a specific feature
   we depend on (Phase 4 did not enumerate one beyond "clean
   `async_trait` drop"), call it out and we'll re-pin.
2. **`subscribe()` returns `UnboundedReceiverStream` (Phase 4) — OK
   to expose `tokio_stream` in the public signature?** The
   alternative `Pin<Box<dyn Stream<Item = JobEventPayload> + Send>>`
   (Phase 2 / Phase 6 / Phase 8 wording) opaques the implementation
   at a one-allocation cost. Phase 4's pick is the right call for
   downstream `.filter_map`/`.then` ergonomics; we want a sanity
   check before M11 freezes the signature.
3. **v0.1 → v1.1 publish strategy.** Three options for the existing
   crate name `arcp` on crates.io (currently at v0.1.0):
   - (a) `0.2.0` "wrong-protocol, see migration" release on the
     legacy branch, then jump to `1.0.0` on the rewrite.
   - (b) Yank v0.1.0 and publish `1.0.0` from M18.
   - (c) Keep v0.1.0 published, mark v1.0.0 as a deliberate
     wire-incompatible bump in the CHANGELOG.
   We recommend **(c)**: less destructive, lets the wholesale-rewrite
   advisory sit in the README of v1.0.0+. Confirm.
4. **`arcp-cli` as a separate crate (Phase 4) — confirm.** Phase 3
   put the CLI inside the `arcp` umbrella. The synthesis takes Phase
   4 (separate `arcp-cli` so the umbrella stays library-only).
   Trade-off: users running `cargo install arcp` no longer get the
   binary — they must `cargo install arcp-cli`. The README quickstart
   in Phase 8 §4 mentions `cargo run --example` rather than `arcp
   serve`, so the public-facing entry is unchanged.
5. **`cargo-mutants` nightly scope expands or stays at two files?**
   Phase 7's scope is `arcp-core/src/messages/` + `arcp-runtime/src/lease.rs`.
   Expanding to `arcp-runtime/src/job.rs` and `arcp-runtime/src/subscription.rs`
   would catch §7.6/§8.4 H-risk regressions but doubles the nightly
   wall-clock. We recommend **defer expansion** until the v1.1 GA
   nightly run has six months of clean signal.
6. **Host adapters land in M13 or split across two milestones?**
   Phase 5 lists four crates. `arcp-axum` and `arcp-hyper` are
   server-side and share `HostAllowlist`; `arcp-tokio-tungstenite` is
   client-side. We bundled all three into M13. Alternative: split
   M13 into M13a (client-side WS shim, no host) and M13b (server-side
   axum + hyper). Bundling is faster; the split is reviewable. Default
   bundled.
7. **DNS-rebind default — deny-all-except-loopback (Phase 5) vs
   permissive (TS reference).** Phase 5 §3.3 inverts the TS posture.
   This is a deliberate Rust-side security tightening; confirm it's
   the right call. The escape hatch
   (`HostAllowlist::permissive_for_dev()`) preserves the dev
   ergonomics with a `tracing::warn!` on every upgrade.
8. **Deletion vs preservation of the `RFC-0001-v2.md` document.** The
   document defines the wire we're abandoning. Two views: (a) delete
   it from the repo (the spec is `spec/docs/draft-arcp-02.1.md` now);
   (b) keep it under `docs/legacy/RFC-0001-v2.md` with a deprecation
   header so the v0.1 crate's docs.rs links remain readable. We
   recommend **(b)** for one release cycle, then delete in v1.1.x.
