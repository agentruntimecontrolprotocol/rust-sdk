# handoff

Cheap-tier first; escalate to deep tier via `agent.handoff` when
confidence is low. Transcript travels as an artifact (RFC §16); the
target runtime is pinned by `kind` + `fingerprint`.

## Before ARCP

Two-tier inference is usually a Python `if confidence < x:
deep_model.invoke(...)`. The "transcript" gets stuffed into a prompt
template and re-tokenized; nothing tracks the original session, the
upstream model identity, or the routing decision.

## With ARCP

```rust
let artifact = package_context(&cheap, transcript).await?;
emit_handoff(&cheap, artifact, trace_id).await?;
```

Artifact-by-reference; `target_runtime.fingerprint` pinned at handoff
time so the receiver's runtime kind can be verified.

## ARCP primitives

- `artifact.put` / `artifact.ref` for inline-base64 transcripts — §16.
- `agent.handoff` with `target_runtime.{url,kind,fingerprint}` — §14,
  §8.3.
- `shared_memory_ref` to point at the transcript artifact — §14.

## File tour

- `main.rs` — confidence gate + package + handoff.
- `cheap.rs` — cheap-tier `attempt` LLM stub.

## Variations

- Stream the transcript chunks instead of packaging at the end.
- Add a `shared_memory_ref` of `kind: "kv"` for shared scratch space
  between tiers.
