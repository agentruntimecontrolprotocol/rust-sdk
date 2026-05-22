# Resume (§6.3)

Resume lets a client recover events after a transport drop. The Rust SDK stores
events in `EventLog` and can replay by session and event sequence.

Spec reference: [§6.3](../../../spec/docs/draft-arcp-1.1.md#63-resume).

## What to persist

Clients should persist:

- `session_id`
- resume token or runtime-specific resume credential
- highest processed `event_seq`

The runtime must retain events for the session until the resume window expires.
Use a file-backed event log when reconnect must survive runtime restarts.

## Replay

On resume, the runtime sends events with `event_seq > last_event_seq`. Sequence
numbers are session-scoped, so one high-water mark covers every job in the
session.

## Example

[`examples/resumability/`](../../examples/resumability/) demonstrates replaying
events after a session boundary.

## Failure modes

- `RESUME_WINDOW_EXPIRED` means the runtime no longer has enough buffered
  history for the session.
- `INVALID_REQUEST` means the resume tuple is malformed or claims an impossible
  sequence boundary.
- An in-memory event log loses resume history on restart.
