# Phase 8 — `docs/` site source + `README.md` plan (v1.1)

Plan only. No `.rs` or `.md` files are written here besides this file.
Voice rules (terse, no marketing, no emojis, no bullets that restate
their heading, every paragraph cites a spec §, a TS path, a current-SDK
path, or a named crate) are enforced top-to-bottom; the anti-slop word
list is treated as hard-rejected on review.

Phases 04 (architecture) and 06 (examples) were not on disk at write
time. The example slate below is reconstructed from
`../../typescript-sdk/examples/` and `../typescript-sdk/CONFORMANCE.md`
§13; the architectural crate split is taken from `03-libraries.md`
(workspace table, lines 205–211).

## 1. `docs/` tree

Layout, one file per row, one-line purpose, target spec § citation,
the TS source whose voice it mirrors, and the citation seam into this
SDK's Rust code (the "x" entries land in Phase 4/5 modules per the
audit map in `02-current-audit.md` lines 102–134).

| Path                                          | Purpose (one line)                                                                                | Spec §        | TS source                                              | Rust seam (Phase 4/5)                      |
| --------------------------------------------- | ------------------------------------------------------------------------------------------------- | ------------- | ------------------------------------------------------ | ------------------------------------------ |
| `docs/00-overview.md`                         | What ARCP is, what the Rust crate set is, what's out of scope (HITL, tool registry, artifacts).   | §1–§3         | `typescript-sdk/README.md:1-23`                        | `arcp-core::lib`                           |
| `docs/01-quickstart.md`                       | Connect → submit → await result; identical narrative to the README quickstart, longer prose.      | §6.2, §7.1    | `typescript-sdk/docs/getting-started.md`               | `arcp-client::Session`                     |
| `docs/02-concepts.md`                         | Envelope (§5), session (§6), job (§7), event (§8), lease (§9), trace (§11), error (§12).          | §5–§12        | `typescript-sdk/README.md:111-237` (Core concepts)     | `arcp-core::{envelope,messages,error}`     |
| `docs/03-features/heartbeat.md`               | `session.ping`/`session.pong`; two-interval close; not counted in `event_seq`.                    | §6.4          | `typescript-sdk/CONFORMANCE.md:229-238`                | `arcp-runtime::session::heartbeat`         |
| `docs/03-features/ack.md`                     | `session.ack { last_processed_seq }`; back_pressure status; `autoAck` coalescing.                 | §6.5          | `typescript-sdk/CONFORMANCE.md:240-250`                | `arcp-runtime::session::ack` + client      |
| `docs/03-features/list-jobs.md`               | `session.list_jobs` / `session.jobs`; filter, cursor, same-principal auth.                        | §6.6          | `typescript-sdk/CONFORMANCE.md:252-262`                | `arcp-runtime::server::list_jobs`          |
| `docs/03-features/subscribe.md`               | `job.subscribe` / `job.subscribed` / `job.unsubscribe`; history replay; no cancel authority.      | §7.6          | `typescript-sdk/CONFORMANCE.md:278-289`                | `arcp-runtime::subscription`               |
| `docs/03-features/agent-versions.md`          | `name@version` grammar; rich `agents` inventory; `AGENT_VERSION_NOT_AVAILABLE`.                   | §7.5          | `typescript-sdk/CONFORMANCE.md:264-275`                | `arcp-core::messages::execution::AgentRef` |
| `docs/03-features/lease-expires-at.md`        | `lease_constraints.expires_at`; watchdog; `LEASE_EXPIRED`; no renewal.                            | §9.5          | `typescript-sdk/CONFORMANCE.md:321-331`                | `arcp-runtime::lease::watchdog`            |
| `docs/03-features/cost-budget.md`             | `cost.budget` patterns; counter decrement on `cost.*` metric; `BUDGET_EXHAUSTED`.                 | §9.6          | `typescript-sdk/CONFORMANCE.md:333-346`                | `arcp-runtime::lease::budget`              |
| `docs/03-features/progress.md`                | `progress` event kind; `{ current, total?, units?, message? }`.                                   | §8.2.1        | `typescript-sdk/CONFORMANCE.md:291-298`                | `arcp-core::messages::execution::Progress` |
| `docs/03-features/result-chunk.md`            | `result_chunk` kind; monotone `chunk_seq`; `JobHandle::collect_chunks`.                           | §8.4          | `typescript-sdk/CONFORMANCE.md:300-311`                | `arcp-client::JobHandle::collect_chunks`   |
| `docs/04-examples/submit-and-stream.md`       | Narrative pointer to `examples/submit-and-stream/`; the canonical v1.0 happy-path.                | §13.1 / §8.2  | `typescript-sdk/examples/submit-and-stream/`           | `examples/submit-and-stream/`              |
| `docs/04-examples/cancel.md`                  | `job.cancel` + 30 s grace; `final_status: "cancelled"`.                                           | §7.4          | `typescript-sdk/examples/cancel/`                      | `examples/cancel/`                         |
| `docs/04-examples/delegate.md`                | `delegate` event kind; subset enforcement; parent `tool_result` on violation.                     | §10           | `typescript-sdk/examples/delegate/`                    | `examples/delegate/`                       |
| `docs/04-examples/resume.md`                  | `resume_token` rotation; replay by `last_event_seq`; `RESUME_WINDOW_EXPIRED`.                     | §6.3          | `typescript-sdk/examples/resume/`                      | `examples/resume/`                         |
| `docs/04-examples/idempotent-retry.md`        | `idempotency_key`; `DUPLICATE_KEY` on agent/input mismatch.                                       | §7.2          | `typescript-sdk/examples/idempotent-retry/`            | `examples/idempotent-retry/`               |
| `docs/04-examples/lease-violation.md`         | Glob mismatch → `PERMISSION_DENIED` as `tool_result` body.                                        | §9.3          | `typescript-sdk/examples/lease-violation/`             | `examples/lease-violation/`                |
| `docs/04-examples/stdio.md`                   | NDJSON over `AsyncRead`/`AsyncWrite`; child-process shape.                                        | §4.2          | `typescript-sdk/examples/stdio/`                       | `examples/stdio/`                          |
| `docs/04-examples/vendor-extensions.md`       | `x-vendor.*` envelope type, event kind, lease namespace.                                          | §15           | `typescript-sdk/examples/vendor-extensions/`           | `examples/vendor-extensions/`              |
| `docs/04-examples/custom-auth.md`             | Custom `Authenticator` impl; bearer is one of many possible schemes.                              | §6.1          | `typescript-sdk/examples/custom-auth/`                 | `examples/custom-auth/`                    |
| `docs/04-examples/heartbeat.md`               | Two-interval `HEARTBEAT_LOST`; session survives, jobs survive.                                    | §6.4          | `typescript-sdk/examples/heartbeat/`                   | `examples/heartbeat/`                      |
| `docs/04-examples/ack-backpressure.md`        | Client `autoAck`; runtime `back_pressure` `status` event.                                         | §6.5          | `typescript-sdk/examples/ack-backpressure/`            | `examples/ack-backpressure/`               |
| `docs/04-examples/list-jobs.md`               | Filter by status + agent; cursor pagination.                                                      | §6.6          | `typescript-sdk/examples/list-jobs/`                   | `examples/list-jobs/`                      |
| `docs/04-examples/subscribe.md`               | Dashboard-style subscriber; `history: true` replay.                                               | §7.6          | `typescript-sdk/examples/subscribe/`                   | `examples/subscribe/`                      |
| `docs/04-examples/agent-versions.md`          | `name@version` pinning; `AGENT_VERSION_NOT_AVAILABLE`.                                            | §7.5          | `typescript-sdk/examples/agent-versions/`              | `examples/agent-versions/`                 |
| `docs/04-examples/lease-expires-at.md`        | Watchdog fires `LEASE_EXPIRED` mid-run.                                                           | §9.5          | `typescript-sdk/examples/lease-expires-at/`            | `examples/lease-expires-at/`               |
| `docs/04-examples/cost-budget.md`             | Per-currency counters; `BUDGET_EXHAUSTED` as `tool_result`.                                       | §9.6          | `typescript-sdk/examples/cost-budget/`                 | `examples/cost-budget/`                    |
| `docs/04-examples/progress.md`                | Indeterminate vs determinate `progress` body.                                                     | §8.2.1        | `typescript-sdk/examples/progress/`                    | `examples/progress/`                       |
| `docs/04-examples/result-chunk.md`            | Chunk emitter + `JobHandle::collect_chunks` consumer.                                             | §8.4          | `typescript-sdk/examples/result-chunk/`                | `examples/result-chunk/`                   |
| `docs/04-examples/tracing.md`                 | `arcp-otel` middleware wiring; `arcp.lease.expires_at` / `arcp.budget.remaining` attributes.      | §11           | `typescript-sdk/examples/tracing/`                     | `examples/tracing/`                        |
| `docs/05-reference/arcp-core.md`              | Public surface: envelope, IDs, errors, message types, transports.                                 | §5–§8, §12    | `typescript-sdk/packages/core/`                        | `crates/arcp-core/`                        |
| `docs/05-reference/arcp-client.md`            | `Session<S>`, `JobHandle`, `subscribe`, `list_jobs`, `ack`.                                       | §6.5–§7.6     | `typescript-sdk/packages/client/`                      | `crates/arcp-client/`                      |
| `docs/05-reference/arcp-runtime.md`           | Server, agent registration, lease/budget enforcement, FSM.                                        | §6–§9         | `typescript-sdk/packages/runtime/`                     | `crates/arcp-runtime/`                     |
| `docs/05-reference/arcp-otel.md`              | OTel span/attribute glue (v1.1 attrs §11). Exact-minor pinned per `03-libraries.md:80-84`.        | §11           | `typescript-sdk/packages/middleware/otel/`             | `crates/arcp-otel/`                        |
| `docs/05-reference/arcp.md`                   | Umbrella facade re-exports + `arcp` CLI bin.                                                      | —             | `typescript-sdk/packages/sdk/`                         | `crates/arcp/`                             |
| `docs/06-conformance.md`                      | Section-by-section status with `file:line` cites; mirrors TS `CONFORMANCE.md`.                    | §4–§15        | `typescript-sdk/CONFORMANCE.md`                        | `crates/*/src/**` cites                    |
| `docs/99-migration-from-rfc-0001.md`          | Wholesale wire-surface delta from the current crate (`02-current-audit.md`); not additive.        | n/a           | —                                                      | `02-current-audit.md` gap-matrix           |

