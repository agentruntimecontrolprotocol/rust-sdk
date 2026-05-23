# Jobs (§7)

Jobs are the unit of work in ARCP. A client submits an agent name plus input;
the runtime accepts, runs, emits events, and terminates with success,
cancellation, or error.

Spec reference: [§7](../../../spec/docs/draft-arcp-1.1.md#7-jobs).

## Submit and invoke

`Session<Authenticated>::invoke` is the direct Rust client helper for
submit-and-await flows. It sends a job request, waits for acceptance, and
resolves on the terminal result.

See [`examples/submit_and_stream.rs`](../../examples/submit_and_stream.rs).

## Runtime dispatch

Register tool handlers in `ToolRegistry`. A handler receives JSON input and a
`ToolContext` for cancellation, budgets, model-use enforcement, artifacts, and
event emission.

## State

The runtime tracks jobs through pending, running, and terminal states in
`JobRegistry`. Terminal outcomes include completed, failed, and cancelled.

## Cancellation

Cancellation uses `tokio_util::sync::CancellationToken`. Runtime code signals
the token; tool code should observe it and stop cooperatively.

See [`examples/cancellation.rs`](../../examples/cancellation.rs).

## Idempotency

Set `Envelope::idempotency_key` (§6.4) when retrying submit after a client
crash or reconnect. A duplicate request with identical content collapses to
the original job; a key collision with different content surfaces as
`ALREADY_EXISTS`.

See [`examples/idempotent_retry.rs`](../../examples/idempotent_retry.rs).

## Agent versions

Agent references can include versions. A pinned version that the runtime cannot
serve returns `AGENT_VERSION_NOT_AVAILABLE`.

See [`examples/agent_versions/`](../../examples/agent_versions/).

## Subscriptions

Clients can observe a job from another session when permitted by runtime
policy. Use job subscribe/unsubscribe for one job or generic subscriptions for
filtered envelope streams.

See [`examples/job_subscribe/`](../../examples/job_subscribe/) and
[`examples/subscriptions/`](../../examples/subscriptions/).
