# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

The Rust SDK is a Cargo workspace; the version recorded here tracks
`workspace.package.version` in [`Cargo.toml`](Cargo.toml) and applies to the
co-released `arcp`, `arcp-core`, `arcp-client`, and `arcp-runtime` crates.
The middleware reservation stubs (`arcp-tower`, `arcp-axum`,
`arcp-actix-web`, `arcp-otel`) are versioned independently at
`0.1.0-alpha.0`.

## [2.0.0] - 2026-05

### Changed

- Restructured the single `arcp` crate into a Cargo workspace ahead of 2.0
  publish: the umbrella `arcp` crate now re-exports `arcp-core` (wire types,
  IDs, transports, error taxonomy), `arcp-client` (`ARCPClient` + type-state
  `Session`), and `arcp-runtime` (`ARCPRuntime`, SQLite event log, JWT/bearer
  auth, `arcp` CLI). Direct consumers can pull individual crates to slim
  dependencies.

### Added

- Reservation stubs for forthcoming middleware: `arcp-tower`, `arcp-axum`,
  `arcp-actix-web`, `arcp-otel`. These crates publish at `0.1.0-alpha.0`
  and currently re-export `arcp-core` only.

## [0.1.0] - 2026-05-10

### Added

- Initial reference SDK release aligned with ARCP protocol v1.1 (see README status).

