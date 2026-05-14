# Phase 3 — Library & Crate Selection (v1.1)

Scope: one decision per concern, one rejected alternative per decision,
one line on recency. Every paragraph cites a spec §, a TS path, a path
inside this SDK, or a named crate. The bar set by `Cargo.toml`
lines 124–140 (`unsafe_code = deny`, `unwrap_used = deny`, clippy
pedantic + nursery) is the hard floor and is not loosened anywhere
below.

## Serde stack

Pick `serde` 1 + `serde_json` 1 (already in `Cargo.toml:54-55`). Reject
`simd-json`: every wire frame in ARCP v1.1 is a single envelope under
4 KB — `session.hello`, `job.submit`, `job.event` (§5, §6.2, §8.1) — and
`simd-json` only outperforms `serde_json` on multi-MB documents; it also
ships `unsafe` SIMD intrinsics that fail the `unsafe_code = deny` lint
(`Cargo.toml:125`). `serde_json` shipped in the last 12 months and is
stdlib-grade; `serde` likewise. Use `serde(tag = "type", content =
"payload")` for the message enum and `#[serde(deny_unknown_fields)]`
*only* on body shapes that the spec forbids extending (e.g. §8.2.1
`progress` body) — the envelope itself MUST tolerate unknowns per §5.1.

## Async runtime

Pick `tokio` 1 with the feature set already in `Cargo.toml:39-50`.
Reject runtime-agnostic (`futures` only): the WebSocket transport pins
to `tokio-tungstenite` (`Cargo.toml:69`), heartbeats need
`tokio::time::interval` for the §6.4 two-interval watchdog, and the
back-pressure path (§6.5 `back_pressure` status event) wants
`tokio::sync::mpsc` with bounded capacity so a slow consumer applies
backpressure to the producer instead of growing the buffer. `tokio`
shipped in the last 12 months. Keep `tokio-util` for
`CancellationToken` (§7.4 grace-period cancel) and `tokio-stream` for
the `Stream` adapters that back `JobHandle::events()` (audit
`02-current-audit.md` §7.6, H-risk).

## WebSocket

Pick `tokio-tungstenite` 0.24 (already in `Cargo.toml:69`, optional
behind `transport-ws`). Reject `fastwebsockets` (Cloudflare): it
optimises for HTTP/1.1 server upgrade throughput and exposes raw frame
internals; the SDK does not host an HTTP server, and the spec mandates
text frames carrying NDJSON envelopes (§4, §5) where frame-level perf
is a rounding error against `serde_json` parse cost. Reject `axum`'s
`WebSocketUpgrade`: that would force every embedder to mount the SDK
inside `axum`, which collides with `02-current-audit.md`'s "Phase 5
hosting must remain framework-agnostic" line. **Decision: the SDK
exposes a `Transport` trait wrapping a `Stream<Item =
Result<Envelope>>` + `Sink<Envelope>` pair (already shaped this way in
`src/transport/mod.rs`). It does NOT ship a `tower::Service`.** A
`tower::Service<Envelope, Response = ()>` is a poor fit because §8 is
fire-and-forget event emission, not request/response — wrapping it in
`Service` adds a `Poll<Ready>` indirection the embedder pays for
nothing. `tokio-tungstenite` 0.24 shipped in the last 12 months.

## HTTP

No client-side HTTP. v1.1 §6.1 specifies bearer-token auth on
`session.hello.payload.auth.token` only; there is no JWKS fetch
mandated by the spec, no OAuth flow, no token-introspection endpoint.
Reject `reqwest`: 90+ transitive crates for a use we do not have.
Reject `hyper` 1.x: same logic, lower-level, same outcome. If a
deployer needs JWKS rotation they can ship their own
`Authenticator` impl (the trait survives from `src/auth/mod.rs`,
`02-current-audit.md` line 73) and pull `reqwest` themselves — keep it
out of the SDK proper, mirroring the TS choice in
`typescript-sdk/packages/core` which has no HTTP client either.

