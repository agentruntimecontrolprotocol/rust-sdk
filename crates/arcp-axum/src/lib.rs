//! # arcp-axum
//!
//! axum integration for the Agent Runtime Control Protocol (ARCP).
//!
//! ## Status: placeholder
//!
//! This crate is published as a name reservation. A real axum integration
//! — Router, WebSocket upgrade handler, and `ARCPRuntime` extractor — is
//! planned for a future minor release. The current version re-exports
//! [`arcp_core`] so dependents can prepare imports.
//!
//! In the meantime, an end-to-end axum + ARCP server example lives at
//! [`examples/axum_server.rs`][example] in the umbrella crate.
//!
//! [example]: https://github.com/agentruntimecontrolprotocol/rust-sdk/blob/main/crates/arcp/examples/axum_server.rs

#![deny(unsafe_code)]
#![deny(missing_docs)]
#![warn(unreachable_pub)]

pub use arcp_core;
