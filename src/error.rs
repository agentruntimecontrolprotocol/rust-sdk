//! Canonical error model (RFC §18).
//!
//! Two layered types:
//!
//! - [`ErrorCode`] — the wire-level taxonomy from §18.2 as a
//!   `#[non_exhaustive]` enum. Exists so the runtime, the client, and
//!   external code can pattern-match on a single source of truth.
//! - [`ARCPError`] — the in-process `Result<_, _>` error returned from
//!   library APIs. Each variant maps onto an `ErrorCode` via
//!   [`ARCPError::code`] and carries enough context to reconstruct an
//!   error envelope (§18.1) without a second lookup.
//!
//! The [`ARCPError::retryable`] method follows the RFC §18.3 default
//! taxonomy. Callers MAY override per-call via the returned envelope's
//! `retryable` field, but the in-process default is what `retryable()`
//! reports.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::ids::{IdParseError, LeaseId};

/// Canonical wire-level error code (RFC §18.2).
///
/// `RATE_LIMITED` is an alias for `RESOURCE_EXHAUSTED` per §18.2 and is
/// represented by the same variant; the alias survives only at the
/// deserialise boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
#[allow(clippy::upper_case_acronyms)]
pub enum ErrorCode {
    /// `OK` — not an error; reserved.
    #[serde(rename = "OK")]
    Ok,
    /// `CANCELLED`
    #[serde(rename = "CANCELLED")]
    Cancelled,
    /// `UNKNOWN`
    #[serde(rename = "UNKNOWN")]
    Unknown,
    /// `INVALID_ARGUMENT`
    #[serde(rename = "INVALID_ARGUMENT")]
    InvalidArgument,
    /// `DEADLINE_EXCEEDED`
    #[serde(rename = "DEADLINE_EXCEEDED")]
    DeadlineExceeded,
    /// `NOT_FOUND`
    #[serde(rename = "NOT_FOUND")]
    NotFound,
    /// `ALREADY_EXISTS`
    #[serde(rename = "ALREADY_EXISTS")]
    AlreadyExists,
    /// `PERMISSION_DENIED`
    #[serde(rename = "PERMISSION_DENIED")]
    PermissionDenied,
    /// `RESOURCE_EXHAUSTED` (also serialised from the alias `RATE_LIMITED`).
    #[serde(rename = "RESOURCE_EXHAUSTED", alias = "RATE_LIMITED")]
    ResourceExhausted,
    /// `FAILED_PRECONDITION`
    #[serde(rename = "FAILED_PRECONDITION")]
    FailedPrecondition,
    /// `ABORTED`
    #[serde(rename = "ABORTED")]
    Aborted,
    /// `OUT_OF_RANGE`
    #[serde(rename = "OUT_OF_RANGE")]
    OutOfRange,
    /// `UNIMPLEMENTED`
    #[serde(rename = "UNIMPLEMENTED")]
    Unimplemented,
    /// `INTERNAL`
    #[serde(rename = "INTERNAL")]
    Internal,
    /// `UNAVAILABLE`
    #[serde(rename = "UNAVAILABLE")]
    Unavailable,
    /// `DATA_LOSS`
    #[serde(rename = "DATA_LOSS")]
    DataLoss,
    /// `UNAUTHENTICATED`
    #[serde(rename = "UNAUTHENTICATED")]
    Unauthenticated,
    /// `HEARTBEAT_LOST` (RFC §10.3)
    #[serde(rename = "HEARTBEAT_LOST")]
    HeartbeatLost,
    /// `LEASE_EXPIRED` (RFC §15.5)
    #[serde(rename = "LEASE_EXPIRED")]
    LeaseExpired,
    /// `LEASE_REVOKED` (RFC §15.5)
    #[serde(rename = "LEASE_REVOKED")]
    LeaseRevoked,
    /// `BACKPRESSURE_OVERFLOW`
    #[serde(rename = "BACKPRESSURE_OVERFLOW")]
    BackpressureOverflow,
    /// `BUDGET_EXHAUSTED` (ARCP v1.1 §12; §9.6)
    #[serde(rename = "BUDGET_EXHAUSTED")]
    BudgetExhausted,
    /// `LEASE_SUBSET_VIOLATION` (ARCP v1.1 §9.4)
    #[serde(rename = "LEASE_SUBSET_VIOLATION")]
    LeaseSubsetViolation,
    /// `AGENT_VERSION_NOT_AVAILABLE` (ARCP v1.1 §12; §7.5)
    #[serde(rename = "AGENT_VERSION_NOT_AVAILABLE")]
    AgentVersionNotAvailable,
}

