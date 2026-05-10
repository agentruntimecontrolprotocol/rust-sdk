//! Canonical message envelope (RFC §6.1).
//!
//! Every wire-level ARCP message is an [`Envelope`]. The protocol metadata
//! lives at the envelope level; the type-specific body lives in
//! [`crate::messages::MessageType`], embedded with `#[serde(flatten)]` so
//! that on the wire `type` and `payload` appear at the envelope level
//! alongside `id`, `timestamp`, etc., per the canonical wire format.
//!
//! Two layered representations exist:
//!
//! - [`Envelope`] — typed; `payload` is a [`MessageType`].
//! - [`RawEnvelope`] — untyped; `payload` is a `serde_json::Value` and
//!   `type` is a free-form string. The transport boundary uses this when
//!   it needs to inspect the message before committing to a type (e.g.
//!   to apply RFC §21.3 unknown-message handling without a deserialise
//!   error).
//!
//! The two are interconvertible via [`Envelope::into_raw`] /
//! [`RawEnvelope::try_into_typed`].

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{
    IdempotencyKey, JobId, MessageId, SessionId, SpanId, StreamId, SubscriptionId, TraceId,
};
use crate::messages::MessageType;
use crate::PROTOCOL_VERSION;

/// Message priority class (RFC §6.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    /// Low priority; first to be shed under backpressure.
    Low,
    /// Default priority.
    #[default]
    Normal,
    /// Above-default priority.
    High,
    /// Reserved for messages that must not be deferred (RFC §6.5).
    Critical,
}

/// Typed protocol envelope (RFC §6.1).
///
/// `payload` is `#[serde(flatten)]`-embedded so the wire form is flat:
///
/// ```json
/// {
///   "arcp": "1.0", "id": "msg_...", "timestamp": "...",
///   "type": "ping", "payload": {}
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Envelope {
    /// Protocol version understood by the sender.
    pub arcp: String,

    /// Globally unique message id; transport-level idempotency key
    /// (RFC §6.4).
    pub id: MessageId,

    /// Sender timestamp in RFC 3339 format.
    pub timestamp: DateTime<Utc>,

    /// Logical sender id (e.g. client / runtime / agent name).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,

    /// Logical recipient id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,

    /// Required once a session exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<SessionId>,

    /// Required for durable job events.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_id: Option<JobId>,

    /// Required for stream events.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_id: Option<StreamId>,

    /// Required for subscription delivery.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subscription_id: Option<SubscriptionId>,

    /// Stable id for one user-visible request or workflow (recommended).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<TraceId>,

    /// Span id for the current operation (recommended).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_id: Option<SpanId>,

    /// Parent span id when this message is part of a trace tree.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_span_id: Option<SpanId>,

    /// Id of the command or request this message answers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<MessageId>,

    /// Id of the message that directly caused this message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub causation_id: Option<MessageId>,

    /// Logical idempotency key for the command intent (RFC §6.4).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<IdempotencyKey>,

    /// Message priority. Default `normal`.
    #[serde(default, skip_serializing_if = "is_default_priority")]
    pub priority: Priority,

    /// Object of namespaced extension fields (RFC §21).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extensions: Option<serde_json::Value>,

    /// Type-specific body. Flattened so `type` and `payload` appear at the
    /// envelope level on the wire.
    #[serde(flatten)]
    pub payload: MessageType,
}

// Signature dictated by serde's `skip_serializing_if`, which requires
// `fn(&T) -> bool`; we cannot change it to take by value.
#[allow(clippy::trivially_copy_pass_by_ref)]
const fn is_default_priority(p: &Priority) -> bool {
    matches!(p, Priority::Normal)
}

impl Envelope {
    /// Construct a new envelope with the supplied payload, a freshly
    /// generated [`MessageId`], the current UTC timestamp, the crate's
    /// [`PROTOCOL_VERSION`], and `priority = normal`.
    #[must_use]
    pub fn new(payload: MessageType) -> Self {
        Self {
            arcp: PROTOCOL_VERSION.to_owned(),
            id: MessageId::new(),
            timestamp: Utc::now(),
            source: None,
            target: None,
            session_id: None,
            job_id: None,
            stream_id: None,
            subscription_id: None,
            trace_id: None,
            span_id: None,
            parent_span_id: None,
            correlation_id: None,
            causation_id: None,
            idempotency_key: None,
            priority: Priority::default(),
            extensions: None,
            payload,
        }
    }

    /// Convert to the untyped wire shape suitable for inspection / logging
    /// without re-deserialising.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ARCPError::Serialization`] if the envelope
    /// cannot be serialised (this should not happen for any value
    /// constructed by this crate).
    pub fn into_raw(self) -> Result<RawEnvelope, crate::error::ARCPError> {
        let value = serde_json::to_value(&self)?;
        Ok(serde_json::from_value(value)?)
    }
}

