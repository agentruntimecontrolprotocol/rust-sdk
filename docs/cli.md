# CLI

The crate ships an `arcp` binary for local runtime hosting and version checks.

## Version

```sh
cargo run -- version
```

Prints the crate implementation version, the ARCP wire version, and the Rust
implementation kind.

## Serve

```sh
cargo run -- serve --bind 127.0.0.1:7777
```

By default, `serve` listens over WebSocket and advertises anonymous auth. Add a
bearer token to require authentication:

```sh
cargo run -- serve \
  --bind 127.0.0.1:7777 \
  --bearer secret-token \
  --principal alice@example.com
```

Current `serve` support is intentionally small: one WebSocket listener, built-in
anonymous or static bearer auth, and default runtime capabilities for streaming
and artifacts. Rich host integrations live in application code and examples.

## Tail

`tail` is present as a placeholder for future event-log inspection:

```sh
cargo run -- tail --session ses_...
```

The current implementation reports that subscription/event-log tailing is not
yet wired into the CLI.

## Logging

Use `-v`, `-vv`, or `-vvv` for progressively more verbose tracing output. The
`RUST_LOG` environment variable overrides the default filter.
