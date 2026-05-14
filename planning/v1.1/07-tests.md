# Phase 7 — Test Strategy (Rust SDK, ARCP v1.1)

Scope: the test plan for the post-rewrite Rust SDK. The current
`tests/` tree (14 files, listed at the bottom of this document) targets
`RFC-0001-v2.md` — wrong protocol per `planning/v1.1/02-current-audit.md`.
Nearly every case is discarded; the stack (`insta`, `tokio-test`,
`tempfile`, `tests/common/mod.rs` fixture pattern) survives.

Coverage floor: **87 % lines AND 87 % regions** per `cargo-llvm-cov`,
up from the **85 %** line floor cited in `rust-sdk/README.md:36`.
Both numbers gate CI via `--fail-under-lines 87 --fail-under-regions 87`.

The conformance row source of truth is
`typescript-sdk/CONFORMANCE.md`. The Rust `tests/conformance.rs`
mirrors that file row-for-row; if a row is added there and not here,
CI fails.

---

## 1. Stack

| Concern                     | Crate / tool                          | Where                                                         |
| --------------------------- | ------------------------------------- | ------------------------------------------------------------- |
| Test runner                 | `cargo test`                          | default                                                       |
| Async test attribute        | `#[tokio::test]`                      | single-threaded by default                                    |
| Multi-thread async (opt-in) | `#[tokio::test(flavor = "multi_thread", worker_threads = 4)]` | heartbeat watchdog, subscriber fan-out, cancel-grace race |
| Wire-shape regression       | `insta` 1 (`json` feature)            | `crates/arcp-core/tests/*.rs` + `tests/snapshots/`            |
| Property-based              | `proptest` 1 + `proptest-derive`      | FSM, `event_seq` monotonicity, lease subset                   |
| Time control                | `tokio::time::pause` + `advance`      | every timeout / heartbeat / grace test                        |
| Tracing assertions          | `tracing-test` 0.2                    | §6.4 watchdog emission, §7.4 cancellation log line            |
| Temp filesystem             | `tempfile` 3                          | SQLite event-log fixtures, stdio transport pipes              |
| Coverage                    | `cargo-llvm-cov`                      | CI gate                                                       |
| Mutation                    | `cargo-mutants` (scoped, nightly job) | `arcp-core::messages`, `arcp-runtime::lease` only             |

### Threading rule

Default is `#[tokio::test]` (current-thread runtime). The current-thread
runtime spawns in < 50 µs and does not exercise `tokio`'s work-stealing
scheduler; the FSM, lease, and envelope tests do not race anything that
the scheduler would surface. Opt into
`#[tokio::test(flavor = "multi_thread", worker_threads = 4)]` only when
the test asserts a property that *requires* concurrent execution:

- `arcp-runtime::session` heartbeat watchdog (the `AtomicI64`
  `last_seen_at` update path described in `02-current-audit.md:111`
  must race against the watchdog tick).
- `arcp-runtime::subscription` fan-out (`SessionContext::send`'s fan-out
  block must run while a subscriber holds its mpsc receiver across an
  `await` point).
- `arcp-runtime::server` cancel-grace `tokio::select!` race
  (`02-current-audit.md:117`).

Justification: the multi-thread runtime adds ~3–5× spawn cost and
serialises poorly under `--test-threads`; making it the default
inflates wall-clock CI time without changing what's tested.

### `insta` keys

One `.snap` per v1.1 envelope variant, locked to a fixed `MessageId`
and fixed `timestamp` so the recorded JSON is byte-stable:

```
session.welcome.snap         session.jobs.snap
session.ping.snap            session.pong.snap
session.ack.snap             session.bye.snap
session.error.snap           job.submit.snap
job.accepted.snap            job.cancel.snap
job.event.log.snap           job.event.thought.snap
job.event.tool_call.snap     job.event.tool_result.snap
job.event.status.snap        job.event.metric.snap
job.event.artifact_ref.snap  job.event.delegate.snap
job.event.progress.snap      job.event.result_chunk.snap
job.result.inline.snap       job.result.chunked.snap
job.error.cancelled.snap     job.error.timed_out.snap
job.error.lease_expired.snap job.subscribed.snap
job.unsubscribe.snap
```

