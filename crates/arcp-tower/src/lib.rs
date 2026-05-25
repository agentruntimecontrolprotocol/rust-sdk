//! # arcp-tower
//!
//! Name reservation for a planned Tower integration. **This crate currently
//! provides no `tower::Service` or `tower::Layer` types**; it does not even
//! depend on `tower`. It only re-exports [`arcp_core`] so dependents can
//! prepare imports against a stable crate name.
//!
//! A real `tower::Service` integration that bridges HTTP-style transports
//! into the ARCP runtime is planned for a future minor release. Follow
//! <https://github.com/agentruntimecontrolprotocol/rust-sdk> for progress.

#![deny(unsafe_code)]
#![deny(missing_docs)]
#![warn(unreachable_pub)]

pub use arcp_core;