impl ErrorCode {
    /// Default retryability per RFC §18.3.
    ///
    /// Errors flagged retryable here MAY still be rejected by application
    /// policy; conversely, errors flagged non-retryable MAY be retried by
    /// callers who know more than the protocol does. This method reports
    /// only the protocol's default.
    #[must_use]
    pub const fn retryable(self) -> bool {
        matches!(
            self,
            Self::ResourceExhausted
                | Self::Unavailable
                | Self::DeadlineExceeded
                | Self::Internal
                | Self::Aborted
        )
    }

    /// Wire-level string spelling of the code (`"INVALID_ARGUMENT"`, etc.).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "OK",
            Self::Cancelled => "CANCELLED",
            Self::Unknown => "UNKNOWN",
            Self::InvalidArgument => "INVALID_ARGUMENT",
            Self::DeadlineExceeded => "DEADLINE_EXCEEDED",
            Self::NotFound => "NOT_FOUND",
            Self::AlreadyExists => "ALREADY_EXISTS",
            Self::PermissionDenied => "PERMISSION_DENIED",
            Self::ResourceExhausted => "RESOURCE_EXHAUSTED",
            Self::FailedPrecondition => "FAILED_PRECONDITION",
            Self::Aborted => "ABORTED",
            Self::OutOfRange => "OUT_OF_RANGE",
            Self::Unimplemented => "UNIMPLEMENTED",
            Self::Internal => "INTERNAL",
            Self::Unavailable => "UNAVAILABLE",
            Self::DataLoss => "DATA_LOSS",
            Self::Unauthenticated => "UNAUTHENTICATED",
            Self::HeartbeatLost => "HEARTBEAT_LOST",
            Self::LeaseExpired => "LEASE_EXPIRED",
            Self::LeaseRevoked => "LEASE_REVOKED",
            Self::BackpressureOverflow => "BACKPRESSURE_OVERFLOW",
            Self::BudgetExhausted => "BUDGET_EXHAUSTED",
            Self::LeaseSubsetViolation => "LEASE_SUBSET_VIOLATION",
            Self::AgentVersionNotAvailable => "AGENT_VERSION_NOT_AVAILABLE",
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// In-process error type returned from library APIs.
///
/// Maps 1:1 onto the canonical [`ErrorCode`] taxonomy, with extra context on
/// each variant so call sites can build a structured error envelope (§18.1)
/// directly. The variants are `#[non_exhaustive]` so the taxonomy can grow
/// without a breaking change.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
#[allow(clippy::upper_case_acronyms)]
pub enum ARCPError {
    /// Operation was cancelled by the caller, the runtime, or by policy.
    #[error("operation cancelled: {reason}")]
    Cancelled {
        /// Free-form reason for the cancellation.
        reason: String,
    },

    /// Malformed or invalid argument.
    #[error("invalid argument: {detail}")]
    InvalidArgument {
        /// Description of the violated constraint.
        detail: String,
    },

    /// Operation timed out before completion.
    #[error("operation timed out: {detail}")]
    DeadlineExceeded {
        /// Description of what timed out.
        detail: String,
    },

    /// Referenced entity does not exist.
    #[error("not found: {kind} (id={id})")]
    NotFound {
        /// Kind of entity (e.g. `"job"`, `"artifact"`).
        kind: &'static str,
        /// Lookup key as a string.
        id: String,
    },

    /// Entity creation conflicted with an existing entity.
    #[error("already exists: {kind} (id={id})")]
    AlreadyExists {
        /// Kind of entity that conflicted.
        kind: &'static str,
        /// Lookup key as a string.
        id: String,
    },