The fixed-clock + fixed-id trick: every snapshot test calls
`fixtures::frozen()` which returns a `Clock` impl whose `now()` is
`2026-05-14T00:00:00Z` and an `IdGen` whose `next()` produces a fixed
ULID per call-site. Without this, every snapshot rewrites on every run
and `insta review` becomes noise.

### `proptest` over `quickcheck`

`proptest` is selected because (a) `proptest-derive` can `#[derive]`
strategies for the v1.1 message enum and `Lease`/`LeaseConstraints`
shapes — `quickcheck` requires hand-written `Arbitrary`; and (b)
`proptest`'s shrinking produces minimal failing envelope trees, which
matters for the §5.1 "ignore unknown fields" fuzz path where the input
is a recursive `serde_json::Value`. `quickcheck` shrinks numerically
only and would surface a 4 KB blob instead of the offending key.

### `cargo-llvm-cov` — coverage gate

Phase 3 (`planning/v1.1/03-libraries.md:140-148`) ratifies
`cargo-llvm-cov`. The README's prior **85 %** line target is the
baseline; this phase raises it to **87 % lines AND 87 % regions**.
Regions are stricter than lines: a single `if let Some(_) = ...` line
with both arms unexercised counts as one uncovered line but two
uncovered regions. The dual gate prevents the "covered the happy path,
forgot the `else`" pattern.

CI command:

```
cargo llvm-cov --workspace --all-features \
  --fail-under-lines 87 --fail-under-regions 87 \
  --ignore-filename-regex 'tests/|examples/|benches/|crates/arcp/src/bin/'
```

### `cargo-mutants` — scoped yes

Phase 3 (`planning/v1.1/03-libraries.md:168-177`) said no for v1.1
because a full-workspace run is 20–60 min on GH free-tier. Test
strategy revisits and accepts a **scoped** run: only
`crates/arcp-core/src/messages/` and `crates/arcp-runtime/src/lease.rs`.

Defence: those two modules are the highest-leverage in the SDK —
`messages` defines the entire wire surface (every v1.1 envelope round
trips through it) and `lease.rs` is the only authority-bearing path
(§9.5, §9.6, §10.2 all go through `validate_lease_op`). A mutation in
`messages` that survives means a v1.1 envelope variant is untested; a
mutation in `lease.rs` that survives is a security bug. Out of those
two modules, the full mutate-and-test run is ~6 min on GH Actions
Ubuntu Standard (4 vCPU), measured against the Phase 3 estimate scaled
by file count (the two modules are ~12 % of the source tree).

Run config:

```
cargo mutants --in-place \
  --package arcp-core --file 'src/messages/**.rs' \
  --package arcp-runtime --file 'src/lease.rs' \
  --timeout 300
```

Scheduled nightly on `main`, non-blocking on PR — surviving mutants
file an issue rather than fail the merge.

---

## 2. Layered plan

Bottom-up. Each layer lists the crate, the file, the proof obligation,
and the spec § / TS path that pins the contract.

### 2.1 Envelope unit — `crates/arcp-core/src/envelope.rs`

Test file: inline `#[cfg(test)] mod tests` inside `envelope.rs`, plus
`crates/arcp-core/tests/envelope_snapshots.rs` for `insta`.

Obligations:

- `arcp` field MUST serialise as the string `"1"` (§5.1, mirrors TS
  `packages/core/src/version.ts:PROTOCOL_VERSION`).
- Every v1.1 envelope variant round-trips
  `serialize → bytes → deserialize → PartialEq`.
- Unknown top-level fields are preserved by the `RawEnvelope` layer and
  dropped by the typed `Envelope` layer (§5.1 + `02-current-audit.md`
  "two-layer typed/raw envelope split").
- `insta` snapshot of each variant's wire shape, keyed to the fixed
  `MessageId` + fixed `timestamp` described above.

Negative cases (each a separate `#[test]`):

- `arcp: "1.0"` → `INVALID_REQUEST` (the current SDK emits `"1.0"` per
  `02-current-audit.md` line 33; this guard exists to fail the rewrite
  loudly if the regression returns).
- `arcp: 1` (number, not string) → parse error.
- Missing `id` → parse error.
- `event_seq: -1` → parse error (`u64`, §5.1).
- `event_seq` on `session.ping` → reject (§6.4 forbids — pings are not
  counted).

### 2.2 Message unit — `crates/arcp-core/src/messages/`

