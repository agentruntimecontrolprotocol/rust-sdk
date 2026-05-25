//! Newtype wrappers for the protocol's identifier fields (RFC §6.1.1).
//!
//! Identifiers are categorised into two flavours:
//!
//! - **Prefixed ULIDs** for runtime-minted identifiers. The on-the-wire form
//!   is `<prefix>_<ULID>`; the prefix is asserted on parse. Mixing a
//!   `SessionId` with a `MessageId` is a compile error.
//! - **Free-form opaque strings** for ids whose format is determined by the
//!   environment: `TraceId` and `SpanId` (per OpenTelemetry / Datadog /
//!   Honeycomb conventions, §17.1) and `IdempotencyKey` (client-supplied
//!   logical intent key, §6.4).

use std::fmt;
use std::str::FromStr;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use ulid::Ulid;

/// Errors produced when parsing a typed ID from a string.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IdParseError {
    /// The input did not start with the expected `<prefix>_` prefix.
    #[error("expected prefix `{expected}_` for {kind}, got `{got}`")]
    WrongPrefix {
        /// Identifier name (e.g. `"SessionId"`).
        kind: &'static str,
        /// Expected prefix without the trailing underscore.
        expected: &'static str,
        /// The full string that was offered.
        got: String,
    },
    /// The string was empty or had no body after the prefix.
    #[error("empty id body for {kind}")]
    Empty {
        /// Identifier name.
        kind: &'static str,
    },
}

macro_rules! prefixed_id {
    ($name:ident, $prefix:literal, $doc:literal) => {
        #[doc = $doc]
        ///
        /// On-the-wire form is
        #[doc = concat!("`", $prefix, "_<ULID>`")]
        /// where the ULID provides monotonic, sortable uniqueness.
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(String);

        impl $name {
            /// Mint a fresh id with a freshly generated ULID body.
            #[must_use]
            pub fn new() -> Self {
                Self(format!("{}_{}", $prefix, Ulid::new()))
            }

            /// Borrow the underlying string representation.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }

            /// The prefix used by this id type (without the trailing
            /// underscore).
            #[must_use]
            pub const fn prefix() -> &'static str {
                $prefix
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl FromStr for $name {
            type Err = IdParseError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                let with_underscore = concat!($prefix, "_");
                let Some(rest) = s.strip_prefix(with_underscore) else {
                    return Err(IdParseError::WrongPrefix {
                        kind: stringify!($name),
                        expected: $prefix,
                        got: s.to_owned(),
                    });
                };
                if rest.is_empty() {
                    return Err(IdParseError::Empty {
                        kind: stringify!($name),
                    });
                }
                Ok(Self(s.to_owned()))
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(&self.0)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let raw = String::deserialize(deserializer)?;
                raw.parse().map_err(D::Error::custom)
            }
        }
    };
}

macro_rules! freeform_id {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(String);

        impl $name {
            /// Construct from any non-empty string.
            ///
            /// # Errors
            ///
            /// Returns [`IdParseError::Empty`] if `value` is empty.
            pub fn new(value: impl Into<String>) -> Result<Self, IdParseError> {
                let s = value.into();
                if s.is_empty() {
                    Err(IdParseError::Empty {
                        kind: stringify!($name),
                    })
                } else {
                    Ok(Self(s))
                }
            }

            /// Borrow the underlying string representation.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl FromStr for $name {
            type Err = IdParseError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Self::new(s)
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(&self.0)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let raw = String::deserialize(deserializer)?;
                raw.parse().map_err(D::Error::custom)
            }
        }
    };
}