## Tracing & OTel

Pick `tracing` 0.1 + `tracing-subscriber` 0.3 (already in
`Cargo.toml:57-58`). For §11 OTel attributes (`arcp.lease.expires_at`,
`arcp.budget.remaining`), add `opentelemetry` 0.27 +
`opentelemetry_sdk` 0.27 + `tracing-opentelemetry` 0.28. Reject the
`opentelemetry-otlp` exporter as a direct dependency — gate it behind
an `otel-otlp` feature so the core crate stays exporter-agnostic; the
TS reference splits the same way (`typescript-sdk/packages/middleware`
holds OTel glue, not `core`). Recency: the OTel-Rust crates ship every
2–3 months and break the public API roughly every minor (0.26 → 0.27
moved `Tracer` traits). Pin to **exact** minors
(`opentelemetry = "=0.27"`, `tracing-opentelemetry = "=0.28"`) in
`arcp-otel` and only bump in coordination with
`tracing-opentelemetry`'s release notes; do NOT pin in `arcp-core` so
the core stays exporter-free.

## Errors

Pick `thiserror` 2 inside every `arcp-*` crate (already in
`Cargo.toml:56`). **`anyhow` is forbidden in `arcp-*` crates.** The
rule: errors that cross the crate boundary are part of the API and
need a typed enum that maps to §12's 15-code taxonomy; `anyhow` erases
the variant past that seam and a downstream embedder cannot
`match err.code()` on an `anyhow::Error`. `anyhow` is allowed in
`examples/` and `tests/` (binary-shaped, "past the seam" code) but the
lib crates set `deny(clippy::wildcard_imports)` to keep accidental
`use anyhow::*` out. `thiserror` shipped in the last 12 months.

## IDs

Pick `ulid` 1 for `MessageId`, `SessionId`, `JobId`, `IdempotencyKey`
(already in `Cargo.toml:59`; newtype machinery survives per
`02-current-audit.md` line 62) and reject `rusty_ulid` (last release
>18 months ago — disqualifies under the recency rule). For `TraceId` /
`SpanId`, the W3C trace-context spec (§11) requires 32-hex / 16-hex
opaque IDs. Use a hand-rolled newtype over `[u8; 16]` / `[u8; 8]` with
`getrandom` for fill — do NOT pull `uuid` v1 just for `v7`: §5 only
says envelope `id` MUST be a ULID *or* UUIDv7, and we already
generate ULIDs; adding a second ID system to satisfy "or" buys nothing.
`ulid` 1 shipped in the last 12 months.

| ID newtype        | Generator       | Wire form        |
| ----------------- | --------------- | ---------------- |
| `MessageId`       | `ulid` 1        | Crockford base32 |
| `SessionId`       | `ulid` 1        | Crockford base32 |
| `JobId`           | `ulid` 1        | Crockford base32 |
| `IdempotencyKey`  | `ulid` 1        | Crockford base32 |
| `EventSeq`        | `u64` counter   | JSON number      |
| `TraceId`         | `getrandom`     | 32-hex (W3C §11) |
| `SpanId`          | `getrandom`     | 16-hex (W3C §11) |

## Testing

`#[tokio::test]` from `tokio` (`Cargo.toml:72`, `test-util` feature for
`tokio::time::pause`). `insta` 1 for envelope snapshots
(`Cargo.toml:77`) — there are ~18 v1.1 envelopes (§6, §7, §8) and each
one becomes a `.snap` so wire drift fails CI. `tracing-test` 0.2
(`Cargo.toml:74`) for asserting span/event emission during the §7.4
cancellation race and §6.4 heartbeat-loss path. **Pick `proptest` over
`quickcheck`**: `proptest` ships shrinking that produces minimal
failing cases (necessary when the input is a randomised envelope tree
of unknown depth — §5.1 forward-compat ignores unknowns, and we need to
prove the deserializer survives arbitrary unknown extras), and
`quickcheck` shrinks numerically only. `proptest` shipped in the last
12 months; `quickcheck` 1 has not shipped in >18 months — disqualifies.
**Ship `criterion` benchmarks for envelope encode/decode and the
event-log replay path only.** Not for the WebSocket round-trip — that
benchmark would measure `tokio-tungstenite` and OS-level loopback, not
us. `criterion` shipped in the last 12 months.

