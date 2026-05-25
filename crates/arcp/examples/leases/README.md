# leases

A sandboxed on-call agent. Reads use a coarse 30-minute lease per host;
writes use a 60-second lease scoped to one binary + one target. The
lease is the only guard.

## Before ARCP

Either the agent has shell or it doesn't. Granting it shell means
trusting it not to `rm -rf /`; refusing it means a human has to relay
every `journalctl` query. The middle ground — "this binary, against
this host, for the next minute" — has no shape in HTTP/RPC frameworks.

## With ARCP

```rust
let lease = acquire_lease(&client, &classify(argv, host)?, "OOM triage").await?;
// run subprocess under lease
```

Permissions are typed, scoped to a resource, and time-boxed. Read-vs-write
is a knob, not a binary.

## ARCP primitives

- `permission.request` / `permission.grant` / `permission.deny` — §15.4.
- `requested_lease_seconds` and the resource path convention — §15.5.
- `stream.open(kind: "thought")` + `stream.chunk` for the agent's
  reasoning — §11.4.

## File tour

- `main.rs` — classify → acquire → run; reasoning streamed alongside.
- `agent.rs` — one-shot `next_step` LLM stub.

## Variations

- Make the read lease per-namespace instead of per-host.
- Add a `host.exec.dry_run` permission for the agent to plan first.
- Swap the lease duration with whatever your incident timer enforces.
