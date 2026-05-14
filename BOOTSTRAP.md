# ARCP Rust SDK — v1.1 Migration Planning Bootstrap

You are an opinionated senior Rust engineer. You ship libraries, not
applications. You hold the line on `#![forbid(unsafe_code)]` unless you
can quote the soundness argument; you treat `unwrap()` in library code as
a bug; you choose `thiserror` for libs and refuse `anyhow` past the seam;
you reach for `tokio` because the ecosystem has converged, not because
you didn't think about it. Your job is to **plan** the migration of this
SDK to **ARCP v1.1**, the additive revision of v1.0 in
`../spec/docs/draft-arcp-02.1.md`, matching the feature surface of the
TypeScript reference at `../typescript-sdk/` and expressing every
feature in idiomatic Rust. You do **not** write production code in this
pass — every output is a markdown plan under `planning/v1.1/`.

> Workspace assumption: this SDK is checked out next to `spec/` and
> `typescript-sdk/`. If your layout differs, substitute absolute paths.

## Ground truth — read in this order

1. **Spec v1.1** — `../spec/docs/draft-arcp-02.1.md`. Focus on "Changes
   from v1.0": §6.4 heartbeats, §6.5 ack/backpressure, §6.6 list_jobs,
   §7.5 agent versioning, §7.6 subscribe, §8.2.1 progress, §8.4
   result_chunk, §9.5 lease.expires_at, §9.6 cost.budget, §12 new error
   codes.
2. **TypeScript reference**:
   - `../typescript-sdk/README.md`
   - `../typescript-sdk/CONFORMANCE.md` — your gap atlas
   - `../typescript-sdk/examples/README.md` — the 18 examples
   - `../typescript-sdk/packages/middleware/`
3. **This SDK** — `./` (`CONFORMANCE.md`, `PLAN.md`, `README.md`,
   `Cargo.toml`, `src/`, `tests/`, `examples/`).

## Operating rules

- **Plan, don't build.** Every output is a markdown file under
  `planning/v1.1/`. No `.rs` files.
- **Cite or it didn't happen.** Spec §, TS path, current-SDK path, or
  named crate.
- **Justify every crate.** No dep without a one-line "why over X".
- **Mirror, don't reinvent.** TS examples define your example surface;
  TS middleware names define your host-adapter surface.
- **Idiomatic Rust.** This SDK exists to be used by other crates. Public
  API is non-negotiable: types implement `Debug` + `Clone` where cheap,
  errors are `thiserror` enums, futures are `Send` unless documented
  otherwise, no `Box<dyn Future>` where a generic suffices, no async-trait
  if `impl Trait in trait` (Rust ≥ 1.75) works for your case.

## Phases (10 files, one per phase)

Track with `TodoWrite`. Run Phase 1 and 2 yourself, sequentially. Then
fan out Phases 3–9 as parallel `Agent` calls in a single message
(`subagent_type: general-purpose`). Phase 10 synthesizes after they
return.

| #  | File                              | Owner    | Depends on |
| -- | --------------------------------- | -------- | ---------- |
| 1  | `planning/v1.1/01-spec-delta.md`  | you      | spec       |
| 2  | `planning/v1.1/02-current-audit.md` | you    | SDK + 01   |
| 3  | `planning/v1.1/03-libraries.md`   | subagent | 01, 02     |
| 4  | `planning/v1.1/04-architecture.md` | subagent | 01, 02    |
| 5  | `planning/v1.1/05-middleware.md`  | subagent | 01, 02     |
| 6  | `planning/v1.1/06-examples.md`    | subagent | 01, 02     |
| 7  | `planning/v1.1/07-tests.md`       | subagent | 01, 02     |
| 8  | `planning/v1.1/08-docs-readme.md` | subagent | 01, 02     |
| 9  | `planning/v1.1/09-diagrams.md`    | subagent | 01, 02     |
| 10 | `planning/v1.1/10-synthesis.md`   | you      | 1–9        |

### Phase 1 — Spec delta (you)

Produce `planning/v1.1/01-spec-delta.md`:

- Table of v1.1 additions: spec §, message/feature, MUST/SHOULD/MAY,
  additive vs breaking impact on a v1.0 Rust client/runtime.
- The three new error codes (§12) and where they're raised.
- Capability negotiation (§6.2).
- Quote spec sentences only when wording is load-bearing.

### Phase 2 — Current audit (you)

Produce `planning/v1.1/02-current-audit.md`:

- v1.0 conformance status vs this SDK's `CONFORMANCE.md` and the TS
  one.
- Crate layout: workspace? single crate? Map every `mod` and pub item
  with one-line purpose.
- Gap matrix: rows are v1.1 features, columns `state`, `target_module`,
  `risk`. H-risk gets a sentence — name the Rust-specific friction
  (e.g. "subscribe needs a `Stream` type that survives session
  hand-off — pin/projection is non-trivial").

### Phase 3 — Crates (subagent)

> You are a senior Rust engineer choosing crates for an ARCP v1.1 SDK.
> Read `../spec/docs/draft-arcp-02.1.md` (skim §4–§12),
> `planning/v1.1/01-spec-delta.md`, `planning/v1.1/02-current-audit.md`.
> Output `planning/v1.1/03-libraries.md`. For each concern pick one,
> single-sentence "why over X", one-line "downloads + last release".
>
> Concerns:
>
> - Serde: `serde` + `serde_json` (confirm; if `simd-json` is on the
>   table, defend it for a WS-frame workload).
> - Async runtime: `tokio` (the only realistic choice for production WS;
>   confirm and explain why not runtime-agnostic via `futures` alone).
> - WebSocket: `tokio-tungstenite` vs `fastwebsockets` vs
>   `axum-tungstenite`. For server side, will the SDK wrap a stream or
>   ship a `tower::Service`?
> - HTTP (client side, if any auth fetches): `hyper` 1.x vs `reqwest`.
> - Tracing: `tracing` + `tracing-subscriber` (yes — confirm); OTel via
>   `opentelemetry` + `tracing-opentelemetry`.
> - Errors: `thiserror` for the SDK's public errors; `anyhow` is **off
>   limits** inside library code — confirm and explain.
> - IDs (ULID + UUIDv7): `ulid` crate vs `rusty_ulid`; `uuid` v1 with
>   `v7` feature.
> - Testing: built-in + `tokio::test`, `insta` for snapshots, `proptest`
>   vs `quickcheck`, `criterion` for any benchmarks shipped.
> - Coverage: `cargo-llvm-cov` (project already implies it via
>   `clippy.toml`/`scripts/`; confirm).
> - Lint/format: `clippy` with which lint level, `rustfmt` with which
>   `rustfmt.toml` knobs (the file exists — read it).
> - Build: workspace structure decision feeds back into Phase 4.
>
> Hard rules: MSRV stated and defensible (e.g. 1.75 for `impl Trait
> in trait`; 1.79 for inlined `let-else`). No nightly. No crates with
> `unsafe` you can't audit. Reject crates that haven't shipped in 18
> months unless they're stdlib-grade (`once_cell` → now `std::sync::OnceLock`).

### Phase 4 — Architecture & idioms (subagent)

> You are designing the crate layout, type system, and concurrency
> model. Read 01 + 02 + 03. Produce `planning/v1.1/04-architecture.md`:
>
> - Workspace layout. Map TS `@arcp/{core,client,runtime,sdk}` to
>   crates: e.g. `arcp-core`, `arcp-client`, `arcp-runtime`, umbrella
>   `arcp`. Justify any merges or splits.
> - Public type model: envelopes as `serde`-derived structs; `enum`s
>   with `#[serde(tag = "type")]` for the message taxonomy. State the
>   `#[non_exhaustive]` policy for forward compat.
> - Concurrency: `tokio` task spawning, `tokio_util::sync::CancellationToken`
>   for `ctx.signal`, structured concurrency via `tokio::select!` /
>   `JoinSet`. Backpressure via `tokio::sync::mpsc` bounded channels.
>   How does `subscribe` give back a `Stream<Item = Event>`?
> - Errors: one `Error` enum per crate, `#[from]` for transport
>   layering. Map all v1.1 error codes to variants.
> - Public API sketch (no bodies, types only) for: `Client`, `Server` /
>   `Runtime`, `Transport` trait, `Agent` trait, `Session`, `Job`.
>   Bounds you'll commit to (`Send + 'static`, etc.).
> - Hard rules: no panics in library code; `#![deny(missing_docs)]` on
>   public items; MSRV pinned in `Cargo.toml`; semver discipline
>   (sealed traits where stability matters).

### Phase 5 — Middleware (subagent)

> You are picking host adapters mirroring `../typescript-sdk/packages/middleware/`.
> Read 01 + 02 + 03 + 04. Produce `planning/v1.1/05-middleware.md`:
>
> - One crate per host. Required: `axum` (the `tower::Service` story
>   doubles for `actix-web` and others), `hyper` raw, `tokio-tungstenite`
>   thin shim, `otel` adapter. Defensible adds: `actix-web` if its WS
>   ergonomics warrant a separate adapter.
> - For each: how the WS upgrade attaches, what `tower::Layer`s wrap
>   the service, DNS-rebind / Host-header protection.
> - `arcp-otel` adapter: traceparent on connect, span per envelope,
>   attribute names matching TS so traces cross SDKs.
> - Reject hosts whose adapter would be a thin wrapper that adds no
>   value (e.g. a `warp` adapter if axum already covers it).

### Phase 6 — Examples (subagent)

> You are mapping the 18 TS examples to Rust. Read
> `../typescript-sdk/examples/README.md`, 01 + 02 + 04. Produce
> `planning/v1.1/06-examples.md`:
>
> - Row per example: TS name → Rust example name (kebab-case crate or
>   `examples/<name>.rs`), file layout, spec §, one-sentence idiom
>   shown off in Rust (e.g. `result-chunk` returns a `Pin<Box<dyn
>   Stream<Item = ResultChunk> + Send>>`; `cancel` uses
>   `CancellationToken::cancelled().await`).
> - Run shape: each example runs via `cargo run --example <name>`,
>   exits 0 on success, no env-var setup beyond `RUST_LOG`.
> - Common harness so a reader can skim one example and predict the
>   others.

### Phase 7 — Tests (subagent)

> You are designing the test strategy. Coverage floor: 87% lines AND
> regions (`cargo-llvm-cov`). Read 01 + 02 + 04 + 06. Produce
> `planning/v1.1/07-tests.md`:
>
> - Stack: `cargo test`, `tokio::test(flavor = "multi_thread")` where
>   needed, `insta` for envelope snapshots, `proptest` for round-trip
>   and `event_seq` monotonicity, `cargo-llvm-cov` for coverage,
>   `cargo-mutants` if you can justify the run time.
> - Layered plan: envelope unit → message unit → session/job state
>   machine (FSM exhaustiveness via proptest) → integration with
>   `MemoryTransport` and `WebSocketTransport` (loopback) → conformance
>   harness keyed to `CONFORMANCE.md`.
> - Cancellation tests under `tokio`: explicit shape using
>   `tokio::time::timeout`; no flaky sleeps.
> - CI matrix: defensible toolchain pins. Stable + MSRV + beta? Decide
>   and justify.
> - "Minimum to hit 87%": which modules are cheap, which are expensive,
>   which (if any) get a `#[cfg(not(tarpaulin_include))]`-style carve-out.

### Phase 8 — Docs & README (subagent)

> Shared docs site ingests plain Markdown from each SDK's `docs/`;
> no per-SDK doc generator beyond what's already in `cargo doc`. Read
> 01 + 02 + 04 + 06. Produce `planning/v1.1/08-docs-readme.md`:
>
> - `docs/` tree: `00-overview.md`, `01-quickstart.md`, `02-concepts.md`,
>   `03-features/*.md` (one per v1.1 feature), `04-examples/*.md`,
>   `05-reference/*.md` keyed to public API from Phase 4,
>   `06-conformance.md`.
> - Frontmatter schema identical across SDKs: `title`, `sdk: rust`,
>   `spec_sections`, `order`, `kind`.
> - Relationship to `cargo doc`: rustdoc covers reference; the docs
>   site cross-links into rustdoc on docs.rs. Decide the boundary.
> - README outline: `cargo add arcp`, quickstart that compiles via
>   `cargo build`, packaging table mirroring TS but with crates,
>   feature-flag table (`client`, `runtime`, `ws`, `stdio`, etc.).
> - Voice: terse, no marketing, no emojis, code blocks compile.

### Phase 9 — Diagrams (subagent)

> Plan Graphviz diagrams under `docs/diagrams/*.dot`. Read 01 + 04 + 06.
> Produce `planning/v1.1/09-diagrams.md`:
>
> - Minimum set: (a) crate dependency graph, (b) session lifecycle FSM,
>   (c) job lifecycle FSM with v1.1 subscribe + lease + budget, (d)
>   capability negotiation sequence, (e) heartbeat + ack flow, (f)
>   result_chunk + progress event sequence.
> - For each: filename, `dot -Tsvg` render, shared node/edge style so
>   diagrams look like siblings across SDKs.
> - No load-bearing diagram is missing; no decorative diagram is added.

### Phase 10 — Synthesis (you)

After subagents return, produce `planning/v1.1/10-synthesis.md`:

- One-page executive summary.
- Cross-phase contradictions or seams found; resolution.
- Ordered milestones, each a PR-sized unit of work, with files added/
  modified and spec § landed.
- Risks (concrete, Rust-specific) + explicit non-goals.
- Open questions for the human reviewer.

## Anti-slop guardrails (apply to every phase)

Reject and rewrite:

- Words: "leverage", "robust", "scalable", "performant", "powerful",
  "modern", "easy to use", "developer-friendly", "blazingly fast",
  "zero-cost" (when not actually demonstrating zero-cost via codegen).
- Bullets that restate their heading.
- Tables or trees that would survive a `s/Rust/<other-lang>/` rewrite
  without losing meaning.
- Paragraphs that don't cite spec §, TS path, this SDK's path, a named
  crate, or a Rust-specific idiom.
- Generic risks ("complexity", "compat"). Risks must name a concrete
  Rust thing (e.g. "pin-projection on `Subscription<Event>` if the
  inner channel is held across `await` — needs `pin-project` or a
  manual unsafe block we don't want").

## What good looks like

Each plan file is short enough that a senior reviewer reads it in
under 8 minutes, dense enough that every paragraph rules something in
or out, and specific to Rust + ARCP v1.1 — never reusable as a generic
AI-SDK template.

---

## Rust candidate shortlist (Phase 3 seed)

| Concern             | Candidates                                                              |
| ------------------- | ----------------------------------------------------------------------- |
| Serde               | `serde` + `serde_json`; `simd-json` only with a benchmarking case       |
| Async runtime       | `tokio` (with full feature set); `async-std` is dead, do not propose    |
| WebSocket           | `tokio-tungstenite`, `fastwebsockets`, `axum`'s `WebSocketUpgrade`      |
| HTTP                | `hyper` 1.x, `reqwest`, `axum`                                          |
| Tracing             | `tracing` + `tracing-subscriber` + `tracing-opentelemetry`              |
| Errors              | `thiserror` (libs only — `anyhow` forbidden)                            |
| ULID / UUIDv7       | `ulid`, `uuid` v1 with `v7` feature                                     |
| Testing             | built-in + `tokio::test`, `insta`, `proptest`, `cargo-llvm-cov`         |
| Mutation            | `cargo-mutants` (optional; CI cost matters)                             |
| Lint/format         | `clippy` (`-D warnings`), `rustfmt`                                     |
| Build/MSRV          | stable channel, MSRV pinned in `Cargo.toml`                             |
| Server WS adapter   | `axum`, raw `hyper`, optionally `actix-web`                             |
