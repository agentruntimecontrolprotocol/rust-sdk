# heartbeats

Supervisor + worker pool. Heartbeat loss reroutes the in-flight task
via `idempotency_key` so a worker that survived a network blip dedupes
instead of re-executing.

## Before ARCP

Each scheduler reinvents heartbeats. Some use Redis TTLs. Some use a
sidecar liveness probe. Re-dispatch policy varies: most don't dedupe,
so a worker that stalled instead of died can re-run the same task.

## With ARCP

```rust
dispatch(&supervisor, Task {
    task_id: format!("t{n:03}"),
    idempotency_key: format!("openclaw:t{n:03}"),  // same on every re-dispatch
    ..
}, &mut roster, &mut jobs).await?;
```

Reaper enforces N=2 missed heartbeats per RFC §10.3. Re-dispatch reuses
the idempotency key; a survived worker dedupes.

## ARCP primitives

- `job.heartbeat` with `sequence` + `deadline_ms` + `state` — §10.3.
- N=2 missed-heartbeat policy — §10.3.
- `idempotency_key` for re-dispatch safety — §6.4.
- `agent.delegate` from supervisor to worker — §14.
- `session.evicted` for clean worker shutdown.

## File tour

- `main.rs` — supervisor + co-hosted worker fleet for the demo.
- `work.rs` — `do_work` stub for the actual job.

## Variations

- Vary `deadline_ms` per role (long-running vs latency-sensitive).
- Quarantine + drain a worker after N consecutive `job.failed`.
- Wire reap events back to a separate audit subscriber.
