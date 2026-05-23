# Sessions (§6)

A session is a long-lived ARCP context over one transport. It starts with a
handshake, carries job traffic, and ends with `session.close` or transport
drop.

Spec reference: [§6](../../../spec/docs/draft-arcp-1.1.md#6-sessions).

## Handshake

The Rust SDK names the v1.1 handshake messages `SessionOpen`,
`SessionAccepted`, `SessionRejected`, and `SessionUnauthenticated`.

```
client  -> runtime  session.open
runtime -> client   session.accepted
        -> client   session.rejected or session.unauthenticated
```

`Session<Unauthenticated>` exposes only `authenticate`. A successful handshake
returns `Session<Authenticated>`, which exposes job and runtime operations.

## Capabilities

`Capabilities` declares optional surfaces such as anonymous auth, streaming,
artifacts, subscriptions, model use, provisioned credentials, and feature
support. The runtime advertises what it supports; the accepted session records
the negotiated capabilities.

## Ack and back-pressure

The runtime can be configured with `RuntimeBuilder::with_ack_window`. Once the
number of emitted but unacknowledged countable envelopes reaches the window,
writers pause until the client sends `session.ack`.

See [`examples/ack_backpressure.rs`](../../examples/ack_backpressure.rs).

## Heartbeat

`session.ping` and `session.pong` (ARCP v1.1 §6.4) keep long-lived sessions
fresh and detect stalled peers. See
[`examples/session_heartbeat/`](../../examples/session_heartbeat/).

## Listing and subscribing

The runtime implements session job listing plus generic and job-scoped
subscriptions:

- `session.list_jobs` for visible jobs in the session.
- `job.subscribe` / `job.unsubscribe` for cross-session job observation.
- `subscribe` / `unsubscribe` for filtered envelope fanout.

See [`examples/session_list_jobs/`](../../examples/session_list_jobs/) and
[`examples/job_subscribe/`](../../examples/job_subscribe/).

## Closing

Either side may send `session.close` (`SessionClosePayload`). After close,
job-scoped traffic must stop and the transport should be closed by the
owner. In-flight jobs continue server-side and remain resumable until the
runtime's retention window expires.
