# Leases

ARCP v1.1 leases describe the authority a job receives when the runtime accepts
work. The Rust SDK supports `cost.budget`, `model.use`, and lease-bound
provisioned credentials.

## `cost.budget`

`cost.budget` is a list of budget amounts such as `["USD:1.00"]`. Tool code can
report spend through `ToolContext::charge`:

```rust
ctx.charge("cost.llm", 0.03, "USD").await?;
```

The runtime decrements the matching currency and emits
`cost.budget.remaining` metrics. Once a counter is exhausted, the helper returns
`ARCPError::BudgetExhausted`, which becomes `job.failed` with
`BUDGET_EXHAUSTED`.

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

Delegation uses subset semantics: a child lease may be equal to or narrower than
its parent. For example, `tier-fast/small` is allowed under `tier-fast/*`, but
`*` is rejected with `LEASE_SUBSET_VIOLATION`.

## Provisioned Credentials

Provisioned credentials move enforcement to an upstream service. When a runtime
is configured with a `CredentialProvisioner`, it advertises
`model_use` and `provisioned_credentials`. For a job with a lease request, the
runtime:

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

## Provisioner Implementations

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

A LiteLLM-style implementation would translate:

- `cost.budget` to the upstream spend cap.
- `model.use` to the upstream allowed model list.
- `expires_at` to the upstream TTL.

Revocation should delete the upstream key. Transient revocation failures are
retried by the runtime, and outstanding ids remain in the ledger until
revocation succeeds.

## Security Checklist

- Do not log credential values.
- Do not emit credential values in telemetry or subscription events.
- Keep provisioner adapters outside core unless they are vendor neutral.
- Reject or narrow delegated leases that exceed the parent envelope.
- Translate upstream budget exhaustion into `BUDGET_EXHAUSTED` at the ARCP
  boundary.