    /// Caller lacks the required permission or lease.
    #[error("permission denied: {detail}")]
    PermissionDenied {
        /// Description of the missing permission.
        detail: String,
    },

    /// Quota or rate limit hit.
    #[error("resource exhausted: {detail}")]
    ResourceExhausted {
        /// Description of the exhausted resource.
        detail: String,
        /// Floor for the next attempt, if known (§18.3).
        retry_after_seconds: Option<u64>,
    },

    /// Required pre-condition unmet (e.g. job not in cancellable state).
    #[error("failed precondition: {detail}")]
    FailedPrecondition {
        /// Description of the unmet pre-condition.
        detail: String,
    },

    /// Concurrency conflict or hard termination.
    #[error("operation aborted: {detail}")]
    Aborted {
        /// Description of the abort cause.
        detail: String,
    },

    /// Argument outside the valid range.
    #[error("argument out of range: {detail}")]
    OutOfRange {
        /// Description of the range violation.
        detail: String,
    },

    /// Feature not supported by this runtime.
    #[error("not implemented (RFC §{section}): {detail}")]
    Unimplemented {
        /// RFC section reference (e.g. `"10.6"`).
        section: &'static str,
        /// Description of the missing surface.
        detail: String,
    },

    /// Internal runtime error. Should be rare and indicate a bug.
    #[error("internal error: {detail}")]
    Internal {
        /// Description of the internal failure.
        detail: String,
    },

    /// Transient unavailability; retry MAY succeed.
    #[error("service unavailable: {detail}")]
    Unavailable {
        /// Description of the unavailable subsystem.
        detail: String,
    },

    /// Unrecoverable data loss or corruption (e.g. retention expired).
    #[error("data loss: {detail}")]
    DataLoss {
        /// Description of what was lost.
        detail: String,
    },

    /// Missing or invalid credentials.
    #[error("unauthenticated: {detail}")]
    Unauthenticated {
        /// Description of the auth failure.
        detail: String,
    },

    /// Job missed required heartbeats (RFC §10.3).
    #[error("heartbeat lost: missed_count={missed_count}")]
    HeartbeatLost {
        /// How many consecutive heartbeats were missed.
        missed_count: u32,
    },

    /// Operation attempted with an expired lease (RFC §15.5).
    #[error("lease expired: lease_id={lease_id}")]
    LeaseExpired {
        /// The expired lease.
        lease_id: LeaseId,
    },

    /// Operation attempted with a revoked lease (RFC §15.5).
    #[error("lease revoked: lease_id={lease_id} (reason={reason})")]
    LeaseRevoked {
        /// The revoked lease.
        lease_id: LeaseId,
        /// Reason supplied by the grantor.
        reason: String,
    },

    /// Subscription or stream dropped due to backpressure overflow.
    #[error("backpressure overflow: {detail}")]
    BackpressureOverflow {
        /// Description of the overflowing channel.
        detail: String,
    },

    /// A `cost.budget` capability counter reached its maximum (ARCP v1.1 §9.6).
    #[error("budget exhausted: {detail}")]
    BudgetExhausted {
        /// Description of the exhausted budget counter.
        detail: String,
    },

    /// A delegated or child lease attempted to exceed its parent envelope.
    #[error("lease subset violation: {detail}")]
    LeaseSubsetViolation {
        /// Description of the violated lease axis.
        detail: String,
    },

    /// `job.submit` named an `agent@version` the runtime does not have (ARCP v1.1 §7.5).
    #[error("agent version not available: {agent}@{version}")]
    AgentVersionNotAvailable {
        /// Agent name.
        agent: String,
        /// Requested version.
        version: String,
    },

    /// Unknown error. Avoid in favour of a specific code.
    #[error("unknown error: {detail}")]
    Unknown {
        /// Description of the failure.
        detail: String,
    },

