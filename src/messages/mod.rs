//! Wire-level message payload types (RFC §6.2).
//!
//! Phase 1 ships a thin skeleton that covers a handful of variants — just
//! enough to round-trip envelopes and to drive the snapshot tests in
//! `envelope.rs`. Phase 2 fills in every variant under `session.rs`,
//! `control.rs`, `execution.rs`, `streaming.rs`, `human.rs`,
//! `permissions.rs`, `subscriptions.rs`, `artifacts.rs`, and
//! `telemetry.rs`.
//!
//! [`MessageType`] is a tagged enum (`#[serde(tag = "type", content =
//! "payload")]`) so on the wire a message renders flat:
//!
//! ```json
//! { "type": "ping", "payload": { "nonce": "..." } }
//! ```
//!
//! When this is `#[serde(flatten)]`-embedded into [`crate::envelope::Envelope`]
//! the `type` and `payload` keys appear at the envelope level alongside the
//! other metadata fields, matching the canonical wire format from §6.1.

use serde::{Deserialize, Serialize};

/// Tagged enum of every protocol message payload (RFC §6.2).
///
/// Phase 1 covers `ping`, `pong`, `event.emit`, `log`, and `metric` to
/// exercise the envelope round-trip. Other variants land in Phase 2.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
#[non_exhaustive]
pub enum MessageType {
    /// `ping` — control liveness probe.
    #[serde(rename = "ping")]
    Ping(PingPayload),
    /// `pong` — response to `ping`.
    #[serde(rename = "pong")]
    Pong(PongPayload),
    /// `event.emit` — generic structured event (carrier for synthetic
    /// envelopes such as `subscription.backfill_complete`).
    #[serde(rename = "event.emit")]
    EventEmit(EventEmitPayload),
    /// `log` — structured log line (RFC §17.2).
    #[serde(rename = "log")]
    Log(LogPayload),
    /// `metric` — telemetry sample (RFC §17.3).
    #[serde(rename = "metric")]
    Metric(MetricPayload),
}

impl MessageType {
    /// Wire-level discriminator string for this variant (`"ping"`, etc.).
    #[must_use]
    pub const fn type_name(&self) -> &'static str {
        match self {
            Self::Ping(_) => "ping",
            Self::Pong(_) => "pong",
            Self::EventEmit(_) => "event.emit",
            Self::Log(_) => "log",
            Self::Metric(_) => "metric",
        }
    }
}

/// Payload for a `ping` message. Optional opaque nonce echoed back in `pong`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PingPayload {
    /// Optional nonce; if present, the corresponding `pong` echoes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nonce: Option<String>,
}

/// Payload for a `pong` message.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PongPayload {
    /// Echoed nonce, if the `ping` carried one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nonce: Option<String>,
}

/// Payload for an `event.emit` message.
///
/// `event.emit` is a generic carrier; the meaning of each event lives in
/// `name` (e.g. `"subscription.backfill_complete"`) and any additional
/// data is opaque JSON in `data`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventEmitPayload {
    /// Event name (a dotted, namespaced identifier).
    pub name: String,
    /// Opaque event data; schema is determined by `name`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// Severity level for [`LogPayload`] (RFC §17.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    /// `trace` — most verbose.
    Trace,
    /// `debug`
    Debug,
    /// `info`
    Info,
    /// `warn`
    Warn,
    /// `error`
    Error,
    /// `critical` — most severe.
    Critical,
}

/// Payload for a `log` message (RFC §17.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogPayload {
    /// Severity level.
    pub level: LogLevel,
    /// Free-form human-readable message.
    pub message: String,
    /// Optional structured attributes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attributes: Option<serde_json::Value>,
}

/// Payload for a `metric` message (RFC §17.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricPayload {
    /// Metric name. Standard names are listed in
    /// [`standard_names`].
    pub name: String,
    /// Numeric value.
    pub value: f64,
    /// Unit of measure.
    pub unit: String,
    /// Optional dimensions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dims: Option<serde_json::Value>,
}

/// Reserved standard metric names (RFC §17.3.1).
///
/// Runtimes producing these concepts MUST use these names with the indicated
/// units. Non-standard variants MUST be namespaced.
pub mod standard_names {
    /// `tokens.used` (unit: `tokens`).
    pub const TOKENS_USED: &str = "tokens.used";
    /// `cost.usd` (unit: `usd`).
    pub const COST_USD: &str = "cost.usd";
    /// `gpu.seconds` (unit: `seconds`).
    pub const GPU_SECONDS: &str = "gpu.seconds";
    /// `tool.invocations` (unit: `count`).
    pub const TOOL_INVOCATIONS: &str = "tool.invocations";
    /// `latency.ms` (unit: `ms`).
    pub const LATENCY_MS: &str = "latency.ms";
    /// `bytes.in` (unit: `bytes`).
    pub const BYTES_IN: &str = "bytes.in";
    /// `bytes.out` (unit: `bytes`).
    pub const BYTES_OUT: &str = "bytes.out";
    /// `errors.total` (unit: `count`).
    pub const ERRORS_TOTAL: &str = "errors.total";
}

/// Synthetic event name emitted by the runtime at the boundary between
/// subscription backfill and live tail (RFC §13.3).
pub const SUBSCRIPTION_BACKFILL_COMPLETE: &str = "subscription.backfill_complete";

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_panics_doc
)]
mod tests {
    use super::*;

    #[test]
    fn ping_round_trips_through_serde() {
        let m = MessageType::Ping(PingPayload {
            nonce: Some("abc".into()),
        });
        let json = serde_json::to_string(&m).expect("serialize");
        let back: MessageType = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(m, back);
    }

    #[test]
    fn ping_wire_shape_matches_rfc() {
        let m = MessageType::Ping(PingPayload { nonce: None });
        let json = serde_json::to_value(&m).expect("serialize");
        assert_eq!(json, serde_json::json!({"type": "ping", "payload": {}}),);
    }

    #[test]
    fn metric_with_standard_name_round_trips() {
        let m = MessageType::Metric(MetricPayload {
            name: standard_names::TOKENS_USED.into(),
            value: 1432.0,
            unit: "tokens".into(),
            dims: Some(serde_json::json!({"model": "claude-3.5", "kind": "input"})),
        });
        let json = serde_json::to_string(&m).expect("serialize");
        let back: MessageType = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(m, back);
    }

    #[test]
    fn type_name_matches_wire_discriminator() {
        let cases = [
            (MessageType::Ping(PingPayload::default()), "ping"),
            (MessageType::Pong(PongPayload::default()), "pong"),
            (
                MessageType::EventEmit(EventEmitPayload {
                    name: "x".into(),
                    data: None,
                }),
                "event.emit",
            ),
        ];
        for (m, expected) in cases {
            assert_eq!(m.type_name(), expected);
        }
    }

    #[test]
    fn unknown_type_fails_deserialize() {
        let bad = "{\"type\":\"never.heard.of.it\",\"payload\":{}}";
        let result: Result<MessageType, _> = serde_json::from_str(bad);
        assert!(result.is_err());
    }
}
