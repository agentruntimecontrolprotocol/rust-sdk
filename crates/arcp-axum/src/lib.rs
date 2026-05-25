//! # arcp-axum
//!
//! Name reservation for a planned axum integration. **This crate currently
//! provides no axum types**; it does not depend on `axum`. It only
//! re-exports [`arcp_core`] so dependents can prepare imports against a
//! stable crate name.
//!
//! A real axum integration — Router, WebSocket upgrade handler, and
//! `ARCPRuntime` extractor — is planned for a future minor release. Until
//! then, see [`crates/arcp/examples/axum_server.rs`][example] in the
//! umbrella crate for a hand-rolled axum-plus-ARCP pattern that uses
//! `axum` directly rather than anything in this crate.
//!
//! [example]: https://github.com/agentruntimecontrolprotocol/rust-sdk/blob/main/crates/arcp/examples/axum_server.rs

#![deny(unsafe_code)]
#![deny(missing_docs)]
#![warn(unreachable_pub)]

pub use arcp_core;