    /// JSON serialisation / deserialisation failure at the wire boundary.
    #[error("serialisation error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Persistent storage failure (event log, artifact store).
    #[error("storage error: {0}")]
    Storage(#[from] rusqlite::Error),

    /// JWT decode / verify failure during authentication.
    #[error("auth token error: {0}")]
    Token(#[from] jsonwebtoken::errors::Error),

    /// Identifier failed to parse on a wire boundary.
    #[error("id parse error: {0}")]
    Id(#[from] IdParseError),
}

impl ARCPError {
    /// Map this in-process error to its canonical [`ErrorCode`].
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self {
            Self::Cancelled { .. } => ErrorCode::Cancelled,
            Self::InvalidArgument { .. } | Self::Id(_) => ErrorCode::InvalidArgument,
            Self::DeadlineExceeded { .. } => ErrorCode::DeadlineExceeded,
            Self::NotFound { .. } => ErrorCode::NotFound,
            Self::AlreadyExists { .. } => ErrorCode::AlreadyExists,
            Self::PermissionDenied { .. } => ErrorCode::PermissionDenied,
            Self::ResourceExhausted { .. } => ErrorCode::ResourceExhausted,
            Self::FailedPrecondition { .. } => ErrorCode::FailedPrecondition,
            Self::Aborted { .. } => ErrorCode::Aborted,
            Self::OutOfRange { .. } => ErrorCode::OutOfRange,
            Self::Unimplemented { .. } => ErrorCode::Unimplemented,
            Self::Internal { .. } | Self::Storage(_) => ErrorCode::Internal,
            Self::Unavailable { .. } => ErrorCode::Unavailable,
            Self::DataLoss { .. } => ErrorCode::DataLoss,
            Self::Unauthenticated { .. } | Self::Token(_) => ErrorCode::Unauthenticated,
            Self::HeartbeatLost { .. } => ErrorCode::HeartbeatLost,
            Self::LeaseExpired { .. } => ErrorCode::LeaseExpired,
            Self::LeaseRevoked { .. } => ErrorCode::LeaseRevoked,
            Self::BackpressureOverflow { .. } => ErrorCode::BackpressureOverflow,
            Self::BudgetExhausted { .. } => ErrorCode::BudgetExhausted,
            Self::LeaseSubsetViolation { .. } => ErrorCode::LeaseSubsetViolation,
            Self::AgentVersionNotAvailable { .. } => ErrorCode::AgentVersionNotAvailable,
            Self::Unknown { .. } | Self::Serialization(_) => ErrorCode::Unknown,
        }
    }

    /// Convenience: return the §18.3 default retryability for this error.
    #[must_use]
    pub const fn retryable(&self) -> bool {
        self.code().retryable()
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
    use super::*;

    #[test]
    fn error_code_round_trips_through_serde() {
        for code in [
            ErrorCode::Ok,
            ErrorCode::Cancelled,
            ErrorCode::InvalidArgument,
            ErrorCode::DeadlineExceeded,
            ErrorCode::NotFound,
            ErrorCode::AlreadyExists,
            ErrorCode::PermissionDenied,
            ErrorCode::ResourceExhausted,
            ErrorCode::FailedPrecondition,
            ErrorCode::Aborted,
            ErrorCode::OutOfRange,
            ErrorCode::Unimplemented,
            ErrorCode::Internal,
            ErrorCode::Unavailable,
            ErrorCode::DataLoss,
            ErrorCode::Unauthenticated,
            ErrorCode::HeartbeatLost,
            ErrorCode::LeaseExpired,
            ErrorCode::LeaseRevoked,
            ErrorCode::BackpressureOverflow,
            ErrorCode::BudgetExhausted,
            ErrorCode::LeaseSubsetViolation,
            ErrorCode::AgentVersionNotAvailable,
            ErrorCode::Unknown,
        ] {
            let s = serde_json::to_string(&code).expect("serialize");
            let back: ErrorCode = serde_json::from_str(&s).expect("deserialize");
            assert_eq!(code, back, "round-trip for {code}");
            assert_eq!(s.trim_matches('"'), code.as_str());
        }
    }

    #[test]
    fn rate_limited_alias_decodes_to_resource_exhausted() {
        let code: ErrorCode = serde_json::from_str("\"RATE_LIMITED\"").expect("alias");
        assert_eq!(code, ErrorCode::ResourceExhausted);
    }

    #[test]
    fn retryability_matches_rfc_18_3() {
        // Retryable by default
        for c in [
            ErrorCode::ResourceExhausted,
            ErrorCode::Unavailable,
            ErrorCode::DeadlineExceeded,
            ErrorCode::Internal,
            ErrorCode::Aborted,
        ] {
            assert!(c.retryable(), "{c} should be retryable");
        }
        // Non-retryable by default
        for c in [
            ErrorCode::InvalidArgument,
            ErrorCode::NotFound,
            ErrorCode::AlreadyExists,
            ErrorCode::PermissionDenied,
            ErrorCode::FailedPrecondition,
            ErrorCode::Unimplemented,
            ErrorCode::Unauthenticated,
            ErrorCode::DataLoss,
            ErrorCode::LeaseSubsetViolation,
        ] {
            assert!(!c.retryable(), "{c} should NOT be retryable");
        }
    }

    #[test]
    fn arcp_error_maps_to_canonical_code() {
        let err = ARCPError::PermissionDenied {
            detail: "missing lease".into(),
        };
        assert_eq!(err.code(), ErrorCode::PermissionDenied);
        assert!(!err.retryable());
    }

    #[test]
    fn id_parse_error_propagates_via_from() {
        let parse_err: IdParseError = "junk".parse::<crate::ids::SessionId>().unwrap_err();
        let err: ARCPError = parse_err.into();
        assert_eq!(err.code(), ErrorCode::InvalidArgument);
    }

    #[test]
    fn v1_1_error_codes_serialize_to_wire_strings() {
        assert_eq!(ErrorCode::BudgetExhausted.as_str(), "BUDGET_EXHAUSTED");
        assert_eq!(ErrorCode::LeaseExpired.as_str(), "LEASE_EXPIRED");
        assert_eq!(
            ErrorCode::LeaseSubsetViolation.as_str(),
            "LEASE_SUBSET_VIOLATION"
        );
        assert_eq!(
            ErrorCode::AgentVersionNotAvailable.as_str(),
            "AGENT_VERSION_NOT_AVAILABLE"
        );
        assert_eq!(
            serde_json::to_string(&ErrorCode::BudgetExhausted).expect("serialize"),
            "\"BUDGET_EXHAUSTED\""
        );
        assert_eq!(
            serde_json::to_string(&ErrorCode::AgentVersionNotAvailable).expect("serialize"),
            "\"AGENT_VERSION_NOT_AVAILABLE\""
        );
        let budget: ErrorCode =
            serde_json::from_str("\"BUDGET_EXHAUSTED\"").expect("deserialize budget");
        assert_eq!(budget, ErrorCode::BudgetExhausted);
        let subset: ErrorCode =
            serde_json::from_str("\"LEASE_SUBSET_VIOLATION\"").expect("deserialize subset");
        assert_eq!(subset, ErrorCode::LeaseSubsetViolation);
        let agent_ver: ErrorCode = serde_json::from_str("\"AGENT_VERSION_NOT_AVAILABLE\"")
            .expect("deserialize agent version");
        assert_eq!(agent_ver, ErrorCode::AgentVersionNotAvailable);
    }

    #[test]
    fn v1_1_arcp_errors_map_to_canonical_codes() {
        let budget = ARCPError::BudgetExhausted {
            detail: "cost.budget USD counter <= 0".into(),
        };
        assert_eq!(budget.code(), ErrorCode::BudgetExhausted);
        assert!(!budget.retryable());

        let subset = ARCPError::LeaseSubsetViolation {
            detail: "model.use widened".into(),
        };
        assert_eq!(subset.code(), ErrorCode::LeaseSubsetViolation);
        assert!(!subset.retryable());

        let agent_ver = ARCPError::AgentVersionNotAvailable {
            agent: "summarizer".into(),
            version: "2.3.0".into(),
        };
        assert_eq!(agent_ver.code(), ErrorCode::AgentVersionNotAvailable);
        assert!(!agent_ver.retryable());
    }

    #[test]
    fn serde_error_propagates_via_from() {
        let parse: Result<serde_json::Value, _> = serde_json::from_str("not-json");
        let err: ARCPError = parse.unwrap_err().into();
        assert_eq!(err.code(), ErrorCode::Unknown);
    }
}