## Coverage

`cargo-llvm-cov` (referenced in `rust-sdk/README.md:54`). Confirm. Set
a floor of **87 %** line coverage in CI matching Phase 7's prompt,
enforced by `cargo llvm-cov --fail-under-lines 87`. Reject `tarpaulin`
— `cargo-llvm-cov` uses the rustc-built-in instrumentation
(`-Cinstrument-coverage`) which is the same tooling `rust-lang/rust`
uses for its own coverage; `tarpaulin` ptraces and miscounts async
branches under `tokio`. `cargo-llvm-cov` shipped in the last 12 months.

## Lint & format

Lint posture in `Cargo.toml:124-140` is correct as written. **Tighten
one knob**: promote `clippy::expect_used` from `warn` to `deny`
(`Cargo.toml:133`) once the rewrite lands — every `.expect()` in the
v1.1 rewrite is either provably-infallible (justified by `#[allow]`
with a SAFETY-shaped comment) or a bug, and warnings on a clean tree
become noise. Leave `module_name_repetitions` and
`multiple_crate_versions` allowed — `multiple_crate_versions` will
trip on the OTel/`hashbrown`/`socket2` minor-skew that the ecosystem
will not solve before v1.1 ships. For `rustfmt.toml`: leave
`max_width = 100`, `use_field_init_shorthand = true` as-is. Reject
adding `imports_granularity = "Module"` and `group_imports =
"StdExternalCrate"` — both are nightly-only (already noted in
`rustfmt.toml:5-7`) and the toolchain pin is stable
(`rust-toolchain.toml:2`).

## Mutation testing

**No.** Reject `cargo-mutants` for v1.1 CI. The crate is healthy
(shipped in the last 12 months) and the discipline is good in
principle, but a single full mutation run on a SDK this size is a
20–60-minute wall-clock cost on GH Actions free-tier runners
(`x86_64-unknown-linux-gnu` Standard, 4 vCPU). v1.1 already commits to
`proptest` (envelope coverage) and `insta` (wire-format coverage) and
the §12 taxonomy is checked by exhaustive match (the `#[non_exhaustive]`
+ rust-edition 2021 wildcard rules in `src/error.rs` audit, line 60).
Revisit post-1.1 — gated on a self-hosted runner.

## Build / MSRV

Set **`rust-version = "1.82"`** in `Cargo.toml:5` (currently `1.88`).
Defence: 1.82 stabilises `&raw const` and the precise capturing syntax
needed for `impl Stream + Send + 'a` in the `subscribe()` return type
(`02-current-audit.md` line 119 H-risk note), and 1.75 (`impl Trait in
trait`) is the minimum that lets the `Transport` trait drop
`#[async_trait]` (`Cargo.toml:63`) — but 1.75 lacks `async fn` in
public traits' object-safety story, which 1.79 unlocks via
`async_fn_in_trait`'s warn-by-default. 1.82 covers all of it with
margin and is ~14 months old (well past distro-pinning lag for
Debian-stable, Amazon Linux 2023, Alpine edge). 1.88 (April 2025
release) is aggressive: it forces every downstream consumer onto a
toolchain younger than the OTel 0.27 release window, with no language
feature we actually depend on. Stable channel only
(`rust-toolchain.toml:2`); no nightly.

## Workspace structure

