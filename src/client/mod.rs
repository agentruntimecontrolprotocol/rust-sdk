//! ARCP client side.
//!
//! Phase 2 ships the type-state [`Session`] and a thin [`ARCPClient`] for
//! opening sessions over a [`Transport`][crate::transport::Transport]. Job
//! and stream APIs land on `Session<Authenticated>` in Phase 3.

pub mod api;
pub mod handlers;

pub use api::{ARCPClient, Authenticated, Session, Unauthenticated};
pub use handlers::{HumanInputHandler, NoopHumanInputHandler};
