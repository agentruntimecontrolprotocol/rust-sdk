# Vendor extensions (§15)

ARCP reserves the core protocol surface; everything else lives in the
extension namespace advertised via `Capabilities::extensions`. The SDK
admits two extension naming forms:

- `arcpx.<vendor-or-domain>[.<name>].v<n>` — recommended for new extensions
  (matches the RFC's own `arcpx.example.v1` capability example).
- `<reverse-dns>.<...>.v<n>` — e.g. `com.acme.workflow.v2`.

The bare `x-` prefix is **reserved for transport-internal experimental
fields** and MUST NOT be used in long-lived deployments; receivers MAY drop
`x-` envelopes silently.

Spec reference: [§15](../../../spec/docs/draft-arcp-1.1.md#15-iana-considerations).

## Extensible surfaces

- Envelope `type` (custom message types)
- Lease capability namespace
- Envelope `extensions` object keys
- Auth schemes implemented by custom authenticators

## Naming

Examples:

```text
arcpx.acme.cancel.v1
com.example.confidence.v1
arcpx.opentelemetry.tracecontext.v1
```

The SDK classifies a wire-level `type` string via `ExtensionRegistry::classify`
into one of: `Core`, `KnownExtension` (registered/advertised),
`UnknownExtension` (well-formed but not advertised), `ReservedExperimental`
(`x-...`), or `Malformed`. Use `Capabilities::extensions` on `session.open` /
`session.accepted` to advertise which extensions a peer supports.

## Round-trip behavior

Unknown vendor extension values should not be silently rewritten or collapsed
into core protocol state. Consumers that understand an extension can register
their own handlers; consumers that do not understand it can ignore it while
preserving the envelope shape.

## Custom leases

Vendor lease capabilities are opaque to the core matcher. If your runtime owns
a custom operation, validate that operation in the handler before touching the
upstream system.

## Example

See [`examples/vendor_extensions.rs`](../../examples/vendor_extensions.rs).