prefixed_id!(
    SessionId,
    "sess",
    "Identifier for an ARCP session (RFC §9)."
);
prefixed_id!(
    MessageId,
    "msg",
    "Globally unique envelope identifier (RFC §6.1.1)."
);
prefixed_id!(JobId, "job", "Identifier for a durable job (RFC §10).");
prefixed_id!(StreamId, "str", "Identifier for a stream (RFC §11).");
prefixed_id!(
    SubscriptionId,
    "sub",
    "Identifier for an observer subscription (RFC §13)."
);
prefixed_id!(
    LeaseId,
    "lease",
    "Identifier for a permission lease (RFC §15.5)."
);
prefixed_id!(
    ArtifactId,
    "art",
    "Identifier for an addressable artifact (RFC §16)."
);

freeform_id!(
    TraceId,
    "Distributed-trace identifier (RFC §17.1). Format is environment-defined."
);
freeform_id!(
    SpanId,
    "Span identifier within a trace (RFC §17.1). Format is environment-defined."
);
freeform_id!(
    IdempotencyKey,
    "Logical idempotency key supplied by the client for a command intent (RFC §6.4)."
);

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_panics_doc
)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn prefixed_id_round_trips_through_string() {
        let id = SessionId::new();
        let s = id.to_string();
        assert!(s.starts_with("sess_"), "got {s}");
        let parsed: SessionId = s.parse().expect("round-trip");
        assert_eq!(id, parsed);
    }

    #[test]
    fn prefixed_id_rejects_wrong_prefix() {
        let err = "msg_01ABC".parse::<SessionId>().expect_err("must reject");
        match err {
            IdParseError::WrongPrefix { expected, .. } => assert_eq!(expected, "sess"),
            IdParseError::Empty { .. } => panic!("expected WrongPrefix, got Empty"),
        }
    }

    #[test]
    fn prefixed_id_rejects_empty_body() {
        let err = "sess_"
            .parse::<SessionId>()
            .expect_err("must reject empty body");
        assert!(matches!(err, IdParseError::Empty { .. }));
    }

    #[test]
    fn prefixed_id_serde_round_trip() {
        let id = MessageId::new();
        let json = serde_json::to_string(&id).expect("serialize");
        assert!(json.starts_with("\"msg_"));
        let back: MessageId = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(id, back);
    }

    #[test]
    fn prefixed_id_serde_rejects_wrong_prefix() {
        let json = "\"sess_01ABC\"";
        let err = serde_json::from_str::<JobId>(json).expect_err("must fail");
        assert!(err.to_string().contains("expected prefix"));
    }

    #[test]
    fn freeform_id_accepts_arbitrary_strings() {
        let key = IdempotencyKey::new("refund-ord_4812").expect("non-empty");
        assert_eq!(key.as_str(), "refund-ord_4812");
        let s = serde_json::to_string(&key).expect("serialize");
        let back: IdempotencyKey = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(key, back);
    }

    #[test]
    fn freeform_id_rejects_empty() {
        let err = IdempotencyKey::new("").expect_err("must reject empty");
        assert!(matches!(err, IdParseError::Empty { .. }));
    }

    #[test]
    fn id_types_are_compile_time_distinct() {
        // SessionId and MessageId are distinct types — this tests that
        // the type system enforces distinctness. We can't *actually* mix
        // them at the call site without a compile error. But we can show
        // that distinct ids produced by both types have distinct prefixes.
        let s = SessionId::new().to_string();
        let m = MessageId::new().to_string();
        assert_ne!(s, m);
        assert!(s.starts_with("sess_"));
        assert!(m.starts_with("msg_"));
    }

    #[test]
    fn ids_are_hashable() {
        let mut set = HashSet::new();
        set.insert(JobId::new());
        set.insert(JobId::new());
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn all_prefixes_are_unique() {
        let prefixes = [
            SessionId::prefix(),
            MessageId::prefix(),
            JobId::prefix(),
            StreamId::prefix(),
            SubscriptionId::prefix(),
            LeaseId::prefix(),
            ArtifactId::prefix(),
        ];
        let unique: HashSet<&&str> = prefixes.iter().collect();
        assert_eq!(unique.len(), prefixes.len(), "id prefixes must not collide");
    }
}
