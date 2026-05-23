# Contributing to arcp

Thanks for your interest in improving the Rust SDK for ARCP. This
document covers how to report issues, propose changes, and get a change merged.

By participating you agree to the [Code of Conduct](CODE_OF_CONDUCT.md).

## Where changes belong

ARCP is two things in two places, and a change belongs to exactly one of them:

- **The protocol** — the wire format, message semantics, lease rules, error
  taxonomy, feature flags. These live in the
  [specification repository](https://github.com/agentruntimecontrolprotocol/spec).
  If your idea changes what goes *on the wire* or what a conformant runtime must
  do, it is a spec change — open it there, not here. This SDK implements the
  spec; it does not define it.
- **This SDK** — how the protocol is expressed idiomatically in Rust:
  bugs, ergonomics, performance, missing-but-specified features, docs, tests.
  Those belong here.

When in doubt, open an issue here and we'll redirect if it's really a protocol
question.

## The golden rule: conform, don't extend

A change to this SDK must keep it a faithful client of
[ARCP v1.1 (draft)](https://github.com/agentruntimecontrolprotocol/spec/blob/main/docs/draft-arcp-1.1.md).
Concretely:

- **Don't invent wire behavior.** No envelope fields, event kinds, error codes,
  or feature flags that the spec doesn't define. If you need one, it's a spec
  proposal first.
- **Negotiate honestly.** Only advertise a feature flag in `session.hello` once
  the SDK actually implements it. The feature matrix in the README must match
  what the code negotiates — a row marked `Supported` is a promise.
- **Respect the semantics.** Sequence numbers stay gap-free and monotonic;
  `LEASE_EXPIRED` and `BUDGET_EXHAUSTED` stay non-retryable; the effective
  feature set is the intersection of client and runtime advertisements. Tests
  must not paper over a semantic the spec requires.
- **Stay layered.** This SDK controls runtimes. It does not expose tools (that's
  MCP) or export telemetry (that's OpenTelemetry). PRs that blur those layers
  will be asked to move the logic out.

## Reporting bugs

Open an issue with: the SDK version and Rust version, the runtime you
connected to, a minimal reproduction (the smallest program that triggers it),
what you expected, and what happened. A failing test is the best possible bug
report. Wire-level traces (the envelopes exchanged) help enormously for protocol
behavior — redact any `auth.token` or provisioned-credential `value` first.

## Proposing a change

For anything beyond a small fix, open an issue describing the problem before
writing code, so we can agree on the approach. Small, focused PRs review faster
than large ones; if a change is big, say so early and we'll help break it down.

## Development setup

The crate targets the toolchain pinned in [`rust-toolchain.toml`](rust-toolchain.toml)
(`stable` with `rustfmt`, `clippy`, and `rust-src`) and an MSRV of `1.88` as
declared in [`Cargo.toml`](Cargo.toml). Install Rust via
[rustup](https://rustup.rs/); `cargo` and the pinned components are then
provisioned automatically on first build.

```sh
git clone https://github.com/agentruntimecontrolprotocol/rust-sdk.git
cd rust-sdk
cargo build --all-targets --all-features
```

## Tests and conformance

Two layers must pass before a PR merges:

- **Unit tests** — this SDK's own suite:

  ```sh
  cargo test --all-features
  ```

- **Conformance** — the SDK's behavior against the reference runtime. New
  protocol-facing code (session negotiation, event sequencing, lease handling,
  error mapping) needs a test that exercises the real exchange, not a mock that
  assumes the answer. Integration tests under [`tests/`](tests/) drive the
  in-process runtime end to end; to point them at an out-of-process runtime,
  start one with `cargo run -- serve --bearer secret-token --principal alice@example.com`
  and connect the WebSocket transport at `ws://127.0.0.1:8765`. See
  [`CONFORMANCE.md`](CONFORMANCE.md) for the section-by-section coverage matrix.

CI runs both on every PR. A PR that changes which feature flags the SDK
negotiates must also update the README feature matrix in the same change.

## Coding standards

Format, lint, and verify with the same commands CI runs:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo check --all-targets --all-features
```

Match the surrounding code. Public API changes need doc comments and an entry in
the changelog. Prefer clarity over cleverness in a library others build on.

## Commit and pull-request conventions

- Write focused commits with present-tense, imperative subjects
  (`add result_chunk reassembly`, not `added` / `adds`).
- Reference the issue a PR closes (`Closes #123`).
- Keep the PR description honest about scope and any spec sections touched.
- Rebase on the default branch and ensure CI is green before requesting review.
- Sign off your commits to certify the [Developer Certificate of Origin](https://developercertificate.org/):

  ```sh
  git commit -s -m "your message"
  ```

## Releases

Releases are cut by maintainers. The crate is published to
[crates.io](https://crates.io/crates/arcp) by a maintainer running
`cargo publish` after tagging the release commit; `cargo publish --dry-run` is
part of the pre-release checklist. The SDK is versioned with semantic
versioning independently of the protocol version it speaks; a protocol version
bump is noted in the changelog when the negotiated ARCP version changes.

## License

By contributing, you agree that your contributions are licensed under the
project's [Apache-2.0](LICENSE-APACHE) license.
