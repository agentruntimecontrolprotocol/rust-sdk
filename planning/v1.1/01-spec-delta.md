# Phase 1 — Spec Delta v1.0 → v1.1

Source: `../spec/docs/draft-arcp-02.1.md`. Wire-level additions only;
v1.0 sections that were not changed are out of scope.

The `arcp` envelope field stays `"1"`. Every addition is gated by a
feature flag negotiated in `session.hello.capabilities.features` and
`session.welcome.capabilities.features`; the negotiated set is the
intersection (§6.2). A peer MUST NOT exercise a feature outside that
intersection.

## §6.2 Capability negotiation

| Field                                                    | Direction        | RFC level | Impact on a v1.0 client/runtime |
| -------------------------------------------------------- | ---------------- | --------- | ------------------------------- |
| `session.hello.payload.capabilities.features: string[]`  | client → runtime | MAY       | Additive; v1.0 omits it.        |
| `session.welcome.payload.capabilities.features: string[]`| runtime → client | SHOULD    | Additive; v1.0 omits it.        |
| `session.welcome.payload.heartbeat_interval_sec: number` | runtime → client | SHOULD    | Additive; v1.0 ignores it.      |
| `capabilities.agents` rich object shape                  | runtime → client | MAY       | Union with v1.0 `string[]`; clients accept either. |

Effective feature set is the lexical intersection of the two lists.
A v1.1 client talking to a v1.0 runtime sees an empty intersection and
degrades to v1.0 behaviour. The eight feature flags below all rely on
this gate; the §6.2 quote that pins it down is:

> "The effective feature set is the intersection of `session.hello`
> features and `session.welcome` features. Either peer MUST NOT use a
> feature outside that intersection."

## §6.4 Heartbeats — `heartbeat`

| Message         | Direction   | RFC level | Body                                          |
| --------------- | ----------- | --------- | --------------------------------------------- |
| `session.ping`  | either peer | MAY emit  | `{ nonce, sent_at }`                          |
| `session.pong`  | either peer | MUST reply| `{ ping_nonce, received_at }`                 |

Additive. Both messages MUST NOT be counted in `event_seq` — they are
session-control, not job events. Two consecutive silent intervals MAY
close the transport with `HEARTBEAT_LOST` (existing code, new trigger).
The runtime MUST NOT terminate jobs on heartbeat loss; the session
survives for the resume window.

## §6.5 Event acknowledgement — `ack`

| Message       | Direction        | RFC level | Body                          |
| ------------- | ---------------- | --------- | ----------------------------- |
| `session.ack` | client → runtime | MAY       | `{ last_processed_seq }`      |

Additive and advisory. Runtime:

- MAY free buffered events with `seq ≤ last_processed_seq` earlier than
  the time-based window (§6.5).
- MUST NOT free unacknowledged events before the window unless memory
  pressure forces eviction.
- MAY emit a `status { phase: "back_pressure", message? }` event when
  `emitted_seq − last_processed_seq` exceeds an implementation-defined
  threshold.

`session.ack` is NOT counted in `event_seq`. Resume continues to use
`last_event_seq` independently of `ack`.

## §6.6 Job listing — `list_jobs`

| Message            | Direction        | RFC level | Body                                                  |
| ------------------ | ---------------- | --------- | ----------------------------------------------------- |
| `session.list_jobs`| client → runtime | MAY       | `{ filter?, limit?, cursor? }`                        |
| `session.jobs`     | runtime → client | MUST reply| `{ request_id, jobs: JobListEntry[], next_cursor? }`  |

`filter`: optional `status[]`, `agent`, `created_after`, `created_before`.
`JobListEntry`: `{ job_id, agent, status, lease, parent_job_id, created_at,
trace_id, last_event_seq }`. Authorization defaults to same-principal
only; broader scopes are deployment-policy controlled. The runtime MUST
NOT leak job existence across principals. This is a read-only call — it
does not subscribe.

## §7.5 Agent versioning — `agent_versions`

Grammar: `agent ::= name | name "@" version` where `name` is
`[a-z0-9][a-z0-9._-]*` and `version` is `[a-zA-Z0-9.+_-]+`. Resolution:

- Bare `name` → `default` advertised in `session.welcome` agents
  inventory; if no `default`, runtime MAY pick any version (so clients
  needing stability MUST pin).
- `name@version` MUST resolve exactly; missing version is
  `AGENT_VERSION_NOT_AVAILABLE`.
- `job.accepted.payload.agent` and `session.jobs[].agent` MUST echo the
  resolved `name@version`. Once resolved, the running job's version is
  fixed — the runtime MUST NOT migrate it.

`capabilities.agents` rich shape:

```json
"agents": [
  { "name": "code-refactor", "versions": ["1.0.0", "2.0.0"], "default": "2.0.0" }
]
```

Clients SHOULD accept either the flat `string[]` shape (v1.0) or the
rich object shape (v1.1). The flat shape implies "no version info; bare
names only."

## §7.6 Subscription — `subscribe`

| Message            | Direction        | RFC level | Body                                                                      |
| ------------------ | ---------------- | --------- | ------------------------------------------------------------------------- |
| `job.subscribe`    | client → runtime | MAY       | `{ job_id, from_event_seq?, history? }`                                   |
| `job.subscribed`   | runtime → client | MUST reply| `{ job_id, current_status, agent, lease, parent_job_id, trace_id, subscribed_from, replayed }` |
| `job.unsubscribe`  | client → runtime | MAY       | `{ job_id }`                                                              |