Notes:

- One file per v1.1 feature in `03-features/` (nine files) matches the
  §6.4–§9.6 set in `01-spec-delta.md` and the
  `typescript-sdk/CONFORMANCE.md` feature-negotiation matrix
  (lines 196–207). No file collapses two features — the ingest pipeline
  sorts by `order` per file.
- `04-examples/` carries one page per `examples/<name>/`; the v1.0 nine
  plus the v1.1 nine plus `tracing/` total 19. Host-integration
  examples (`express/`, `fastify/`, `bun/`) from
  `typescript-sdk/examples/` are intentionally omitted — there is no
  Rust analogue of Express, and Phase 5 host middleware decisions
  (Hyper / Axum / Tower glue) are out of scope for v1.1 docs. They
  return as `04-examples/host-*.md` in v1.2 when those middleware
  crates land.
- `05-reference/` is one page per public crate, not one page per
  module; rustdoc covers module-level reference (see §3 below for the
  `cargo doc` boundary).
- `06-conformance.md` is a *copy* of `CONFORMANCE.md` in the docs site
  (not a redirect) because the ingest pipeline expects markdown, not
  symlinks; the two files diverge only on header anchors. The Rust
  version cites `file:line` into `crates/*/src/**` exactly as
  `typescript-sdk/CONFORMANCE.md` cites `packages/*/src/**`.

