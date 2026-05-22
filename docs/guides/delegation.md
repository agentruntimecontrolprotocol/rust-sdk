# Delegation (§10)

Delegation lets an agent spawn child work while preserving lease and trace
boundaries.

Spec reference: [§10](../../../spec/docs/draft-arcp-1.1.md#10-delegation).

## Model

The parent emits a delegate event or uses the runtime helper path. The runtime
validates the requested child job, checks that the child lease is a subset of
the parent effective lease, and creates a first-class child job.

## Lease subset

The child lease cannot introduce a new capability or widen an existing target.
Failures surface as `LEASE_SUBSET_VIOLATION`.

## Trace inheritance

Child jobs inherit trace context where available. This lets logs, metrics, and
custom tracing middleware reconstruct the parent/child relationship.

## Cancellation

The ARCP core treats parent and child jobs as separate jobs. If application
semantics require cascading cancellation, parent agent code should track child
ids and cancel or compensate explicitly.

## Examples

- [`examples/delegation/`](../../examples/delegation/)
- [`examples/handoff/`](../../examples/handoff/)
