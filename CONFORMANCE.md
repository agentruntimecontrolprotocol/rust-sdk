# Conformance

Implemented versus deferred protocol surfaces are summarized in **README.md** (Status section). Source modules cite RFC sections in doc comments (e.g. `RFC §8`).

For cross-language conformance tracking, use the monorepo `spec/` tree and shared issue milestones.

## v1.1 Features

| Spec | Surface | Status | Rust SDK location |
| --- | --- | --- | --- |
| §9.6 | `cost.budget` counters and `BUDGET_EXHAUSTED` | Implemented | `src/messages/permissions.rs`, `src/runtime/context.rs`, `tests/cost_budget.rs` |
| §9.7 | `model.use` lease capability and enforcement helper | Implemented | `src/messages/permissions.rs`, `src/runtime/context.rs`, `tests/model_use.rs` |
| §9.8 | Lease-bound provisioned credentials | Implemented | `src/runtime/credentials.rs`, `src/runtime/server.rs`, `tests/provisioned_credentials.rs` |
