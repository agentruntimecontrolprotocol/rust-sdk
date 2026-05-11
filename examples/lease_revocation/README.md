# lease_revocation

Warehouse DB admin agent. Bootstrap reads pre-grant a lease per table;
every write triggers a `permission.request` the operator must approve.
A mid-flight `lease.revoked` invalidates the local cache so the next
call re-prompts.

## Before ARCP

Read-only DB roles are easy. Write roles end up as either
"developer-fullaccess" (terrifying) or "ticket-driven" (slow). Mid-job
revocation is unheard of: revoking a credential mid-flight breaks the
running query.

## With ARCP

```rust
authorize(&client, "SELECT count(*) FROM public.orders WHERE ...", &mut leases).await?;
authorize(&client, "UPDATE public.orders SET status='refunded' WHERE id=4812", &mut leases).await?;
```

Bootstrap leases live for an hour; writes are 5 minutes. The drain
loop wires `lease.revoked` straight into the cache.

## ARCP primitives

- `permission.request` with `resource: "table:..."` — §15.4.
- `permission.grant` carrying `expires_at` + `lease_id` — §15.5.
- `lease.revoked` mid-flight, processed via a background drain — §15.5.
- `requested_lease_seconds` distinct per op class.

## File tour

- `main.rs` — bootstrap pre-grants + the per-statement authorize loop.
- `sql.rs` — sqlparser-equivalent classifier (read / write / ddl).

## Variations

- Add a `lease.extended` handler to refresh long-running queries.
- Swap the cache for a Redis-backed shared map across replicas.
- Reject writes outright on production schemas; only permit them in dev.
