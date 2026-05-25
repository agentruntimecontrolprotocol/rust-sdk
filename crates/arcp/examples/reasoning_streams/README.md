# reasoning_streams

Primary emits a `kind: thought` reasoning stream; a mirror peer runtime
subscribes, critiques each thought, and delegates the critique back via
`agent.delegate`. The mirror is a peer, not a pure observer.

## Before ARCP

Chain-of-thought is a private prompt-engineering trick. There's no
standard way to expose it to a second model, redact it from third
parties, or budget it.

## With ARCP

```rust
// Primary:
client.send(envelope("stream.open", stream_id, payload={kind: "thought"}));
// chunks flow with role: "assistant_thought"

// Mirror peer:
let sub_id = subscribe(mirror, target_session_id, ["stream.chunk"]).await;
// consume kind: thought; delegate critique back
```

`token_budget` on the mirror caps cost; `unsubscribe` tears down cleanly.

## ARCP primitives

- `stream.open` / `stream.chunk` with `kind: "thought"` and
  `role: "assistant_thought"` — §11.4.
- `subscribe` with `types: ["stream.chunk"]` filter — §13.
- `agent.delegate` from mirror back to primary, carrying the critique
  in `context` — §14.

## File tour

- `main.rs` — primary loop + mirror loop, wired via an internal channel.
- `agents.rs` — `primary_step` + `critique_thought` LLM stubs.

## Variations

- Make the mirror redact + summarize before re-emitting.
- Cap critique frequency: "no more than 1 per N seconds."
- Run multiple mirrors at different specializations; vote.
