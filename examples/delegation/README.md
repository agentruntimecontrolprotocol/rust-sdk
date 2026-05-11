# delegation

Fan a request out to peer runtimes; tolerate partial failure. The
load-bearing pattern is `JobMux`: a single reader on `client.events()`
fans envelopes out to per-job `tokio::sync::mpsc` channels.

## Before ARCP

Parallel `for await env in client.events()` loops starve each other —
only one wins per await. Without a mux you serialize everything.

## With ARCP

```rust
let mux = JobMux::new();
mux.start(Arc::clone(&client));
for peer in PEERS {
    let job = delegate(&client, peer, request, trace_id).await;
    mux.register(job.job_id.clone()).await;
}
let completed = futures::future::join_all(jobs.iter().map(|j| collect(&mux, j))).await;
```

`trace_id` propagates so peers join one distributed trace.

## ARCP primitives

- `agent.delegate` — §14.
- `job.accepted` / `job.completed` / `job.failed` / `job.cancelled` —
  §10.
- `trace_id` propagation across peers — §17.
- `idempotency_key` for re-delegation safety — §6.4.

## File tour

- `main.rs` — fan-out + `JobMux` + `collect`.
- `synth.rs` — final synthesis stub.

## Variations

- Add a fastest-wins variant: cancel laggards once `min_results`
  arrive.
- Rebalance: track per-peer P50 latency from past runs to pick the
  best target order.
