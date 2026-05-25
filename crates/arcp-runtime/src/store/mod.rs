//! Persistent storage primitives.
//!
//! [`eventlog`] is an append-only SQLite-backed log used for transport-level
//! dedup, subscription backfill (ARCP v1.1 §7.6), and resume
//! (ARCP v1.1 §6.3). The artifact store (see [`crate::runtime::artifact`])
//! lives next to the runtime concerns rather than here.

pub mod credentials;
pub mod eventlog;