## 2. Frontmatter schema

Identical across SDKs so the shared ingest pipeline does not fork.
Every doc file under `docs/` MUST open with a YAML frontmatter block.
Required fields: `title`, `sdk`, `spec_sections`, `order`, `kind`.
Optional fields: `feature` (the v1.1 feature flag this page documents,
required iff `kind == "feature"`), `example` (the directory name under
`examples/`, required iff `kind == "example"`), `crate` (the Cargo
crate this page documents, required iff `kind == "reference"`).

```yaml
---
title: "Heartbeats"
sdk: rust
spec_sections: ["6.4"]
order: 30
kind: feature
feature: heartbeat
---
```

Field rules:

- `title`: human title, no SDK prefix (the ingest pipeline adds it).
- `sdk`: literal `rust`. Sister SDKs use `typescript`, `python`, etc.
- `spec_sections`: array of dotted §-numbers (strings, not numbers —
  `"8.2.1"` would lose precision as a JSON number).
- `order`: integer, multiples of **10** (10, 20, 30 …). The 10-step
  cadence lets later inserts land at 15 / 25 without renumbering. Two
  files in the same directory MUST NOT share an `order`; the ingest
  pipeline sorts ascending and treats ties as undefined order.
- `kind`: one of `overview` | `quickstart` | `concept` | `feature` |
  `example` | `reference` | `conformance`. Closed set; the pipeline
  rejects unknown values (mirrors the §15 "unknown types ignored" rule
  but in reverse — frontmatter is producer-controlled).