Test files: `tests/messages_session.rs`, `tests/messages_execution.rs`,
`tests/messages_agent_ref.rs`.

Obligations from §7.5, §9.5, §9.6:

| Test                                  | Asserts                                                                                                |
| ------------------------------------- | ------------------------------------------------------------------------------------------------------ |
| `parse_agent_ref_bare`                | `parse_agent_ref("code-refactor")` → `AgentRef { name: "code-refactor", version: None }`               |
| `parse_agent_ref_versioned`           | `parse_agent_ref("code-refactor@1.0.0")` → `version: Some("1.0.0")`                                    |
| `parse_agent_ref_rejects_upper`       | `"CodeRefactor"` → `InvalidRequest` (grammar `[a-z0-9]...`, §7.5)                                      |
| `parse_agent_ref_rejects_empty`       | `""` and `"@1.0.0"` → `InvalidRequest`                                                                 |
| `parse_budget_amount_usd`             | `"USD:1.42"` → `BudgetAmount { currency: "USD", value: Decimal::from_str("1.42") }`                    |
| `parse_budget_amount_credits`         | `"credits:1000"` round-trips                                                                           |
| `parse_budget_amount_rejects_negative`| `"USD:-1.00"` → `InvalidRequest` (§9.6 "Negative values MUST be rejected")                             |
| `lease_constraints_rejects_past`      | `expires_at` in the past relative to injected `Clock::now()` → `InvalidRequest` (§9.5)                 |
| `lease_constraints_rejects_naive`     | `expires_at` without trailing `Z` → `InvalidRequest` (§9.5 "MUST be ISO 8601 with `Z`")                |
| `progress_body_rejects_negative_current` | §8.2.1 "`current` MUST be non-negative"                                                             |
| `result_chunk_rejects_bad_encoding`   | `encoding: "hex"` → parse error (§8.4 enum `{utf8, base64}`)                                            |

### 2.3 State machine — `crates/arcp-runtime/src/job.rs`

Test file: `crates/arcp-runtime/tests/fsm.rs`.

§7.3 states: `pending`, `running`, `success`, `error`, `cancelled`,
`timed_out`. The TS reference encodes this as `JOB_TRANSITIONS` in
`packages/runtime/src/job.ts` — same shape for Rust.

`proptest` strategy: generate a `Vec<JobEvent>` of length 1..=32 from
the alphabet `{ Submit, Accept, Emit(kind), Cancel, Timeout, Result,
Error }`, replay against a fresh `Job`, assert:

1. No illegal transition (e.g., `Result` from `pending`, `Cancel` from
   `success`) is accepted — the FSM returns `Err(InvalidTransition)`.
2. Once a terminal state is reached, every subsequent event is rejected
   with `Err(JobAlreadyTerminal)`.
3. The terminal-state distribution over 10 000 random sequences
   produces at least one of each of the four terminals — a smoke check
   that the FSM is actually reachable, not stuck.

### 2.4 `event_seq` monotonicity — `crates/arcp-runtime/src/session.rs`

Test file: `crates/arcp-runtime/tests/event_seq.rs`.

§5.1 + §8.3 require `event_seq` to be session-scoped, strictly
monotonic, gap-free across reconnects, and to skip `session.ping`,
`session.pong`, `session.ack` (per §6.4, §6.5).

`proptest` strategy: generate interleaved traffic
`Vec<Either<OutboundEvent, NonCountedControl>>` and assert:

- The emitted `event_seq` sequence on the outbound side is `0, 1, 2,
  ...` with no gaps.
- The non-counted control messages (`ping`, `pong`, `ack`) do not
  advance the counter.
- After a simulated reconnect (`SessionContext::set_event_seq(N)`), the
  next emitted seq is `N + 1`.

### 2.5 Lease enforcement — `crates/arcp-runtime/src/lease.rs`

Test file: `crates/arcp-runtime/tests/lease.rs`.

§9 surface: glob canonicalization (§9.2 + §14), subset (§9.4),
`expires_at` (§9.5), budget decrement (§9.6).

Concrete cases:

