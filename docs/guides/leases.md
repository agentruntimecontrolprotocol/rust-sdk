# Leases (§9)

ARCP v1.1 leases describe the authority a job receives when the runtime accepts
work. The Rust SDK supports lease requests, subset checks, expiry, budgets,
model-use constraints, and lease-bound provisioned credentials.

Spec reference: [§9](../../../spec/docs/draft-arcp-1.1.md#9-leases).

## `cost.budget`

`cost.budget` is a list of budget amounts such as `["USD:1.00"]`. Tool code can
report spend through `ToolContext::charge`:

```rust
ctx.charge("cost.llm", 0.03, "USD").await?;
```

The runtime decrements the matching currency and emits
`cost.budget.remaining` metrics. Once a counter is exhausted, the helper returns
`BUDGET_EXHAUSTED`.

See [`examples/cost_budget/`](../../examples/cost_budget/).

## `model.use`

`model.use` is a list of model glob patterns. The supported wildcard is `*`.
All other characters match literally.

```json
{
  "model_use": ["anthropic/claude-3-haiku-*", "tier-fast/*"]
}
```

Tool handlers should call `ToolContext::enforce_model_use(model)` before an LLM
or gateway call when the runtime is in the path. A mismatch returns
`PERMISSION_DENIED`.

See [`tests/model_use.rs`](../../tests/model_use.rs) for integration coverage.

## Subset validation

Delegation uses subset semantics: a child lease may be equal to or narrower than
its parent. For example, `tier-fast/small` is allowed under `tier-fast/*`, but
`*` is rejected with `LEASE_SUBSET_VIOLATION`.

The effective parent lease is the lease accepted by the runtime, not merely the
lease the parent requested.

## Expiration

Lease constraints may include expiration. Once expired, lease-gated operations
return `LEASE_EXPIRED`.

See [`examples/lease_expires_at.rs`](../../examples/lease_expires_at.rs).

## Provisioned credentials

Provisioned credentials move enforcement to an upstream service. When a runtime
is configured with a `CredentialProvisioner`, it advertises `model_use` and
`provisioned_credentials`. For a job with a lease request, the runtime:

1. Finalizes the lease.
2. Calls `CredentialProvisioner::issue`.
3. Sends `job.accepted` with `payload.credentials`.
4. Revokes outstanding credentials when the job completes, fails, or is
   cancelled.

The wire credential shape is:

```json
{
  "id": "cred_0000000000000001",
  "scheme": "bearer",
  "value": "secret",
  "endpoint": "https://gateway.example",
  "profile": "fast",
  "constraints": {
    "model_use": ["tier-fast/*"],
    "cost_budget": ["USD:1"]
  }
}
```

The SDK treats `value` as secret: it is redacted from `Debug`, omitted from
subscription fanout, and not included in job inventory responses.

See [`examples/provisioned_credentials/`](../../examples/provisioned_credentials/).

## Provisioner implementations

Core defines only the vendor-neutral trait:

```rust
#[async_trait::async_trait]
pub trait CredentialProvisioner: Send + Sync {
    async fn issue(
        &self,
        lease: &LeaseRequest,
        ctx: &CredentialJobContext,
    ) -> Result<Vec<ProvisionedCredential>, ARCPError>;

    async fn revoke(&self, id: &CredentialId) -> Result<(), ARCPError>;
}
```

A gateway implementation should translate ARCP constraints to upstream spend
caps, allowed model lists, and TTLs. Revocation should delete the upstream key.

## Security checklist

- Do not log credential values.
- Do not emit credential values in telemetry or subscription events.
- Keep provisioner adapters outside core unless they are vendor-neutral.
- Reject or narrow delegated leases that exceed the parent envelope.
- Translate upstream budget exhaustion into `BUDGET_EXHAUSTED` at the ARCP boundary.
