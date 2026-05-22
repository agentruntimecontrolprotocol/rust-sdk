# Job events (§8)

Job events are the stream of observable work emitted while a job runs.

Spec reference: [§8](../../../spec/docs/draft-arcp-1.1.md#8-job-events).

## Reserved event kinds

The Rust SDK represents the standard ARCP event shapes in
`arcp::messages::execution` and related modules:

| Kind | Purpose |
| --- | --- |
| `log` | Human-readable log line with level and attributes. |
| `thought` | Reasoning or internal agent note when a deployment exposes it. |
| `tool_call` | Agent requested a tool operation. |
| `tool_result` | Tool operation result or structured ARCP error. |
| `status` | Lifecycle status such as running or progress. |
| `metric` | Numeric measurement. |
| `artifact_ref` | Reference to externally or runtime-stored artifact. |
| `delegate` | Child job request. |

## Progress

Progress is represented as structured status data and demonstrated in
[`examples/progress.rs`](../../examples/progress.rs).

## Result chunks

Large results can be chunked with `result.chunk` and completed with a terminal
result reference. The SDK validates ordering, encoding, and terminal behavior.

See [`examples/result_chunk/`](../../examples/result_chunk/).

## Sequence numbers

`event_seq` is session-scoped. One strictly increasing counter spans every job
in the session, which lets ack and resume use a single high-water mark.

## Vendor event kinds

Custom events must use `x-vendor.*` names. The extension registry classifies
core, advertised vendor, unadvertised vendor, experimental, and malformed names.