| Test                                  | Asserts                                                                                   |
| ------------------------------------- | ----------------------------------------------------------------------------------------- |
| `glob_single_segment`                 | `fs.read:/etc/*` matches `/etc/hosts`, not `/etc/ssl/openssl.cnf`                         |
| `glob_recursive`                      | `fs.read:/etc/**` matches `/etc/ssl/openssl.cnf`                                          |
| `canonicalize_dotdot`                 | `fs.read:/etc/../etc/hosts` canonicalises to `/etc/hosts` before glob check (§14)         |
| `canonicalize_url`                    | `net.fetch:https://Example.com/` normalises host to lowercase, default port stripped     |
| `subset_strict`                       | Child `fs.read:/etc/passwd` is subset of parent `fs.read:/etc/**`; reverse is not        |
| `subset_budget`                       | Child `cost.budget` per currency ≤ parent's *remaining* (§9.4 + CONFORMANCE.md §9.4 row) |
| `subset_proptest`                     | `proptest` over random `(parent, child)` lease pairs: subset transitive, anti-symmetric  |
| `expires_at_future_required`          | Past `expires_at` at acceptance → `InvalidRequest`                                       |
| `expires_at_evaluated_at_op`          | With injected `Clock`, advancing past `expires_at` causes `validate_lease_op` → `LeaseExpired` |
| `budget_decrement_positive`           | `metric { name: "cost.tokens", unit: "USD", value: 0.10 }` decrements `USD` counter      |
| `budget_decrement_negative_rejected`  | Negative `value` does NOT decrement; emits `InvalidRequest`                              |
| `budget_exhausted`                    | Counter reaching `0.00` causes next op → `BudgetExhausted`                               |

Clock injection: `Lease`-validation entry points take
`&dyn Clock` (trait with one method, `now() -> DateTime<Utc>`). A
`TestClock` implementation wraps `Arc<Mutex<DateTime<Utc>>>` so tests
advance it deterministically without `tokio::time` (the lease path is
not async).

### 2.6 Integration — `tests/integration_memory.rs` + `tests/integration_ws.rs`

`integration_memory.rs`: spin up an `ARCPRuntime` and an `ARCPClient`
joined by `MemoryTransport` (carry-over per `02-current-audit.md` line
80). One test per `examples/` scenario:

| Test                       | Scenario (TS analogue under `typescript-sdk/examples/`) |
| -------------------------- | ------------------------------------------------------- |
| `submit_and_stream`        | `submit-and-stream/`                                    |
| `cancel_within_grace`      | `cancel/` (§7.4)                                        |
| `idempotent_retry`         | `idempotent-retry/`                                     |
| `lease_violation`          | `lease-violation/`                                      |
| `delegate_subset_ok`       | `delegate/`                                             |
| `delegate_subset_violation`| §10.2                                                   |
| `resume_after_drop`        | `resume/`                                               |
| `heartbeat_pong_replied`   | `heartbeat/`                                            |
| `heartbeat_lost`           | §6.4 two-interval close                                 |
| `ack_back_pressure`        | `ack-backpressure/`                                     |
| `list_jobs_same_principal` | `list-jobs/`                                            |
| `subscribe_history_replay` | `subscribe/`                                            |
| `agent_version_resolved`   | `agent-versions/`                                       |
| `lease_expires_during_run` | `lease-expires-at/`                                     |
| `budget_exhausted_metric`  | `cost-budget/`                                          |
| `progress_event_visible`   | `progress/`                                             |
| `result_chunk_assembled`   | `result-chunk/`                                         |

`integration_ws.rs`: re-run a subset (`submit_and_stream`,
`cancel_within_grace`, `resume_after_drop`, `result_chunk_assembled`)
over `WebSocketTransport` bound to `127.0.0.1:0` (port 0 → kernel
assigns; the test reads `local_addr()` back). This catches frame
boundary bugs in `tokio-tungstenite` 0.24 that `MemoryTransport` hides
(`MemoryTransport` passes whole envelopes; the WS path splits on text
frames per §4.1).

### 2.7 Conformance harness — `tests/conformance.rs`

Mirrors `typescript-sdk/CONFORMANCE.md` row-for-row. The test body is a
table — `&'static [ConformanceRow]` — with one row per spec § listed in
CONFORMANCE.md. Each row asserts at least one of:

- **Negotiable**: the named feature flag (`heartbeat`, `ack`,
  `list_jobs`, `subscribe`, `agent_versions`, `lease_expires_at`,
  `cost.budget`, `progress`, `result_chunk`) appears in
  `V1_1_FEATURES` and `intersect_features([flag], [flag])` returns it.
