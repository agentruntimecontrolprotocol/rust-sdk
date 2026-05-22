# Errors (§12)

The SDK maps ARCP error codes to `ARCPError` and `ErrorCode`.

Spec reference: [§12](../../../spec/docs/draft-arcp-1.1.md#12-error-taxonomy).

## Codes

| Code | Typical source |
| --- | --- |
| `INVALID_REQUEST` | Malformed envelope, bad payload, impossible resume boundary. |
| `UNAUTHENTICATED` | Missing or invalid session credential. |
| `PERMISSION_DENIED` | Lease or authorization check failed. |
| `JOB_NOT_FOUND` | Unknown job id. |
| `AGENT_NOT_AVAILABLE` | Agent name not registered. |
| `AGENT_VERSION_NOT_AVAILABLE` | Requested version is unavailable. |
| `CANCELLED` | Job cancelled cooperatively. |
| `TIMEOUT` | Runtime deadline expired. |
| `INTERNAL_ERROR` | Unexpected runtime failure. |
| `LEASE_SUBSET_VIOLATION` | Child lease exceeds parent lease. |
| `LEASE_EXPIRED` | Lease constraint has expired. |
| `BUDGET_EXHAUSTED` | Budget counter has been depleted. |
| `RESUME_WINDOW_EXPIRED` | Runtime can no longer replay the session. |
| `HEARTBEAT_LOST` | Peer missed heartbeat expectations. |
| `DUPLICATE_KEY` | Idempotency key reused with different content. |

## Retry guidance

Retry only errors that are transient in your deployment, usually transport
loss, timeout, heartbeat loss, or internal failures. Combine retries with an
idempotency key so duplicate submits collapse to the same job.

## Tool errors

Tool handlers should return `ARCPError` when a failure should become a protocol
error. Application-level tool failures can also be encoded as `tool_result`
events so the agent can recover without failing the entire job.

## Tests

Error-code serialization and retryability are covered in [`src/error.rs`](../../src/error.rs).