Same-principal scope by default; `PERMISSION_DENIED` otherwise. After
`job.subscribed`, the subscribed job's `job.event` envelopes interleave
into the subscriber's session stream using the subscriber's `event_seq`
space — not the submitter's. Subscription confers observation only;
cancel authority stays with the submitting session.

`from_event_seq` + `history: true` replays buffered events whose
original sequence exceeds `from_event_seq`, bounded by the resume buffer
window. The replayed events carry the subscriber's new `event_seq`.

§7.7 cross-reference: subscribe ≠ resume. Resume continues the same
session and carries cancel authority; subscribe is a fresh session
without cancel authority. Dashboards SHOULD use subscribe; reconnecting
CLIs SHOULD use resume.

## §8.2.1 `progress` event kind

Reserved kind. Body: `{ current, total?, units?, message? }`.
`current` MUST be non-negative; `total` is OPTIONAL (absent = indeterminate);
if both are present, `current` SHOULD be ≤ `total`. Advisory only — the
protocol does not act on progress events.

## §8.4 Result streaming — `result_chunk`

Reserved kind. Body: `{ result_id, chunk_seq, data, encoding ∈ {utf8, base64}, more }`.
Constraints:

- Chunks for one `result_id` MUST be emitted in `chunk_seq` order (0-based,
  monotonic).
- Terminating `job.result` MUST carry the same `result_id` plus optional
  `result_size` and `summary`.
- A job MUST NOT mix inline `result` and `result_chunk`. Once any
  `result_chunk` is emitted, `job.result` MUST be the chunked variant.
- Assembled result = decoded `data` concatenated in `chunk_seq` order.

Security: implementations SHOULD cap individual chunk size (≈ 1 MB) and
total assembled size; exceeding either MUST return `INTERNAL_ERROR`
(§14).

## §9.5 Lease expiration — `lease_expires_at`

Additive `lease_constraints` field on `job.submit` and echoed on
`job.accepted`:

```json
"lease_constraints": { "expires_at": "2026-05-13T23:42:00Z" }
```

`expires_at` MUST be ISO 8601 with `Z` (UTC) and MUST be strictly in
the future at submission time; otherwise `INVALID_REQUEST`. Enforcement:

- Runtime MUST evaluate `expires_at` on every authority-bearing operation.
- Operations at-or-after `expires_at` MUST fail with `LEASE_EXPIRED`
  (surfaced as a `tool_result` error per §13.4).
- Runtime MUST emit `job.error { final_status: "error", code: "LEASE_EXPIRED" }`
  when the lease elapses while running; runtime MAY proactively terminate
  before a violation.
- No renewal API in v1.1 — to extend authority, cancel and resubmit.

Delegation (§9.4 additions): child `expires_at` MUST NOT exceed parent's;
child without `lease_constraints` inherits parent's implicitly (i.e.
child effective = `min(child_expires, parent_expires)`).

## §9.6 Budget — `cost.budget`

New reserved capability namespace; patterns are amount strings.

```
amount  ::= currency ":" decimal
currency ::= "USD" | "EUR" | "credits" | <runtime-defined>
```

Counters initialise from the lease at acceptance and decrement on
`metric` events whose `name` starts with `cost.` and whose `unit`
matches a budgeted currency. Negative values MUST be rejected (no
decrement). Operations against the lease MUST check all counters; any
counter ≤ 0 fails with `BUDGET_EXHAUSTED`, surfaced preferably as a
`tool_result` error so the agent can decide whether to continue with
non-cost-bearing work.

Runtime MAY emit `metric { name: "cost.budget.remaining", value, unit }`
proactively (debounced) so clients can render gauges without summing.

Delegation (§9.4 additions): child `cost.budget` per currency MUST NOT
exceed parent's *remaining* (not initial) budget at delegation time.

## §11 Trace propagation additions

Span attributes (RECOMMENDED, additive):

- `arcp.lease.expires_at` — when present on the job's lease.
- `arcp.budget.remaining` — encoded so per-currency counters are
  recoverable (e.g., JSON string of a `{ "USD": 1.42 }` map).

## §12 Error taxonomy — three additions

The v1.0 taxonomy of twelve grows to fifteen. All three are non-retryable
by default — `retryable: false` MUST be set, since naïve retry will
fail identically (§12).

| Code                          | Raised by                                                                                     |
| ----------------------------- | --------------------------------------------------------------------------------------------- |
| `AGENT_VERSION_NOT_AVAILABLE` | Runtime on `job.submit` when `name@version` cannot be resolved (§7.5). Returned as `session.error`. |
| `LEASE_EXPIRED`               | Lease-enforcement path when `now ≥ lease.expires_at` (§9.5). Returned via `tool_result.error` and `job.error`. |
| `BUDGET_EXHAUSTED`            | Lease-enforcement path when any `cost.budget` counter ≤ 0 (§9.6). Returned via `tool_result.error` and (optionally) `job.error`. |

## Additive vs breaking summary

Every v1.1 addition is additive at the wire level: a v1.0 client
connecting to a v1.1 runtime sends no feature flags, never receives a
new message kind, and the new fields on `session.welcome` /
`job.submit` / `job.accepted` are ignorable per envelope rules (§5.1
unknown-field rule). A v1.1 client connecting to a v1.0 runtime
discovers the empty feature intersection and refuses to call into the
v1.1 helpers (`client.ack`, `client.subscribe`, etc. — see TS
`packages/client/src/client.ts`).

There are NO new lifecycle states (§7.3 unchanged); `LEASE_EXPIRED` and
`BUDGET_EXHAUSTED` are flavours of the existing `final_status: "error"`.
There is NO new envelope field, transport, or wire-format change.
