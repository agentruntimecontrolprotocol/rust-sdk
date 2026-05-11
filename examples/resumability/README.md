# resumability

A durable research job that **actually crashes and resumes**. Set
`CRASH_AFTER_STEP=synthesize` and the example calls
`std::process::exit(137)` mid-flight. A second invocation with the
printed `RESUME_*` env vars picks up at the next step.

## Before ARCP

"Durable workflow" usually means a workflow engine: Temporal,
Cadence, Airflow. Each owns its own runtime, its own SDK, its own
store, and won't compose with another.

## With ARCP

```text
CRASH_AFTER_STEP=synthesize cargo run --example resumability
# crash; prints RESUME_JOB_ID, RESUME_AFTER_MSG_ID, RESUME_CHECKPOINT_ID

RESUME_JOB_ID=...  RESUME_AFTER_MSG_ID=...  RESUME_CHECKPOINT_ID=... \
  cargo run --example resumability
# replays via `resume`, jumps to next step, finishes.
```

Per-step `idempotency_key` (RFC §6.4) makes the LLM call free on
replay: the runtime returns the prior outcome.

## ARCP primitives

- `workflow.start` — §19.
- `job.progress` + `job.checkpoint` — §10.
- `resume` envelope with `after_message_id` + `checkpoint_id` — §19.
- `subscription.backfill_complete` to mark replay done — §13.3.
- `tool.error` with `code: DATA_LOSS` for retention loss — §18.

## File tour

- `main.rs` — start vs resume; per-step driver.
- `steps.rs` — `run_step` stub for plan/gather/synthesize/critique/finalize.

## Variations

- Persist the checkpoint id to a local file; reattach on the next
  process start without env-var ceremony.
- Add idempotency salting per tenant.
