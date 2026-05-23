# Errors (§12)

The SDK maps ARCP error codes to `ARCPError` and `ErrorCode`. The full
taxonomy lives in [`src/error.rs`](../../src/error.rs); the codes below
mirror the `ErrorCode` enum verbatim.

Spec reference: [§12](../../../spec/docs/draft-arcp-1.1.md#12-error-taxonomy).

## Codes

| Code | Typical source |
| --- | --- |
| `CANCELLED` | Job cancelled cooperatively by caller, runtime, or policy. |
| `UNKNOWN` | Failure without a more specific code (rare; avoid in new code). |
| `INVALID_ARGUMENT` | Malformed envelope, bad payload, impossible resume boundary. |
| `DEADLINE_EXCEEDED` | Runtime, job, or lease deadline expired. |
| `NOT_FOUND` | Unknown job, agent, artifact, or other referenced entity. |
| `ALREADY_EXISTS` | Entity creation conflicted with an existing entity. |
| `PERMISSION_DENIED` | Lease or authorization check failed. |
| `RESOURCE_EXHAUSTED` | Quota or rate limit hit (alias `RATE_LIMITED` accepted on the wire). |
| `FAILED_PRECONDITION` | Required pre-condition unmet (e.g. job not in cancellable state). |
| `ABORTED` | Concurrency conflict or hard termination. |
| `OUT_OF_RANGE` | Argument outside the valid range. |
| `UNIMPLEMENTED` | Surface not supported by this build. |
| `INTERNAL` | Unexpected runtime failure (should be rare). |
| `UNAVAILABLE` | Transient unavailability; retry MAY succeed. |
| `DATA_LOSS` | Unrecoverable data loss (e.g. resume retention expired). |
| `UNAUTHENTICATED` | Missing or invalid session credential. |
| `HEARTBEAT_LOST` | Peer missed heartbeat expectations (§6.4). |
| `LEASE_EXPIRED` | Lease has expired (§9.5). |
| `LEASE_REVOKED` | Lease was revoked by the grantor (§9.5). |
| `BACKPRESSURE_OVERFLOW` | Subscription or stream dropped due to backpressure overflow. |
| `BUDGET_EXHAUSTED` | `cost.budget` counter depleted (§9.6). |
| `LEASE_SUBSET_VIOLATION` | Child lease exceeds parent envelope (§9.4). |
| `AGENT_VERSION_NOT_AVAILABLE` | Requested `agent@version` is unavailable (§7.5). |

## Retry guidance

`ErrorCode::retryable()` is the in-process default. Retryable codes are
`RESOURCE_EXHAUSTED`, `UNAVAILABLE`, `DEADLINE_EXCEEDED`, `INTERNAL`, and
`ABORTED`. Everything else — including `LEASE_EXPIRED`, `BUDGET_EXHAUSTED`,
`PERMISSION_DENIED`, and `HEARTBEAT_LOST` — is non-retryable by default; a
naive retry will fail identically.

Combine retries with an idempotency key (set `Envelope::idempotency_key`
before sending) so duplicate submits collapse to the same job; a key collision
with different content surfaces as `ALREADY_EXISTS`.

## Tool errors

Tool handlers should return `ARCPError` when a failure should become a protocol
error. Application-level tool failures can also be encoded as `tool_result`
events so the agent can recover without failing the entire job.

## Tests

Error-code serialization and retryability are covered in [`src/error.rs`](../../src/error.rs).