- **Constructible**: every code in the 15-entry error taxonomy
  (§12) constructs via its named variant and serialises to the
  spec-pinned wire string.
- **Round-trippable**: the named message variant
  (`session.hello`, …, `job.subscribed`) round-trips through the
  `Envelope` enum and the wire bytes match its `insta` snapshot.

This file is the spec-section index. When `typescript-sdk/CONFORMANCE.md`
gains a row, this file must gain a row — enforced manually but visible
in PR diff. The Rust file links to the TS file in a doc-comment header.

---

## 3. Cancellation tests under `tokio`

No real sleeps. Real sleeps in CI are the #1 source of flake on shared
runners. The pattern:

```rust
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cancel_within_grace() {
    tokio::time::pause();
    let (rt, client, _t) = fixtures::pair(); // MemoryTransport
    let handle = client.submit("slow-agent", json!({})).await.unwrap();

    // Agent emits one log, then awaits cancel-token.
    tokio::time::advance(Duration::from_millis(10)).await;
    client.cancel(handle.job_id(), "user").await.unwrap();

    // Advance just past the 30s grace; assert job.error{cancelled}.
    tokio::time::advance(Duration::from_secs(31)).await;
    let final_evt = tokio::time::timeout(Duration::from_secs(5),
        handle.terminal()).await.expect("hang").unwrap();
    assert_eq!(final_evt.final_status(), FinalStatus::Cancelled);
}
```

Three rules enforced across every async test:

1. **`tokio::time::pause()` first.** Every timeout / heartbeat / grace
   test starts with `tokio::time::pause()` and advances via `advance`.
   Real elapsed wall time in the body is ≤ 1 ms.
2. **Outer `tokio::time::timeout(Duration::from_secs(5), ...)`.** Wraps
   the test future. With paused time, a hung future doesn't advance; the
   timeout fires against real elapsed time, so a bug fails fast instead
   of CI killing the runner at the 10-min mark.
3. **Cancellation race covers both arms of `tokio::select!`.** The §7.4
   grace window must resolve via *either* the agent future completing
   (it noticed the cancel token) *or* the grace deadline firing. Two
   tests: one where the agent yields immediately on cancel, one where it
   ignores cancel and the deadline wins.

Heartbeat tests follow the same pattern, advancing two full intervals
to trigger `HEARTBEAT_LOST` (§6.4) and asserting the `session.error`
envelope arrives without ever touching real time.

---

## 4. CI matrix

| Job                                        | Toolchain | Features              | OS              | Required |
| ------------------------------------------ | --------- | --------------------- | --------------- | -------- |
| `test-stable-default`                      | stable    | default               | ubuntu-latest   | yes      |
| `test-stable-all`                          | stable    | `--all-features`      | ubuntu-latest   | yes      |
| `test-stable-min`                          | stable    | `--no-default-features` | ubuntu-latest | yes      |
| `test-msrv`                                | 1.82      | default               | ubuntu-latest   | yes      |
| `test-beta`                                | beta      | `--all-features`      | ubuntu-latest   | **no**   |
| `test-macos`                               | stable    | default               | macos-latest    | yes      |
| `coverage`                                 | stable    | `--all-features`      | ubuntu-latest   | yes      |
| `mutants-scoped` (nightly cron)            | stable    | default               | ubuntu-latest   | **no**   |

Rationale:

- **MSRV pinned to 1.82** per `planning/v1.1/03-libraries.md:181`.
  Lower toolchains lack the `&raw const` + precise-capturing required
  by the `subscribe()` return type (`02-current-audit.md:119`).
- **Beta non-required.** Surfaces upcoming breakages (`tokio` minor
  bumps, `serde` edge cases) on the schedule the toolchain releases on,
  without blocking merges when the breakage is upstream.
- **Three feature rows.** `--no-default-features` proves the core crate
  compiles without `transport-ws`, `otel`, or `store` —
  `02-current-audit.md` shows that today's crate has features that
  silently no-op when disabled. `--all-features` is the everything-on
  shape that `cargo-llvm-cov` uses for coverage attribution.