**Recommendation: workspace, 5 member crates.** One-line reason: the
TS reference splits the same way
(`typescript-sdk/packages/{core,client,runtime,middleware,sdk}`) and
the §11 OTel surface plus the §6.1 auth pluggability both want crate
boundaries so that `arcp-core` stays exporter-free and
authenticator-free per the "no `reqwest` in core" rule above.

| Crate            | TS analogue                           | Contents                                                                                       |
| ---------------- | ------------------------------------- | ---------------------------------------------------------------------------------------------- |
| `arcp-core`     | `typescript-sdk/packages/core`        | Envelope, ID newtypes, error taxonomy (§12), message types (§6/§7/§8), `Transport` trait, store. |
| `arcp-client`   | `typescript-sdk/packages/client`      | `Session<S>` type-state, `JobHandle`, `subscribe()`/`listJobs()`/`ack` (§6.5, §6.6, §7.6).      |
| `arcp-runtime`  | `typescript-sdk/packages/runtime`     | Handshake (§6.2), heartbeats (§6.4), lease + watchdog (§9.5), budget (§9.6), job FSM (§7.3).   |
| `arcp-otel`     | `typescript-sdk/packages/middleware`  | `tracing-opentelemetry` + §11 attribute glue. Exact-minor pinned.                              |
| `arcp` (facade) | `typescript-sdk/packages/sdk`         | Re-exports + CLI bin (`src/bin/arcp.rs`).                                                      |

## Cargo.toml fragment to drop in

```toml
# Workspace root /Users/nficano/code/arpc/rust-sdk/Cargo.toml
[workspace]
resolver = "2"
members = ["crates/arcp-core", "crates/arcp-client", "crates/arcp-runtime", "crates/arcp-otel", "crates/arcp"]

[workspace.package]
rust-version = "1.82"
edition       = "2021"
license       = "MIT OR Apache-2.0"

[workspace.dependencies]
tokio              = { version = "1", features = ["rt-multi-thread", "macros", "net", "sync", "time", "io-util", "io-std", "fs", "process", "signal"] }
tokio-util         = { version = "0.7", features = ["rt"] }
tokio-stream       = "0.1"
futures            = "0.3"
serde              = { version = "1", features = ["derive"] }
serde_json         = "1"
thiserror          = "2"
tracing            = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
ulid               = { version = "1", features = ["serde"] }
chrono             = { version = "0.4", default-features = false, features = ["serde", "clock"] }
dashmap            = "6"
clap               = { version = "4", features = ["derive"] }
base64             = "0.22"
getrandom          = "0.2"
rusqlite           = { version = "0.32", features = ["bundled"] }
tokio-tungstenite  = "0.24"
opentelemetry      = "=0.27"
opentelemetry_sdk  = "=0.27"
tracing-opentelemetry = "=0.28"
# dev
insta              = { version = "1", features = ["json"] }
tokio-test         = "0.4"
tracing-test       = "0.2"
pretty_assertions  = "1"
tempfile           = "3"
proptest           = "1"
criterion          = "0.5"

[workspace.lints.rust]
unsafe_code      = "deny"
missing_docs     = "deny"
unreachable_pub  = "warn"

[workspace.lints.clippy]
pedantic         = { level = "warn", priority = -1 }
nursery          = { level = "warn", priority = -1 }
unwrap_used      = "deny"
expect_used      = "deny"   # tightened from warn
panic            = "deny"
todo             = "deny"
unimplemented    = "deny"
module_name_repetitions = "allow"
multiple_crate_versions = "allow"
```

Crates dropped from the v1.0 manifest: `async-trait` (replaced by
1.75+ `async fn` in trait via the 1.82 MSRV), `jsonwebtoken` (moved to
deployer-side per the "Errors / HTTP" rationale — `signed_jwt` is
dropped from the SDK per `02-current-audit.md` line 75).

Crates added: `getrandom` (W3C `TraceId`/`SpanId` fill),
`opentelemetry`, `opentelemetry_sdk`, `tracing-opentelemetry`,
`proptest`, `criterion`.
