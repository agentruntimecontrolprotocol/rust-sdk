# Vendor extensions (§15)

ARCP reserves the core protocol surface and provides one extension namespace:
`x-vendor.<vendor>.<name>`.

Spec reference: [§15](../../../spec/docs/draft-arcp-1.1.md#15-iana-considerations).

## Extensible surfaces

- Envelope `type`
- Job event `kind`
- Lease capability namespace
- Envelope `extensions` object keys
- Auth schemes implemented by custom authenticators

## Naming

Examples:

```text
x-vendor.acme.cancel
x-vendor.com.example.confidence
x-vendor.opentelemetry.tracecontext
```

The SDK classifies core, advertised vendor, unadvertised vendor, experimental,
and malformed names through `ExtensionRegistry`.

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
