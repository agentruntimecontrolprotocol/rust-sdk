//! # arcp-actix-web
//!
//! actix-web integration for the Agent Runtime Control Protocol (ARCP).
//!
//! ## Status: placeholder
//!
//! This crate is published as a name reservation. A real actix-web
//! integration — handler factory, WebSocket upgrade, and `ARCPRuntime`
//! adapter — is planned for a future minor release. The current version
//! re-exports [`arcp_core`] so dependents can prepare imports.
//!
//! Follow <https://github.com/agentruntimecontrolprotocol/rust-sdk> for
//! progress.

#![deny(unsafe_code)]
#![deny(missing_docs)]
#![warn(unreachable_pub)]

pub use arcp_core;