- **No Windows.** Stdio transport (`crates/arcp-core/src/transport/stdio.rs`,
  carry-over per `02-current-audit.md:81`) uses NDJSON over async
  `AsyncRead`/`AsyncWrite`. On Windows the line-ending and console
  attach semantics differ in ways that are out of scope for an ARCP SDK
  to fix; deployer-side Windows support is gated by the deployer's own
  pipe shape. macOS is the second-OS-floor (transport tests pass on
  Darwin; `wss://` loopback behaves the same as on Linux).
- **`coverage` is its own job** because `cargo-llvm-cov` requires
  rebuilding the test binaries with `-Cinstrument-coverage` and merging
  profraw files; doing it inside `test-stable-all` would double the
  job's wall time.

---

## 5. Minimum to hit 87 %

### Cheap-to-cover modules (target ≥ 95 %)

| Module                          | Why cheap                                                                                  |
| ------------------------------- | ------------------------------------------------------------------------------------------ |
| `arcp-core::envelope`           | One `serde` round-trip test per variant covers the entire module; ~25 variants × 1 test.   |
| `arcp-core::messages::*`        | Same shape as envelope. Negative cases come almost free (§7.5, §9.5, §9.6 parse rejects).  |
| `arcp-core::error`              | 15 codes × 1 construction × 1 wire-roundtrip = 30 lines of test for ~150 lines of source.  |
| `arcp-core::ids`                | Newtype machinery; ULID `parse → format → eq` per type. `EventSeq` is `u64`-equivalent.    |
| `arcp-core::extensions`         | `x-vendor.*` allow / `unknown` drop is a 6-case match.                                     |

Together these are ≈ 40 % of source line count and trivially clear
≥ 95 %. The lift to 87 % overall comes from the next bucket.

### Expensive modules (target ≥ 80 %)

| Module                          | Why expensive                                                                                                                                                                                |
| ------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `arcp-runtime::server`          | Dispatcher: 18 message variants × {happy, error, feature-gated-off, permission-denied} branches. Coverage by `tests/integration_memory.rs` happy paths + targeted negative tests per branch. |
| `arcp-client::api`              | Type-state shell (`Session<Unauthenticated>` → `Session<Authenticated>`). Type-state transitions are largely compile-time; runtime coverage focuses on `JobHandle` and the event stream.    |
| `arcp-runtime::subscription`    | H-risk per `02-current-audit.md:119`. Fan-out path requires the multi-thread runtime; covered by `subscribe_history_replay` + a `proptest`-driven concurrent-subscriber stress test.        |
| `arcp-runtime::job` + `lease`   | The H-risk modules. §9.5 watchdog + §9.6 budget interplay needs deterministic-clock + paused-time tests. Coverage by `tests/lease.rs` + `tests/integration_memory.rs::lease_expires_during_run`. |
| `arcp-core::store::eventlog`    | SQLite; carry-over per `02-current-audit.md:77`. Schema changed for session-scoped `event_seq`. `tempfile` for ephemeral DB; cover `append`, `read_since_seq`, idempotency `INSERT OR IGNORE`. |

### Carve-outs

`cargo-llvm-cov`'s `--ignore-filename-regex` covers the policy. Excluded
paths:

- `tests/`, `examples/`, `benches/` — not production code.
- `crates/arcp/src/bin/` — the CLI entry-point's argv-parsing branches
  are exercised by `tests/cli.rs` but the `main()` wiring is `panic!`-on-error
  glue we choose not to instrument.
- Inline `#[cfg(not(coverage))]` blocks: any branch that wraps a kernel
  error path that cannot be triggered without root privileges. Two
  known offenders:
  - `arcp-core::transport::websocket` `bind()` failure on `0.0.0.0:1`
    (priv port — only reachable as root).
  - `arcp-core::transport::stdio` `EBADF` on a parent-closed stdin —
    not reachable from within the test binary's own stdio.

Exclusion policy: a `#[cfg(not(coverage))]` annotation on any block
MUST be paired with a doc comment naming the syscall and the privilege
or environment requirement. Coverage CI passes `--cfg coverage` so the
annotation activates only under instrumentation. No silent exclusions.

---

## 6. Risks

