# Observability (§11)

ARCP carries observability data through envelope metadata and job events.

Spec reference: [§11](../../../spec/docs/draft-arcp-1.1.md#11-trace-propagation).

## Trace ids

`Envelope` includes optional `trace_id`. Runtimes should copy trace ids from
submit envelopes to job events, terminal outcomes, and delegated children.

## Logs and metrics

Reserved job event kinds include `log` and `metric`. Use these for structured
agent telemetry that should travel with the job stream.

## Budgets as metrics

`ToolContext::charge` emits `cost.budget.remaining` metrics as budget counters
decrease. Consumers can subscribe to those events for live spend dashboards.

## OpenTelemetry

The crate carries the protocol data needed for OpenTelemetry integration, but
does not ship a native OTel middleware package. Applications can wrap
transports or runtime handlers and propagate W3C trace context through
`Envelope::extensions` under a registered extension key (e.g.
`arcpx.opentelemetry.tracecontext.v1`).

## Examples

See [`examples/tracing.rs`](../../examples/tracing.rs) for trace-id usage.
