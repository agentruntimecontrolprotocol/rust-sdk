# Phase 9 — Diagrams plan (v1.1)

Six load-bearing diagrams. No `.dot` source yet; this file fixes their
filenames, content, render targets, and shared style so Phase 9
execution is mechanical.

Cross-SDK alignment dominates path and style choices. The TypeScript
SDK ships `.dot` + paired `-light.svg`/`-dark.svg` under
`typescript-sdk/diagrams/` (`typescript-sdk/diagrams/README.md` lines
48–58). The Rust SDK adopts the same path — `rust-sdk/diagrams/` —
**not** `docs/diagrams/`. Mirroring the TS layout is the whole point
of "sibling SDKs look like siblings"; `docs/diagrams/` would
desynchronise the GitHub `<picture>` embeds the TS README already
documents (`typescript-sdk/diagrams/README.md` lines 3–11, 70–79).

Every `.dot` ships paired light+dark variants, matching
`typescript-sdk/diagrams/{architecture,session-handshake,job-lifecycle}-{light,dark}.dot`.
Where this plan writes `<name>.dot` below, deliver
`<name>-light.dot` + `<name>-dark.dot`; structure identical, only
colours differ (rule from `typescript-sdk/diagrams/README.md`
lines 39–46).

## 1. The minimum diagram set

| # | File (under `rust-sdk/diagrams/`)            | Citation                                                    | Load-bearing question it answers                                                                              |
| - | -------------------------------------------- | ----------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------- |
| a | `crates-{light,dark}.dot`                    | Phase 4 workspace layout (forthcoming `04-architecture.md`) | "Can I depend on `arcp-core` alone, or does it transitively pull in `tokio`/`rusqlite` from `arcp-runtime`?"  |
| b | `session-fsm-{light,dark}.dot`               | spec §6 (§6.2 caps, §6.3 resume, §6.4 heartbeat, §6.5 ack)  | "What client-visible state am I in when `session.welcome` is late vs when two pings are missed?"              |
| c | `job-fsm-{light,dark}.dot`                   | spec §7.3 lifecycle; §7.6 subscribe                         | "Which envelopes are legal in `running`, and does `job.subscribe` introduce a new state? (no — §7.6)"         |
| d | `capability-negotiation-{light,dark}.dot`    | spec §6.2                                                   | "How do I compute the effective feature set, and what happens if a v1.1 client talks to a v1.0 runtime?"      |
| e | `heartbeat-ack-{light,dark}.dot`             | spec §6.4 + §6.5                                            | "When does the runtime emit `status { phase: 'back_pressure' }`, and how does `session.ack` differ from §6.4?" |
| f | `result-chunk-{light,dark}.dot`              | spec §8.2.1 + §8.4                                          | "What's on the wire for a 4 MB result — when does `job.result` fire vs the terminator chunk?"                 |

### (a) Crate dependency graph — `crates-{light,dark}.dot`

Nodes (rounded box): `arcp-core`, `arcp-client`, `arcp-runtime`, the
umbrella `arcp`, and the middleware crates `arcp-middleware-otel` and
`arcp-middleware-rate-limit` (Phase 3 line 71–80 carve OTel out as
middleware; `02-current-audit.md` line 65 splits the message family
along the same axis). Edges are direct `Cargo.toml` dependencies
between workspace crates only — not transitive, not external. `arcp`
is the ENTRY anchor (blue, `#3B82F6`); `arcp-core` is the HUB anchor
(amber, `#F59E0B`) — every other crate depends on it. Stores
(`shape=cylinder`): none here.

Edge labels: cargo feature gate when one applies (e.g.,
`arcp-client → arcp-core [label="transport-ws"]`). No reverse edges,
no cycles — if Graphviz draws a cycle the layout is wrong, not the
crate graph. The diagram answers "can I depend only on `arcp-core`?"
in one glance: yes, iff `arcp-core` has no outgoing arrows to a
sibling crate.

### (b) Session lifecycle FSM — `session-fsm-{light,dark}.dot`

States (`shape=box, style="rounded,filled"`, default fill): `Connecting`,
`Hello`, `Welcomed`, `Active`, `Resuming`, `Closed`. `Closed` is the
terminal/error anchor — use the red treatment from the shared style
preamble (`color="#b00020", style="rounded,bold"`). `Active` is the
HUB anchor (amber) because §6.4 pings and §6.5 acks both self-loop
there. `Connecting` is the ENTRY anchor (blue).

