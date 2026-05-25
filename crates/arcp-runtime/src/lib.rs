//! Reference runtime (server side) for the Agent Runtime Control Protocol.
//!
//! This crate ships the production-ready runtime — the part that accepts
//! sessions, dispatches messages, runs tools, manages jobs / streams /
//! permissions / leases / subscriptions, and persists what needs persisting:
//!
//! - [`runtime`] — [`ARCPRuntime`][runtime::ARCPRuntime] entrypoint.
//! - [`store`] — SQLite-backed append-only event log + credential ledger.
//! - [`auth`] — bearer, `signed_jwt`, and none [`Authenticator`][arcp_core::auth::Authenticator]
//!   implementations. The trait itself lives in `arcp-core`.
//!
//! Wire-format types ([`Envelope`][arcp_core::Envelope], [`MessageType`][arcp_core::MessageType],
//! errors, IDs, transport trait) are re-exported here for convenience but
//! ultimately live in `arcp-core`. Most users should depend on the umbrella
//! `arcp` crate; pull `arcp-runtime` in directly if you don't need the
//! client side.
//!
//! The crate also ships an `arcp` binary that runs a configurable demo
//! runtime over stdio or websockets — handy for ad-hoc conformance testing.

#![deny(unsafe_code)]
#![deny(missing_docs)]
#![warn(unreachable_pub)]

pub mod auth;
pub mod runtime;
pub mod store;

pub use runtime::ARCPRuntime;

// Re-export the protocol primitives users will routinely reach for.
pub use arcp_core::{
    ARCPError, Envelope, ErrorCode, MessageType, IMPL_KIND, IMPL_VERSION, PROTOCOL_VERSION,
};
