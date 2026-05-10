//! Snapshot tests for canonical envelope wire forms.
//!
//! These pin the on-the-wire serialisation of envelopes to a known shape so
//! any future serde change shows up as a loud diff under `cargo insta
//! review`. The fixtures here are derived from the RFC §6.1 / §17.2 / §17.3
//! examples; per-message-type snapshots will land alongside the full
//! [`MessageType`] population in Phase 2.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_panics_doc
)]

use arcp::envelope::{Envelope, Priority};
use arcp::ids::{MessageId, SessionId, TraceId};
use arcp::messages::{LogLevel, LogPayload, MessageType, MetricPayload, PingPayload};
use chrono::TimeZone;

fn canonical_id() -> MessageId {
    "msg_01JABC0123456789ABCDEFGHJK".parse().expect("valid id")
}

fn canonical_session() -> SessionId {
    "sess_01JABC0123456789ABCDEFGHJK".parse().expect("valid id")
}

fn canonical_timestamp() -> chrono::DateTime<chrono::Utc> {
    chrono::Utc
        .with_ymd_and_hms(2026, 5, 7, 21, 30, 0)
        .single()
        .expect("valid timestamp")
}

#[test]
fn snapshot_minimal_ping_envelope() {
    let mut env = Envelope::new(MessageType::Ping(PingPayload::default()));
    env.id = canonical_id();
    env.timestamp = canonical_timestamp();
    let value = serde_json::to_value(&env).expect("serialize");
    insta::assert_json_snapshot!(value);
}

#[test]
fn snapshot_log_envelope_with_session_and_trace() {
    let mut env = Envelope::new(MessageType::Log(LogPayload {
        level: LogLevel::Warn,
        message: "Retrying tool invocation".into(),
        attributes: Some(serde_json::json!({"attempt": 2, "tool": "filesystem.search"})),
    }));
    env.id = canonical_id();
    env.timestamp = canonical_timestamp();
    env.session_id = Some(canonical_session());
    env.trace_id = Some(TraceId::new("trace_789").expect("non-empty"));
    let value = serde_json::to_value(&env).expect("serialize");
    insta::assert_json_snapshot!(value);
}

#[test]
fn snapshot_metric_envelope_with_priority_and_idempotency_key() {
    use arcp::ids::IdempotencyKey;
    use arcp::messages::standard_names;

    let mut env = Envelope::new(MessageType::Metric(MetricPayload {
        name: standard_names::TOKENS_USED.into(),
        value: 1432.0,
        unit: "tokens".into(),
        dims: Some(serde_json::json!({"model": "claude-3.5", "kind": "input"})),
    }));
    env.id = canonical_id();
    env.timestamp = canonical_timestamp();
    env.session_id = Some(canonical_session());
    env.priority = Priority::High;
    env.idempotency_key = Some(IdempotencyKey::new("refund-ord_4812").expect("non-empty"));
    let value = serde_json::to_value(&env).expect("serialize");
    insta::assert_json_snapshot!(value);
}