Edges, labelled with the triggering envelope:

- `Connecting → Hello` `[label="send session.hello"]` — §6.2 hello with
  `capabilities.features`.
- `Hello → Welcomed` `[label="recv session.welcome"]` — §6.2 welcome
  carries the effective feature set and `heartbeat_interval_sec`.
- `Hello → Closed` `[label="session.error\\nUNAUTHENTICATED |\\nAGENT_VERSION_NOT_AVAILABLE"]`
  — §12 error taxonomy (`01-spec-delta.md` lines 210–214).
- `Welcomed → Active` `[label="first job.submit\\nor first job.event"]`.
- `Active → Active` `[label="session.ping ↔ session.pong\\nsession.ack { last_processed_seq }"]`
  — self-loop covers §6.4 + §6.5. Use the dashed pink feedback edge
  style from the TS template (`typescript-sdk/diagrams/README.md`
  lines 204–215) for this self-loop so it reads as "background
  traffic, not a state change".
- `Active → Resuming` `[label="transport drops\\n(within resume window)"]`
  — §6.3.
- `Resuming → Active` `[label="session.hello { resume: { resume_token,\\nlast_event_seq } } → session.welcome"]`.
- `Active → Closed` `[label="session.bye | HEARTBEAT_LOST"]` — §6.4
  two-silent-intervals trigger.
- `Resuming → Closed` `[label="RESUME_WINDOW_EXPIRED"]` — §12.

No state for "subscribed" — subscription is per-job, not per-session
(§7.6, `01-spec-delta.md` lines 120–123).

### (c) Job lifecycle FSM — `job-fsm-{light,dark}.dot`

