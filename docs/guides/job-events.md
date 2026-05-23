# Job events (§8)

Job events are the stream of observable work emitted while a job runs.

Spec reference: [§8](../../../spec/docs/draft-arcp-1.1.md#8-job-events).

## Reserved event types

The Rust SDK represents the standard ARCP event shapes as variants of
`MessageType` (`arcp::messages`). Wire-level type strings — what appears in
the envelope's `type` field — are:

| Type | Purpose |
| --- | --- |
| `log` | Human-readable log line with level and attributes (`LogPayload`). |
| `metric` | Numeric measurement (`MetricPayload`). |
| `trace.span` | Span emitted for distributed tracing (`TraceSpanPayload`). |
| `tool.invoke` | Agent requested a tool operation. |
| `tool.result` | Tool operation result. |
| `tool.error` | Structured tool failure. |
| `job.started` | Job entered the running state. |
| `job.progress` | Optional `percent`/`message` progress update (`JobProgressPayload`). |
| `job.heartbeat` | Liveness signal from a long-running job (`JobHeartbeatPayload`). |
| `job.result_chunk` | One fragment of a streamed final result (ARCP v1.1 §8.4). |
| `job.completed` / `job.failed` / `job.cancelled` | Terminal outcomes. |
| `artifact.ref` | Reference to a runtime-stored artifact. |
| `agent.delegate` | Child job request (`AgentDelegatePayload`). |
| `agent.handoff` | Hand work to another agent (`AgentHandoffPayload`). |
| `event.emit` | Generic carrier for namespaced custom events. |

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
