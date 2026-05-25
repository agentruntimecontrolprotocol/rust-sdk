//! Shared protocol primitives for the Agent Runtime Control Protocol (ARCP).
//!
//! This crate ships the wire-format types and abstractions both client and
//! runtime sides depend on:
//!
//! - [`envelope`] — the canonical envelope (ARCP v1.1 §5).
//! - [`messages`] — payload structs and [`MessageType`] for the §6–§10
//!   surfaces (sessions, jobs, events, leases, delegation).
//! - [`error`] — canonical error taxonomy.
//! - [`ids`] — strongly-typed opaque identifiers (`JobId`, `SessionId`, …).
//! - [`extensions`] — extension namespace registry and classification.
//! - [`transport`] — [`Transport`][transport::Transport] trait + in-memory
//!   transport. `WebSocket` and stdio transports gated behind features.
//! - [`auth`] — [`Authenticator`][auth::Authenticator] trait. Concrete
//!   bearer / `signed_jwt` / none validators live in `arcp-runtime`.
//!
//! Most users should depend on the umbrella `arcp` crate instead of this
//! one directly. Pull in `arcp-core` only when building an alternative
//! client or runtime that needs the protocol primitives without the
//! reference implementations.

#![deny(unsafe_code)]
#![deny(missing_docs)]
#![warn(unreachable_pub)]

pub mod auth;
pub mod envelope;
pub mod error;
pub mod extensions;
pub mod ids;
pub mod messages;
pub mod transport;

pub use envelope::{Envelope, Priority, RawEnvelope};
pub use error::{ARCPError, ErrorCode};
pub use extensions::{ExtensionRegistry, TypeClassification};
pub use messages::{Capabilities, MessageType};

/// Protocol version implemented by this crate, as carried in the `arcp` field
/// of every envelope (ARCP v1.1 §5).
pub const PROTOCOL_VERSION: &str = "1.1";

/// Implementation version of this crate, derived from `Cargo.toml`. Sibling
/// crates in this workspace move in lockstep, so this constant also reflects
/// the runtime / client / umbrella version.
pub const IMPL_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Implementation kind reported in `runtime.kind` / `client.kind` blocks
/// (ARCP v1.1 §6.2 hello/welcome handshake).
pub const IMPL_KIND: &str = "arcp-rs";
