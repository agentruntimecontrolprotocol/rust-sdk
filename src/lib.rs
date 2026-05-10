//! # arcp — Agent Runtime Control Protocol (reference implementation)
//!
//! This crate is a Rust reference implementation of [ARCP v1.0][rfc], the
//! Agent Runtime Control Protocol. The crate is being built in hard-gated
//! phases against `RFC-0001-v2.md`. See [`PLAN.md`][plan] for the build
//! roadmap and `CONFORMANCE.md` for the per-section status.
//!
//! ## Scope
//!
//! v0.1 implements the protocol fundamentals: envelope, sessions and
//! authentication (`bearer`, `signed_jwt`, `none`), capability negotiation,
//! jobs, streams, human-in-the-loop, permissions, leases, subscriptions,
//! artifacts (inline base64 only), the canonical error taxonomy,
//! observability primitives, and the `WebSocket` and stdio transports.
//!
//! Out-of-scope items (`HTTP/2`, `QUIC`, `mTLS`, `OAuth2`, sidecar binary
//! frames, scheduled jobs, multi-agent delegation, workflows, trust
//! elevation, checkpoint-based resume) return `ARCPError::Unimplemented`
//! when invoked.
//!
//! ## Status
//!
//! Phase 1 — envelope, errors, extensions, event log are landed. Later
//! phases populate the runtime, client, transports, and CLI as described
//! in [`PLAN.md`][plan].
//!
//! [rfc]: https://github.com/nficano/arpc/blob/main/agent-runtime-control-protocol/docs/RFC%200001%20%20v2%20%E2%80%94%20Agent%20Runtime%20Control%20Protocol.md
//! [plan]: https://github.com/nficano/arpc/blob/main/rust-sdk/PLAN.md

#![deny(unsafe_code)]
#![deny(missing_docs)]
#![warn(unreachable_pub)]

pub mod auth;
pub mod client;
pub mod envelope;
pub mod error;
pub mod extensions;
pub mod ids;
pub mod messages;
pub mod runtime;
pub mod store;
pub mod transport;

pub use client::ARCPClient;
pub use envelope::{Envelope, Priority, RawEnvelope};
pub use error::{ARCPError, ErrorCode};
pub use extensions::{ExtensionRegistry, TypeClassification};
pub use messages::{Capabilities, MessageType};
pub use runtime::ARCPRuntime;

/// Protocol version implemented by this crate, as carried in the `arcp` field
/// of every envelope (RFC §6.1).
pub const PROTOCOL_VERSION: &str = "1.0";

/// Implementation version of this crate, derived from `Cargo.toml`.
pub const IMPL_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Implementation kind reported in `runtime.kind` / `client.kind` blocks
/// (RFC §8.2, §8.3).
pub const IMPL_KIND: &str = "arcp-rs";