/// Untyped envelope used at the transport boundary.
///
/// Carries a free-form `type` and an opaque `payload`, plus the same
/// metadata fields as [`Envelope`]. Lets the transport apply RFC §21.3
/// unknown-message handling without triggering a deserialise error on
/// the typed enum.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawEnvelope {
    /// Protocol version understood by the sender.
    pub arcp: String,
    /// Message id.
    pub id: MessageId,
    /// Sender timestamp.
    pub timestamp: DateTime<Utc>,
    /// Free-form type discriminator.
    #[serde(rename = "type")]
    pub type_name: String,
    /// Opaque type-specific body.
    #[serde(default)]
    pub payload: serde_json::Value,

    /// Logical sender id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Logical recipient id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// Session id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<SessionId>,
    /// Job id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_id: Option<JobId>,
    /// Stream id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_id: Option<StreamId>,
    /// Subscription id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subscription_id: Option<SubscriptionId>,
    /// Trace id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<TraceId>,
    /// Span id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_id: Option<SpanId>,
    /// Parent span id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_span_id: Option<SpanId>,
    /// Correlation id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<MessageId>,
    /// Causation id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub causation_id: Option<MessageId>,
    /// Idempotency key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<IdempotencyKey>,
    /// Priority.
    #[serde(default, skip_serializing_if = "is_default_priority")]
    pub priority: Priority,
    /// Extensions object.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extensions: Option<serde_json::Value>,
}

impl RawEnvelope {
    /// Attempt to upgrade to a typed [`Envelope`]. The caller is responsible
    /// for first checking the `type_name` against the extension registry
    /// (see [`crate::extensions`]) so unknown types can be handled per
    /// §21.3 instead of failing the deserialise.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ARCPError::Serialization`] if the inner
    /// payload doesn't match the typed schema for this `type_name`.
    pub fn try_into_typed(self) -> Result<Envelope, crate::error::ARCPError> {
        let value = serde_json::to_value(&self)?;
        Ok(serde_json::from_value(value)?)
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_panics_doc
)]
mod tests {
    use chrono::TimeZone;

    use super::*;
    use crate::ids::{MessageId, SessionId};
    use crate::messages::{LogLevel, LogPayload, PingPayload};

    fn fixed_timestamp() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 5, 7, 21, 30, 0).unwrap()
    }

    #[test]
    fn envelope_round_trips_through_serde() {
        let env = Envelope::new(MessageType::Ping(PingPayload {
            nonce: Some("n".into()),
        }));
        let json = serde_json::to_string(&env).expect("serialize");
        let back: Envelope = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(env, back);
    }

    #[test]
    fn envelope_wire_format_is_flat() {
        let mut env = Envelope::new(MessageType::Ping(PingPayload { nonce: None }));
        env.id = "msg_01JABC0123456789ABCDEFGHJK".parse().expect("valid id");
        env.timestamp = fixed_timestamp();
        let value = serde_json::to_value(&env).expect("serialize");
        // Both `type` and `payload` are envelope-level, not nested.
        assert_eq!(value["type"], "ping");
        assert!(value.get("payload").is_some());
        assert_eq!(value["arcp"], "1.0");
        assert_eq!(value["id"], "msg_01JABC0123456789ABCDEFGHJK");
    }

    #[test]
    fn envelope_with_optional_fields_round_trips() {
        let mut env = Envelope::new(MessageType::Log(LogPayload {
            level: LogLevel::Warn,
            message: "retrying".into(),
            attributes: None,
        }));
        env.session_id = Some(SessionId::new());
        env.trace_id = Some(TraceId::new("trace_789").expect("non-empty"));
        env.correlation_id = Some(MessageId::new());
        env.priority = Priority::High;
        let json = serde_json::to_string(&env).expect("serialize");
        let back: Envelope = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(env, back);
    }

    #[test]
    fn priority_default_is_omitted_on_serialize() {
        let env = Envelope::new(MessageType::Ping(PingPayload::default()));
        let value = serde_json::to_value(&env).expect("serialize");
        assert!(
            value.get("priority").is_none(),
            "default priority must be elided"
        );
    }

    #[test]
    fn priority_round_trips_each_variant() {
        for p in [
            Priority::Low,
            Priority::Normal,
            Priority::High,
            Priority::Critical,
        ] {
            let s = serde_json::to_string(&p).expect("serialize");
            let back: Priority = serde_json::from_str(&s).expect("deserialize");
            assert_eq!(p, back);
        }
    }

    #[test]
    fn raw_envelope_round_trips_to_typed() {
        let env = Envelope::new(MessageType::Ping(PingPayload::default()));
        let raw = env.clone().into_raw().expect("to raw");
        assert_eq!(raw.type_name, "ping");
        let back = raw.try_into_typed().expect("to typed");
        assert_eq!(env, back);
    }

    #[test]
    fn raw_envelope_unknown_type_does_not_fail_decode() {
        // Demonstrates the value of the raw layer: an unknown wire type
        // can still be parsed at the raw level for §21.3 dispatch.
        let wire = serde_json::json!({
            "arcp": "1.0",
            "id": "msg_01JABC0123456789ABCDEFGHJK",
            "timestamp": "2026-05-07T21:30:00Z",
            "type": "arcpx.example.v1",
            "payload": {"hello": "world"},
        });
        let raw: RawEnvelope = serde_json::from_value(wire).expect("raw parse");
        assert_eq!(raw.type_name, "arcpx.example.v1");
        assert_eq!(raw.payload["hello"], "world");
        // The typed upgrade should fail, since arcpx.* is not in MessageType.
        assert!(raw.try_into_typed().is_err());
    }
}
