//! Persistent storage primitives.
//!
//! Phase 1 ships [`eventlog`], an append-only SQLite-backed log used for
//! transport-level idempotency (RFC §6.4), subscription backfill (§13.3),
//! and resume (§19). Later phases add the artifact store (§16) and any
//! additional persisted state.

pub mod eventlog;
