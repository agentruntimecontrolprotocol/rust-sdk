# Troubleshooting

Common failure modes and how to fix them.

## `UNAUTHENTICATED` during handshake

Causes:

- The client used the wrong credential scheme.
- Bearer token does not match the runtime authenticator.
- A signed JWT has the wrong secret, audience, issuer, or validity window.
- Anonymous auth was not advertised by the runtime.

Fix: verify the authenticator directly in a unit test, then check the
`Credentials` value sent by the client.

## `PERMISSION_DENIED` from a tool

Cause: the effective lease does not cover the requested target, model, or
capability.

Fix: narrow the tool call to the accepted lease or request a broader lease at
job submission time. For model gateways, call `ToolContext::enforce_model_use`
before the upstream call so the failure is explicit.

## `LEASE_SUBSET_VIOLATION`

Cause: a child or delegated job requested a lease broader than the parent
job's effective lease.

Fix: use the accepted parent lease as the upper bound and ensure every child
capability is equal to or narrower than that bound.

## `BUDGET_EXHAUSTED`

Cause: a `ToolContext::charge` call depleted a matching `cost.budget` counter.

Fix: request a larger budget, lower tool spend, or stop work after the first
exhaustion error. The runtime emits remaining-budget metrics while counters are
available.

## Resume returns no events

Causes:

- `last_event_seq` is already at or beyond the latest stored event.
- The runtime uses an in-memory event log and restarted.
- The original session expired or was swept.

Fix: persist the event log for restart-tolerant resume and store the highest
processed `event_seq` durably on the client side.

## Stdio transport breaks

Cause: the child process wrote non-envelope bytes to stdout.

Fix: route logs and diagnostics to stderr. Stdout must contain only
newline-delimited ARCP JSON frames.

## WebSocket examples cannot bind

Cause: another process is listening on the requested address.

Fix: pass another address with `--bind`, for example:

```sh
cargo run -- serve --bind 127.0.0.1:7788
```

## Build fails after disabling default features

Causes:

- Code imports `transport::websocket` without enabling `transport-ws`.
- Code imports stdio transport helpers without enabling `transport-stdio`.
- The CLI `serve` command needs `transport-ws`.

Fix: enable the needed feature or gate your own imports with matching
`cfg(feature = "...")` attributes.

## Publish dry-run fails on stale generated files

Run the standard gate from the repository root. The workspace has multiple
publishable crates, so `cargo publish --dry-run` needs to be iterated in
dependency order (downstream crates resolve internal deps to versions
already on crates.io, so the dry-run only succeeds incrementally):

```sh
cargo fmt --all -- --check
cargo test --workspace --all-features
for crate in arcp-core arcp-client arcp-runtime arcp arcp-tower arcp-axum arcp-actix-web arcp-otel; do
    cargo publish --dry-run -p "$crate"
done
```

Then inspect the packaged file list per crate. Each crate's `Cargo.toml`
controls its include set; docs under `docs/` are repository docs and are
not part of any crate package unless that crate's include list is expanded.