| Risk                                                                                                       | Mitigation                                                                                                                                       |
| ---------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| `insta` snapshot drift from non-deterministic `ulid::Ulid::new()` and `chrono::Utc::now()`                  | `fixtures::frozen()` Clock + IdGen, mandatory in every snapshot test; lint via `grep -nF 'Ulid::new' tests/` in CI.                              |
| `tokio::time::pause()` deadlock when an `await` in the test body is on a non-time future                    | Outer `tokio::time::timeout(Duration::from_secs(5), ...)` wrapper; the timeout is on *wall* time, not paused virtual time, so it always fires.    |
| `tokio-tungstenite` 0.24 frame-boundary regression hidden by `MemoryTransport`                              | `integration_ws.rs` re-runs a subset of integration cases over a `127.0.0.1:0` loopback listener.                                                |
| `proptest`-generated lease pairs that exercise pathological glob backtracking in `compile_glob` (DoS risk) | `proptest!` config `#![cases = 256]` per test; per-case timeout via `tokio::time::timeout` on the validate call.                                  |
| `cargo-llvm-cov` undercount on `async fn` returning `impl Future`                                          | Use `--branch` is not yet stable; rely on the dual lines+regions gate. Document that branch-level numbers in the report are advisory.            |
| `rusqlite`-bundled SQLite linkage failure on macos-latest arm64                                            | The `bundled` feature is pinned in `planning/v1.1/03-libraries.md:242`; CI macos-latest job runs `cargo test -p arcp-core --features store` to surface link errors at the test-binary build step. |
| Mutation testing wall time creep as `messages/` grows                                                      | Scope is two named files (`crates/arcp-core/src/messages/`, `crates/arcp-runtime/src/lease.rs`); enforced by an explicit `--file` allowlist, not a glob. New files do not enter the mutate set automatically. |

---

## 7. What carries over from `tests/`

Keep:

- `tests/common/mod.rs` — the fixture-module pattern (every integration
  test does `mod common; use common::*;`). Rewrite the contents around
  the v1.1 envelope shape.
- `tests/snapshots/` — the directory layout (one `.snap` per
  `insta`-tested function). Delete every existing `.snap`; the recorded
  envelopes are wrong-protocol per `02-current-audit.md`.

Discard (the case set, not the file names — the rewrite produces a new
set entirely):

- `tests/handshake.rs`, `tests/handshake_ws.rs` — they test the 4-step
  `session.open` / `session.challenge` / `session.authenticate` /
  `session.accepted` handshake (`02-current-audit.md` §6). v1.1 is
  2-step `session.hello` / `session.welcome`. Replaced by the
  `submit_and_stream` integration test + the `session.welcome` snapshot.
- `tests/permission_challenge.rs`-style tests — the dynamic
  permission/lease lifecycle is dropped per `02-current-audit.md` line
  69. Leases are immutable at acceptance; there is no challenge flow.
- `tests/human_input.rs` — HITL is out of v1.1 scope (§1.2 non-goal,
  `02-current-audit.md:165`).
- `tests/artifact.rs`, `tests/artifact_dispatch.rs` — artifact store is
  dropped (`02-current-audit.md:172`); `artifact_ref` becomes a
  `job.event` body shape covered by the envelope tests.
- `tests/subscription.rs`, `tests/subscription_dispatch.rs` — current
  multi-axis filter-engine subscriptions are gone; v1.1 subscriptions
  are per-`job_id` only and the replacement is `tests/conformance.rs`
  §7.6 row + the `subscribe_history_replay` integration test.
- `tests/extension_unknown.rs` — `arcpx.*` namespace is wrong
  (`02-current-audit.md:178`); replaced by an envelope unit test on
  `x-vendor.*` parse-and-ignore.
- `tests/resume.rs` — wrong resume shape (`02-current-audit.md` §6.3);
  replaced by `resume_after_drop` integration test against the new
  `last_event_seq`-driven replay.
- `tests/cancellation.rs` — wrong wire (`cancel { target, target_id,
  reason, deadline_ms }`); replaced by `cancel_within_grace` per
  Section 3 above.
- `tests/envelope_snapshots.rs` — the file name survives but every
  recorded snapshot is wrong-protocol (the three existing
  `tests/snapshots/*.snap` are `log`, `metric`, and `ping` v0.x
  envelopes); replaced by the variant list in Section 1.
- `tests/job_lifecycle.rs`, `tests/runtime_dispatch.rs`,
  `tests/artifact_dispatch.rs` — replaced by `tests/fsm.rs`
  (Section 2.3) and `tests/integration_memory.rs` (Section 2.6).