Order convention by file:

| File path                       | `kind`       | `order` |
| ------------------------------- | ------------ | ------- |
| `00-overview.md`                | `overview`   | 10      |
| `01-quickstart.md`              | `quickstart` | 20      |
| `02-concepts.md`                | `concept`    | 30      |
| `03-features/heartbeat.md`      | `feature`    | 10      |
| `03-features/ack.md`            | `feature`    | 20      |
| `03-features/list-jobs.md`      | `feature`    | 30      |
| `03-features/subscribe.md`      | `feature`    | 40      |
| `03-features/agent-versions.md` | `feature`    | 50      |
| `03-features/lease-expires-at.md` | `feature`  | 60      |
| `03-features/cost-budget.md`    | `feature`    | 70      |
| `03-features/progress.md`       | `feature`    | 80      |
| `03-features/result-chunk.md`   | `feature`    | 90      |
| `04-examples/*`                 | `example`    | 10–190 (per example in `examples/`) |
| `05-reference/*`                | `reference`  | 10–50   |
| `06-conformance.md`             | `conformance`| 10      |
| `99-migration-from-rfc-0001.md` | `concept`    | 990     |

## 3. `cargo doc` vs the docs site

**`cargo doc` is the reference; the docs site is the narrative.** Both
ship; neither subsumes the other.

The reference seam:

- `#![deny(missing_docs)]` lives at the crate root of every `arcp-*`
  crate (already set in `Cargo.toml:124-140`, kept verbatim per
  `03-libraries.md:151-152`). Every `pub` item carries a rustdoc
  comment; CI fails if not.
- Doctests run via `cargo test --doc` and gate releases. They run on
  the umbrella `arcp` crate only (see "doctest-bearing" rule below).
- `docs.rs` is the canonical reference URL. The docs site links into
  it as the *symbol-level* documentation; the docs site does not
  re-document types or methods.
