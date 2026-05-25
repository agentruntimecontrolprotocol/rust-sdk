//! Reference client (consumer side) for the Agent Runtime Control Protocol.
//!
//! Ships [`ARCPClient`] and the type-state [`Session`] for opening,
//! authenticating, and driving a session over any
//! [`Transport`][arcp_core::transport::Transport]. Job and subscription
//! handles hang off `Session<Authenticated>`; raw streams are surfaced via
//! subscription events rather than a dedicated stream handle.
//!
//! Wire-format types are re-exported from `arcp-core` for ergonomics; most
//! users should pull in the umbrella `arcp` crate which bundles client +
//! runtime + core.

#![deny(unsafe_code)]
#![deny(missing_docs)]
#![warn(unreachable_pub)]

pub mod api;
pub mod handlers;

pub use api::{ARCPClient, Authenticated, JobHandle, Session, SubscriptionHandle, Unauthenticated};

// Re-export the protocol primitives consumers will routinely reach for.
pub use arcp_core::{ARCPError, Envelope, ErrorCode, MessageType, PROTOCOL_VERSION};
