# Job events (§8)

Job events are the stream of observable work emitted while a job runs.

Spec reference: [§8](../../../spec/docs/draft-arcp-1.1.md#8-job-events).

## Reserved event types

The Rust SDK represents the standard ARCP event shapes as top-level
`MessageType` variants (`arcp::messages`). Each row below maps the SDK's
wire-level `type` string to the spec's §8.2 event-kind name and notes any
divergence.

> The spec's §8.2 event kinds (`progress`, `result_chunk`, `log`,
> `thought`, `tool_call`, `tool_result`, `status`, `metric`,
> `artifact_ref`, `delegate`) are defined as `kind` discriminators
> inside a single `job.event` envelope. The Rust SDK currently models
> them as separate top-level `MessageType` variants with `job.`-prefixed
> wire types (e.g. `job.progress`). The wire shape mapping is below;
> the two will be reconciled in a future major release.
>
> See the audit findings for the planned restructure; until then,
> consumers should expect `job.<kind>` on the wire and not the bare
> `<kind>` shape shown in the spec.

| SDK wire `type` | Spec §8.2 `kind` | Purpose |
| --- | --- | --- |
| `log` | `log` | Human-readable log line with level and attributes (`LogPayload`). |
| `metric` | `metric` | Numeric measurement (`MetricPayload`). |
| `trace.span` | (SDK extension) | Span emitted for distributed tracing (`TraceSpanPayload`). |
| `tool.invoke` | `tool_call` | Agent requested a tool operation. |
| `tool.result` | `tool_result` | Tool operation result. |
| `tool.error` | `tool_result` (error form) | Structured tool failure. |
| `job.started` | `status` (`phase: "started"`) | Job entered the running state. |
| `job.progress` | `progress` | Optional `percent`/`message` progress update (`JobProgressPayload`). |
| `job.heartbeat` | (SDK extension; not in §8.2) | Liveness signal from a long-running job (`JobHeartbeatPayload`). |
| `job.result_chunk` | `result_chunk` (ARCP v1.1 §8.4) | One fragment of a streamed final result. |
| `job.completed` / `job.failed` / `job.cancelled` | Terminal `job.result` / `job.error` (§7.3) | Terminal outcomes. |
| `artifact.ref` | `artifact_ref` | Reference to a runtime-stored artifact. |
| `agent.delegate` | `delegate` | Child job request (`AgentDelegatePayload`). |
| `agent.handoff` | (SDK extension) | Hand work to another agent (`AgentHandoffPayload`). |
| `event.emit` | (extension carrier) | Generic carrier for namespaced custom events. |

## Progress

`job.progress` is the structured progress message; see
[`examples/progress.rs`](../../examples/progress.rs).

## Result chunks

Large results stream as `job.result_chunk` envelopes terminated by a
`job.completed` carrying the same `result_id`. `ResultChunkAssembler`
(in `arcp::messages`) validates ordering, encoding, and the terminal
boundary.

See [`examples/result_chunk/`](../../examples/result_chunk/).

## Sequence numbers

`event_seq` is session-scoped. One strictly increasing counter spans every
countable envelope in the session (handshake, heartbeat, and ack envelopes
are not counted — see `MessageType::is_countable_event`), which lets ack
and resume use a single high-water mark.

## Vendor event types

Per RFC §21.1 extension `type` strings follow `arcpx.<vendor>.<name>.v<n>`
or a reverse-DNS form such as `com.acme.workflow.v2`. The bare `x-` prefix
is reserved for transport-internal experimental fields and MUST NOT be
used in long-lived deployments. `ExtensionRegistry` classifies a `type`
string as core, known extension, unknown extension, experimental, or
malformed.