- Cross-link direction at the file level: a rustdoc module-level doc
  comment for, say, `arcp_client::subscribe` opens with a one-line
  pointer to the docs-site narrative — `/// See the [Subscribe
  feature page](https://arcp.dev/docs/rust/features/subscribe) for the
  spec walkthrough.` No `[!doc]` callout box, no Mermaid, no
  marketing prose. The narrative page reciprocates with a "Reference:
  [`arcp_client::Client::subscribe`](https://docs.rs/arcp-client/latest/arcp_client/struct.Client.html#method.subscribe)".

The narrative seam:

- The docs site is concept-first: §6.2 capability negotiation is one
  page (`docs/02-concepts.md`) and the nine v1.1 features each get
  their own page under `docs/03-features/`. The narrative explains
  *why*; the rustdoc explains *what*.
- Spec § citations live on every feature/example page (mirror
  `typescript-sdk/CONFORMANCE.md:229-310`).
- Code in the docs site is copied (not embedded) from `examples/`.
  The README quickstart and `docs/01-quickstart.md` both excerpt
  `examples/submit-and-stream/client.rs`; CI (`cargo check --examples`,
  `03-libraries.md:30-36`) covers compilation. A `scripts/sync-docs.sh`
  diff-check fails CI when the excerpt drifts from the source.

Doctest distribution:

| Crate          | Public-surface rustdoc | Doctest-bearing | Why                                                                                                 |
| -------------- | ---------------------- | --------------- | --------------------------------------------------------------------------------------------------- |
| `arcp` (umbrella) | Yes                 | **Yes**         | Public-facing facade; doctests prove the README quickstart and the top of `01-quickstart.md` compile against the published surface. |
| `arcp-core`    | Yes                    | No              | Type-level documentation only. A doctest in `Envelope` would duplicate the `arcp` umbrella's doctest and double-link the example page. |
| `arcp-client`  | Yes                    | No              | Same logic. The narrative example for `Client::subscribe` lives in `arcp` umbrella doctests + `examples/subscribe/`. |
| `arcp-runtime` | Yes                    | No              | Same. Agent-author examples live in `examples/*/server.rs`.                                          |
| `arcp-otel`    | Yes                    | No              | OTel pin (`opentelemetry = "=0.27"`, `03-libraries.md:81-83`) makes doctests fragile across patch bumps — gate behind the umbrella. |

Rationale: doctests in every crate would force the example to compile
N times and the docs site to link N rustdoc URLs per feature. Keeping
doctests at the umbrella mirrors the TS pattern where
`typescript-sdk/packages/sdk` is the only package whose `README.md`
exercises the public-facing example end-to-end, and the per-package
READMEs are reference stubs.

## 4. `README.md` outline

The current `/Users/nficano/code/arpc/rust-sdk/README.md` targets
`RFC-0001-v2.md` (v0.1) — wholesale drop per `02-current-audit.md`
lines 1–24. The v1.1 README starts over with the structure below.
Section headings, in order, all H2 except the title (H1).

| # | Heading                          | Length budget         | What goes in                                                                                                                                                                                          |
| - | -------------------------------- | --------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1 | `# arcp`                         | 1 paragraph           | One-paragraph what-is. Cite ARCP v1.1 (`../spec/docs/draft-arcp-02.1.md`), call out: bearer auth, immutable per-job lease, single event stream, optional v1.1 features. No marketing words.            |
| 2 | `## Install`                     | 2 lines + 1 code line | `cargo add arcp` then `cargo run --example submit-and-stream`. Both must work post-Phase-6 (cite `Cargo.toml` workspace from `03-libraries.md:215-220` and the `examples/submit-and-stream/` Phase 6 deliverable). |
| 3 | `## Quickstart`                  | 10-line code block    | The excerpt below; sourced from `examples/submit-and-stream/client.rs`. CI (`cargo check --examples`) covers it.                                                                                       |
| 4 | `## Packaging`                   | 1 table               | The Rust crate set, mirroring TS `@arcp/*` names. Schema below.                                                                                                                                       |
| 5 | `## Feature flags`               | 1 table               | Cargo features per `03-libraries.md:42-49` and the OTel split (`03-libraries.md:80-84`). Defaults listed.                                                                                              |
| 6 | `## Documentation`               | 1 table + 1 link      | The `docs/03-features/` row-set indexed by spec §; same shape as the TS `## Documentation` guide table (`typescript-sdk/README.md:24-37`).                                                            |
| 7 | `## Conformance`                 | 1 paragraph           | One-line link to `CONFORMANCE.md`. No status badges.                                                                                                                                                  |
| 8 | `## Migrating`                   | 2 short paragraphs    | "From the v0.1 / `RFC-0001-v2` crate" + "From the TypeScript SDK". Bullets and a 6-row API-equivalents table (schema below in §6).                                                                    |
| 9 | `## License`                     | 1 line                | `MIT OR Apache-2.0` (unchanged from `02-current-audit.md`). The `LICENSE-MIT` + `LICENSE-APACHE` files survive verbatim.                                                                              |

The single 10-line README quickstart sketch (this is the ONLY code
block in this plan beyond the frontmatter example above; sourced from
`examples/submit-and-stream/client.rs` to be written in Phase 6):

```rust
use arcp::{Client, ClientConfig, Submit, WebSocketTransport};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let transport = WebSocketTransport::connect("ws://127.0.0.1:7777/arcp").await?;
    let client = Client::connect(ClientConfig::bearer("tok-demo"), transport).await?;
    let handle = client.submit(Submit::agent("echo").input(serde_json::json!({"hi": 1}))).await?;
    let result = handle.done().await?;
    println!("{result:?}");
    client.close().await
}
```

Packaging table (mirrors `typescript-sdk/README.md:40-46`):

| Crate          | TS analogue                          | When to use                                                                                       |
| -------------- | ------------------------------------ | ------------------------------------------------------------------------------------------------- |
| `arcp`         | `@arcp/sdk`                          | "Give me everything." Re-exports core + client + runtime, ships the `arcp` CLI binary.            |
| `arcp-core`    | `@arcp/core`                         | Shared primitives only — envelopes, errors, messages, transport trait, ID newtypes.               |
| `arcp-client`  | `@arcp/client`                       | Build a client that talks to a runtime. Depends on `arcp-core`.                                   |
| `arcp-runtime` | `@arcp/runtime`                      | Build a runtime that hosts agents. Depends on `arcp-core`.                                        |
| `arcp-otel`    | `@arcp/middleware-otel`              | §11 OTel span/attribute glue. Exact-minor pinned on `opentelemetry` (see `03-libraries.md:80-84`).|

Feature-flag table:

| Feature   | Default  | Gates                                                                                                  | Spec ref     |
| --------- | -------- | ------------------------------------------------------------------------------------------------------ | ------------ |
| `client`  | on       | Re-exports from `arcp-client` (in the `arcp` umbrella only).                                            | §6, §7       |
| `runtime` | on       | Re-exports from `arcp-runtime` (in the `arcp` umbrella only).                                           | §6, §7       |
| `ws`      | on       | `WebSocketTransport` via `tokio-tungstenite` 0.24 (`03-libraries.md:38-54`).                            | §4.1         |
| `stdio`   | on       | `StdioTransport`, NDJSON over `AsyncRead`/`AsyncWrite` (`02-current-audit.md` line 81).                 | §4.2         |
| `otel`    | **off**  | `arcp-otel` re-export. Off-by-default because `opentelemetry` 0.27 pulls a transitive tree the bare SDK doesn't need (`03-libraries.md:70-84`). | §11          |

Documentation table (README §6) mirrors the TS layout
(`typescript-sdk/README.md:29-37`) using the v1.1 feature set from
`docs/03-features/`:

| Guide                                          | Spec   | Guide                                          | Spec   |
| ---------------------------------------------- | ------ | ---------------------------------------------- | ------ |
| [Heartbeats](./docs/03-features/heartbeat.md)  | §6.4   | [Agent versions](./docs/03-features/agent-versions.md) | §7.5 |
| [Ack & back-pressure](./docs/03-features/ack.md) | §6.5 | [Lease expires-at](./docs/03-features/lease-expires-at.md) | §9.5 |
| [List jobs](./docs/03-features/list-jobs.md)   | §6.6   | [Cost budget](./docs/03-features/cost-budget.md) | §9.6 |
| [Subscribe](./docs/03-features/subscribe.md)   | §7.6   | [Progress](./docs/03-features/progress.md)     | §8.2.1 |
| [Concepts](./docs/02-concepts.md)              | §5–§12 | [Result chunks](./docs/03-features/result-chunk.md) | §8.4 |

## 5. Voice rules

Restated here so reviewers can grep:

- Terse. Sentences over 30 words get split.
- Banned vocabulary (review reject): "leverage", "robust", "scalable",
  "performant", "powerful", "modern", "easy to use",
  "developer-friendly", "blazingly fast", "zero-cost". A regex-grep in
  CI (`scripts/anti-slop.sh`) fails the PR.
- No emojis anywhere — README, docs site, rustdoc, examples,
  CHANGELOG.
- Every code block compiles: README and `docs/01-quickstart.md`
  excerpts pass `cargo check --examples`; doctests in the umbrella
  pass `cargo test --doc` per §3 above.
- Every feature page opens with a spec § citation in the body's first
  paragraph (not just the frontmatter). Pattern: "Per ARCP v1.1
  §6.4, …".
- Every feature page carries a one-line TS-equivalent footer when
  the user might already know the TS surface. Pattern:
  `> TS equivalent: \`arcp.client.subscribe(jobId)\` in
  > [`typescript-sdk/packages/client/src/client.ts`](…).`
- Bullets that restate their heading fail review. Tables that would
  survive `s/Rust/<other-lang>/` (i.e., that do not cite a Rust
  type, crate, or `Cargo.toml` line) fail review.

## 6. Migration callouts

Two paragraphs, in this order, in README §8 ("Migrating"):

**From the v0.1 crate (`RFC-0001-v2`).** Not an additive upgrade —
the wire surface is wholly different. Summary of the deltas (full row
matrix in `02-current-audit.md` lines 30–46):

- Envelope: `arcp` field pins to `"1"` (was `"1.0"`); `event_seq`
  added; `source`/`target`/`stream_id`/`subscription_id`/
  `causation_id`/`priority` removed (`02-current-audit.md:59`).
- Handshake: 2-step `session.hello` → `session.welcome` (was 4-step
  `session.open` → `challenge` → `authenticate` → `accepted`,
  `02-current-audit.md:34`).
- Job model: single `job.event` stream + terminal `job.result` /
  `job.error` (was per-phase `job.started`/`progress`/`heartbeat`/
  `completed`/`failed`/`cancelled` envelopes,
  `02-current-audit.md:38`).
- Leases: immutable at `job.accepted` (was dynamic
  `permission.request` / `grant` / `lease.refresh` / `extended` /
  `revoked`, `02-current-audit.md:42`).
- Error codes: 15 domain codes per §12 (was gRPC-style 21 canonical
  codes, `02-current-audit.md:45`).
- Extensions: `x-vendor.*` (was `arcpx.*`, `02-current-audit.md:46`).
- Dropped wholesale: human-in-the-loop, built-in tool registry,
  artifact store, top-level `stream.*` and `permission.*` /
  `lease.*` envelopes, `signed_jwt` and `none` auth schemes
  (`02-current-audit.md:163-181`).

Reach for `docs/99-migration-from-rfc-0001.md` for the full deletion
list and the deprecation timeline (v0.1 is end-of-life at v1.1 GA;
parallel installs share a crate name and so cannot coexist on a single
`Cargo.toml`).

**From the TS SDK.** API-equivalent table, six anchor rows;
`docs/02-concepts.md` carries the full ~30-row version.

| TypeScript (`@arcp/sdk`)                                                                 | Rust (`arcp`)                                                                                                |
| ---------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------ |
| `new ARCPClient({ authScheme: "bearer", token })` (`typescript-sdk/packages/client/src/client.ts`) | `Client::connect(ClientConfig::bearer(token), transport)` in `arcp-client::Client`                            |
| `client.submit({ agent, input, leaseRequest? })` (`typescript-sdk/packages/client/src/client.ts`) | `client.submit(Submit::agent(name).input(value).lease_request(r))` in `arcp-client::Submit`                  |
| `handle.done` (Promise)                                                                  | `handle.done().await` returns `Result<JobOutcome, ARCPError>` in `arcp-client::JobHandle`                    |
| `client.subscribe(jobId, { history, fromEventSeq })`                                     | `client.subscribe(job_id, SubscribeOpts { history, from_event_seq })` returning `Pin<Box<dyn Stream<Item = Event> + Send>>` (per `02-current-audit.md` H-risk note, line 119) |
| `JobContext.streamResult({ resultId })` writer                                           | `JobContext::stream_result(StreamResultOpts { result_id })` writer; `#[must_use] ResultWriter` enforces `chunk_seq` monotone per `02-current-audit.md:124` |
| `JobHandle.collectChunks()`                                                              | `JobHandle::collect_chunks()` — `tokio::sync::mpsc` accumulator decoding `utf8`/`base64`, terminates on `more: false` |

Naming convention: TS `camelCase` methods become Rust `snake_case`;
TS option-bag objects become Rust newtype builders (`Submit`,
`SubscribeOpts`, `StreamResultOpts`) so the call-site reads
`Submit::agent("echo").input(v).lease_request(r)` rather than a struct
literal. This matches the TS option-object ergonomics without
inventing a positional-argument convention that diverges from idiomatic
Rust.

## 7. Anti-slop review checklist (applied to every doc page on PR)

- [ ] No banned word from `scripts/anti-slop.sh` (the §5 list above).
- [ ] No emoji.
- [ ] Every code block compiles (`cargo check --examples` or
      `cargo test --doc` covers it).
- [ ] Frontmatter present, all required fields, `order` a multiple
      of 10, `kind` in the closed set from §2.
- [ ] At least one citation per paragraph to a spec §, a TS path,
      a current-SDK path, or a named crate.
- [ ] No bullet restates its heading.
- [ ] No table that would survive `s/Rust/<other-lang>/`.
- [ ] Spec § cited in the first paragraph of any
      `kind: feature` / `kind: example` page.

## 8. Files this plan does *not* yet specify (deferred)

- `docs/04-examples/host-*.md` — Phase 5 host middleware
  (Hyper/Axum/Tower) ships in v1.2; the example pages follow.
- `docs/05-reference/arcp-core.md` etc. — these are *one paragraph
  + a docs.rs deep link*, not a re-statement of the rustdoc. The
  reference text itself lives in `cargo doc`.
- The `scripts/sync-docs.sh` / `scripts/anti-slop.sh` impls — Phase 7
  CI deliverables, mentioned here only as the enforcement seam.
- The full ~30-row TS→Rust equivalents table — drafted in
  `docs/02-concepts.md`, not the README; the README carries the
  six-row anchor table from §6.

## File deliverable

`/Users/nficano/code/arpc/rust-sdk/planning/v1.1/08-docs-readme.md`
