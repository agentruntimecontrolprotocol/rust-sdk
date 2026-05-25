//! # arcp-otel
//!
//! Name reservation for a planned OpenTelemetry integration. **This crate
//! currently provides no OpenTelemetry types**; it does not depend on
//! `opentelemetry` or `tracing-opentelemetry`. It only re-exports
//! [`arcp_core`] so dependents can prepare imports against a stable crate
//! name.
//!
//! A real OpenTelemetry integration — automatic span emission from ARCP
//! `trace.span` messages, attribute mapping for `arcp.lease.expires_at`
//! and `arcp.budget.remaining` (ARCP v1.1 §11), and a `tracing` bridge —
//! is planned for a future minor release. Follow
//! <https://github.com/agentruntimecontrolprotocol/rust-sdk> for progress.

#![deny(unsafe_code)]
#![deny(missing_docs)]
#![warn(unreachable_pub)]

pub use arcp_core;
