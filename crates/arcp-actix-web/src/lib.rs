//! # arcp-actix-web
//!
//! Name reservation for a planned actix-web integration. **This crate
//! currently provides no actix types**; it does not depend on `actix-web`.
//! It only re-exports [`arcp_core`] so dependents can prepare imports
//! against a stable crate name.
//!
//! A real actix-web integration — handler factory, WebSocket upgrade, and
//! `ARCPRuntime` adapter — is planned for a future minor release. Follow
//! <https://github.com/agentruntimecontrolprotocol/rust-sdk> for progress.

#![deny(unsafe_code)]
#![deny(missing_docs)]
#![warn(unreachable_pub)]

pub use arcp_core;
