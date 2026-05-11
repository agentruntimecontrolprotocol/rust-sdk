# permission_challenge

Two agents on two sessions. Generator proposes a patch; reviewer holds
veto via `permission.request`. Same wire contract as a single agent
asking a human.

## Before ARCP

Two-agent code review is glued together with sidecars: generator
writes a PR, CI runs a "reviewer" model, results posted as a comment.
The reviewer can't say "deny: rewrite this hunk and try again" without
manual loop wiring.

## With ARCP

```rust
let lease = request_apply(&generator, ticket_id, &patch).await;
match lease {
    Err(ARCPError::PermissionDenied { detail }) => prior_denial = Some(detail),
    Ok(_)  => return Ok(()),  // applied
    Err(e) => return Err(e.into()),
}
```

Idempotency key per `(ticket, diff)` lets identical patches dedupe at
the runtime — re-proposing the same diff returns the prior verdict.

## ARCP primitives

- `permission.request` with structured `reason` and a tight
  `requested_lease_seconds` — §15.4.
- `permission.grant` / `permission.deny` with typed `code` — §15.4.
- `idempotency_key` for retry-safe re-asks — §6.4.

## File tour

- `main.rs` — generator side proposes; reviewer loop on a second
  session responds.
- `agents.rs` — `propose` + `review` LLM stubs.

## Variations

- Three reviewers, majority-grant.
- A `permission.request` with `min_reviewers: 2` carried in `extensions`.
- Promote the deny into structured feedback the generator parses.