States (§7.3, exactly these — `01-spec-delta.md` line 228 reaffirms
v1.1 adds no new states): `pending`, `running`, `success`, `error`,
`cancelled`, `timed_out`. `running` is the HUB anchor (amber).
`pending` is the ENTRY anchor (blue). `error`, `cancelled`,
`timed_out` use the red terminal treatment from the shared preamble;
`success` uses default fill (success isn't an error).

Edges, labelled by triggering envelope or condition:

- `pending → running` `[label="job.accepted\\n+ first job.event"]` — §7.1.
- `running → success` `[label="job.result\\n(or terminator result_chunk\\n+ job.result)"]` — §8.4.
- `running → error` `[label="job.error\\n{ final_status: 'error' }"]`. Three
  edges fan in from a dashed annotation cluster labelled
  "lease/budget enforcement" listing
  `LEASE_EXPIRED` (§9.5), `BUDGET_EXHAUSTED` (§9.6),
  and `INTERNAL_ERROR` so readers see why the runtime synthesises a
  `job.error` without an agent-originated message.
- `running → cancelled` `[label="job.cancel → job.error\\n{ final_status: 'cancelled' }\\nwithin 30s grace"]` — §7.4.
- `running → timed_out` `[label="max_runtime_sec elapsed →\\njob.error { final_status: 'timed_out' }"]`.

§7.6 subscribe annotation: a side note (separate rounded-box node
labelled "observer attaches via job.subscribe →\\njob.subscribed
(§7.6); does NOT change state") connected to `running` by a dashed
pink feedback edge with `constraint=false` so it does not distort the
state-machine layout. The annotation node uses the cluster-label
style (muted slate text), not state styling — readers should not
mistake it for a state.

### (d) Capability negotiation sequence — `capability-negotiation-{light,dark}.dot`

`digraph` with `rankdir=LR` and explicit `rank=same` constraints to
force a two-column sequence layout (the same trick used in
`typescript-sdk/diagrams/session-handshake-light.dot` lines 47–64).
Two column anchors at the top: `Client` (ENTRY anchor, blue) on the
left, `Runtime` (HUB anchor, amber) on the right. The TS handshake
diagram uses the identical column treatment, so this Rust diagram
sits beside it without visual conflict.

Nodes for each step (left-to-right messages — `shape=note` to mark
them as wire messages, per the shared preamble):

1. `Hello [label="session.hello\\n{ capabilities:\\n  { features: [\\n    'heartbeat', 'ack',\\n    'list_jobs', 'subscribe',\\n    'progress', 'result_chunk',\\n    'agent_versions', 'lease_expires_at',\\n    'cost.budget'\\n  ] } }", shape=note]`
   — client's superset (the eight v1.1 feature flags from
   `01-spec-delta.md` line 21, plus `cost.budget`).
2. `Welcome [label="session.welcome\\n{ capabilities:\\n  { features: [\\n    'heartbeat', 'ack',\\n    'subscribe', 'progress',\\n    'result_chunk'\\n  ],\\n    heartbeat_interval_sec: 30 } }", shape=note]`
   — runtime's reply, deliberately a subset to show intersection in
   action.
3. `Effective [label="effective = ∩ =\\n{ heartbeat, ack, subscribe,\\n  progress, result_chunk }\\n— either peer MUST NOT use\\na feature outside this set\\n(§6.2)", shape=note, style="rounded,filled,dashed"]`
   — pulled out as an annotation so the rule jumps off the page.
4. Failure-mode annotation node:
   `V10Runtime [label="v1.0 runtime:\\nsession.welcome omits\\ncapabilities.features →\\neffective = ∅ →\\nclient degrades to v1.0", shape=note, style="rounded,filled,dashed"]`
   — clarifies the cross-version interop rule the spec pins down
   (`01-spec-delta.md` lines 21–28).

Edges:
`Client -> Hello -> Runtime` (primary spine);
`Runtime -> Welcome -> Client` (primary spine);
`Hello -> Effective [style=dashed, constraint=false]`,
`Welcome -> Effective [style=dashed, constraint=false]`
(annotation pointers, dashed pink per shared preamble);
`Runtime -> V10Runtime [style=dashed, constraint=false]`.

`rank=same; Client; Runtime;` keeps the two participants on the top
row. The two message clusters below stack vertically by rank to give
the sequence-diagram read order without resorting to PlantUML.

### (e) Heartbeat + ack flow — `heartbeat-ack-{light,dark}.dot`

`rankdir=TB` two-column layout (Client left, Runtime right) using the
same `rank=same` trick. Client is ENTRY anchor (blue); Runtime is
HUB anchor (amber). Three time bands stacked top-to-bottom, each its
own cluster:

- **Idle band** — `Client → Runtime [label="session.ping { nonce, sent_at }"]`
  and `Runtime → Client [label="session.pong { ping_nonce, received_at }"]`.
  Symmetric reverse pair below it (`Runtime → Client` ping, client
  pong) — §6.4 explicitly allows either peer to initiate.
- **Streaming band** — `Runtime → Client [label="job.event { event_seq: N }"]`
  (three or four parallel edges to suggest a stream),
  `Client → Runtime [label="session.ack { last_processed_seq: M }"]`
  (§6.5, M ≤ N), annotated "MAY emit; advisory" in a side note node.
- **Back-pressure band** — `Runtime → Client [label="job.event { event_seq: N }\\nN — M > threshold"]`
  followed by `Runtime → Client [label="job.event { kind: 'status',\\nbody: { phase: 'back_pressure' } }", style=dashed]`
  (§6.5 emission rule). Dashed pink because it is a feedback signal
  about lag, not protocol progress.

Annotation node (cluster-label style):
"session.ping/pong and session.ack are NOT counted in event_seq
(§6.4, §6.5). HEARTBEAT_LOST after two silent intervals → session
closes; running jobs are NOT terminated and survive the resume
window (§6.4, §6.3)."

### (f) Result_chunk + progress event sequence — `result-chunk-{light,dark}.dot`

`rankdir=TB`. Two columns (Agent → Client, with Runtime as the
forwarder in the middle column when useful). The streaming run:

- `progress [kind=progress, body={ current: 0, total: 1000, units: 'rows' }]`
  — first event, optional but common (§8.2.1).
- `result_chunk { result_id: R, chunk_seq: 0, data: '…', encoding: 'utf8', more: true }`
- `progress [body={ current: 250, total: 1000 }]` — interleaved.
- `result_chunk { result_id: R, chunk_seq: 1, …, more: true }`
- … (vertical ellipsis node, default fill, label `…`)
- `result_chunk { result_id: R, chunk_seq: N, data: '', encoding: 'utf8', more: false }`
  — terminator, drawn with the red terminal treatment from the
  shared preamble so readers spot the `more: false` boundary.
- `job.result { result_id: R, result_size, summary? }` — terminating
  envelope (§8.4 rule: same `result_id`).

Annotation nodes (cluster-label style, no border):

- "Chunks MUST be emitted in chunk_seq order (0-based, monotonic)
  per result_id (§8.4)."
- "A job MUST NOT mix inline `result` with `result_chunk`. Once any
  `result_chunk` is emitted, `job.result` MUST be the chunked
  variant (§8.4)."
- "Implementations SHOULD cap individual chunk size ≈ 1 MB and
  total assembled size; exceeding either MUST return INTERNAL_ERROR
  (§8.4 + §14)."

Primary spine carries the chunk sequence; dashed pink feedback edges
carry the interleaved `progress` events so they read as out-of-band
relative to the chunk spine.

## 2. Per-diagram metadata

| Diagram                                  | Sources                                             | Render command                                                       | Render target               |
| ---------------------------------------- | --------------------------------------------------- | -------------------------------------------------------------------- | --------------------------- |
| (a) crates                               | `crates-light.dot`, `crates-dark.dot`               | `dot -Tsvg crates-light.dot -o crates-light.svg` (and dark)          | SVG primary; PNG 2x fallback |
| (b) session FSM                          | `session-fsm-light.dot`, `session-fsm-dark.dot`     | `dot -Tsvg session-fsm-light.dot -o session-fsm-light.svg`           | SVG primary; PNG 2x fallback |
| (c) job FSM                              | `job-fsm-light.dot`, `job-fsm-dark.dot`             | `dot -Tsvg job-fsm-light.dot -o job-fsm-light.svg`                   | SVG primary; PNG 2x fallback |
| (d) capability negotiation               | `capability-negotiation-light.dot`, …`-dark.dot`    | `dot -Tsvg capability-negotiation-light.dot -o …-light.svg`          | SVG primary; PNG 2x fallback |
| (e) heartbeat + ack                      | `heartbeat-ack-light.dot`, `heartbeat-ack-dark.dot` | `dot -Tsvg heartbeat-ack-light.dot -o heartbeat-ack-light.svg`       | SVG primary; PNG 2x fallback |
| (f) result_chunk + progress              | `result-chunk-light.dot`, `result-chunk-dark.dot`   | `dot -Tsvg result-chunk-light.dot -o result-chunk-light.svg`         | SVG primary; PNG 2x fallback |

SVG is the primary output because docs sites scale it without
re-render and GitHub's `<picture>` element auto-switches the paired
light/dark variants by `prefers-color-scheme`
(`typescript-sdk/diagrams/README.md` lines 14–21, 70–79). PNG at 2x
(`dot -Tpng -Gdpi=192 …`) is the fallback for any renderer that
chokes on SVG; render PNG only on demand, not by default.

### Shared style preamble

Every `.dot` opens with the same block. The TS template already pins
the slate palette and two-anchor discipline
(`typescript-sdk/diagrams/diagram-template-light.dot` lines 64–97 and
the dark counterpart lines 10–43); the Rust diagrams reuse it
verbatim so the two SDKs render as visual siblings. The block below
extends the TS preamble with the **state-machine shapes** the task
requires (`shape=note` for messages, red terminal treatment for
error/closed states) — the TS templates leave these as user-level
overrides and the Rust set needs them in the defaults.

```dot
// SHARED — light variant. The dark companion swaps fills/borders per
// the TS palette table (typescript-sdk/diagrams/README.md
// lines 100–119) and otherwise keeps structure identical.
//
// CANVAS
rankdir=TB;             // sequence diagrams override to LR
bgcolor="transparent";
compound=true;
fontname="Helvetica";
splines=spline;
nodesep=0.32;
ranksep=0.55;
pad="0.35,0.25";

// NODE DEFAULTS — state nodes (FSM diagrams)
node [
  shape=box, style="rounded,filled",
  fillcolor="white", color="#CBD5E1",
  fontname="Helvetica", fontsize=11, fontcolor="#1F2937",
  margin="0.22,0.11", penwidth=1.0
];

// EDGE DEFAULTS
edge [
  fontname="Helvetica", fontsize=10,        // 10 not 9 — wire-label legibility
  fontcolor="#64748B", color="#94A3B8",
  penwidth=1.1, arrowsize=0.75, arrowhead=normal
];

// VARIANT OVERRIDES — apply per-node, not as defaults:
//   message-event nodes:    [shape=note]
//   error/terminal nodes:   [shape=box, style="rounded,filled,bold",
//                            color="#b00020", fontcolor="#b00020"]
//   ENTRY anchor (use once): fillcolor="#3B82F6", color="#2563EB",
//                            fontcolor="white", penwidth=1.4
//   HUB anchor (use once):   fillcolor="#F59E0B", color="#D97706",
//                            fontcolor="white", penwidth=1.4
//   feedback / async edge:   [style=dashed, color="#F472B6",
//                             constraint=false]
```

`arrowhead=normal` (task spec) replaces the TS template's
`arrowhead=vee`. This is the only style divergence from the TS
preamble; it matters because the Rust FSMs lean harder on
state-machine transition arrows where `normal` reads less like a
data-flow vee. State the rule once in this preamble; each `.dot`
file pastes the block verbatim. Do not duplicate the rule per file.

## 3. Rejected diagrams

- **Module map of `arcp-runtime`** — rejected. `cargo doc` already
  renders the module tree under `target/doc/arcp_runtime/`; a
  Graphviz duplicate goes stale the first time someone adds a
  module. The crate-dependency diagram (a) answers the only
  cross-module question that survives doc generation.
- **Per-message wire-frame layout** — rejected. The envelope shape
  (`arcp`, `type`, `id`, `session_id`, `event_seq`, `payload`) is a
  six-field flat object; a diagram of it carries less information
  than the table in spec §5.1 and would be out of sync the first
  time a field is added.

## 4. CI rendering

Check the SVGs into the repo. PR diffs stay reviewable
(Graphviz output is deterministic for the same `.dot` + same
Graphviz version, so a diff that changes the SVG without changing the
`.dot` is a CI-version drift signal worth catching). The alternative
— rendering in CI — would add Graphviz to every CI image and break
local `cargo doc` previews that link to the SVGs.

`make diagrams` re-renders all twelve outputs (six diagrams × light +
dark) from `.dot` sources. The exact target, to live at the repo
root `Makefile`:

```make
DOT_SOURCES := $(wildcard diagrams/*.dot)
SVG_OUTPUTS := $(DOT_SOURCES:.dot=.svg)

diagrams: $(SVG_OUTPUTS)

diagrams/%.svg: diagrams/%.dot
	dot -Tsvg $< -o $@
```

CI runs `make diagrams && git diff --exit-code diagrams/` to fail on
drift between `.dot` and committed SVG. Graphviz version pinned via
`.tool-versions` (or a CI matrix line) so the diff is reproducible.

## 5. Cross-SDK style alignment

| Concern              | TypeScript SDK                                                            | Rust SDK                              |
| -------------------- | ------------------------------------------------------------------------- | ------------------------------------- |
| Directory            | `typescript-sdk/diagrams/`                                                | `rust-sdk/diagrams/` — **match**      |
| Source files         | Paired `*-light.dot` + `*-dark.dot`                                       | Same pairing                          |
| Output files         | Paired `*-light.svg` + `*-dark.svg`, embedded via GitHub `<picture>`      | Same                                  |
| State / process node | `shape=box, style="rounded,filled"`                                       | Same                                  |
| Store node           | `shape=cylinder`                                                          | Same (none of the six need one)       |
| ENTRY anchor         | `#3B82F6` fill / `#2563EB` border, white text, `penwidth=1.4`             | Same                                  |
| HUB anchor           | `#F59E0B` fill / `#D97706` border, white text, `penwidth=1.4`             | Same                                  |
| Cluster fills        | Outer `#F1F5F9` / inner `#F8FAFC` (light); `#0F172A` / `#1E293B` (dark)   | Same                                  |
| Primary edge         | `#64748B` (light) / `#94A3B8` (dark), `penwidth=1.2`                      | Same                                  |
| Secondary edge       | `#CBD5E1` (light) / `#475569` (dark), `penwidth=1.0`                      | Same                                  |
| Feedback edge        | Dashed `#F472B6`, `constraint=false`, label `#DB2777`/`#F472B6`           | Same                                  |
| Arrowhead            | `arrowhead=vee` (data-flow)                                               | `arrowhead=normal` (state-machine)    |
| Message-event node   | not in template (TS diagrams don't include sequence diagrams)             | `shape=note` (defined in this plan)   |
| Error/terminal node  | not in template                                                           | `style="rounded,bold", color="#b00020"` (defined in this plan) |

The two Rust-only additions (`shape=note` for message events, red
terminal treatment for error states) are introduced as **per-node
overrides** rather than new defaults. If/when the TS SDK adopts
sequence or FSM diagrams it can paste the same two rules into its
template without touching the existing defaults block.
