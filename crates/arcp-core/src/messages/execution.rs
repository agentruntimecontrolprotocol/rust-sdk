//! Execution messages — tools, jobs, agents, workflows (RFC §10).

use serde::{Deserialize, Serialize};

use crate::error::ErrorCode;
use crate::ids::JobId;

/// Parsed agent identifier per ARCP v1.1 §7.5.
///
/// Grammar (§7.5):
///
/// ```text
/// agent   ::= name | name "@" version
/// name    ::= [a-z0-9][a-z0-9._-]*
/// version ::= [a-zA-Z0-9.+_-]+
/// ```
///
/// `version` is `None` for a bare-name reference. Bare names resolve to
/// the runtime's advertised `default` for that agent (see
/// `Capabilities::agents`); pinned versions match exactly and surface
/// [`crate::error::ErrorCode::AgentVersionNotAvailable`] if missing.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AgentRef {
    /// Bare agent name (without the `@version` suffix).
    pub name: String,
    /// Optional pinned version. `None` means "resolve to default".
    pub version: Option<String>,
}

/// Error returned by [`AgentRef::parse`] for inputs that violate the
/// §7.5 grammar.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AgentRefParseError {
    /// The bare-name component is empty or does not match
    /// `[a-z0-9][a-z0-9._-]*`.
    #[error("invalid agent name {0:?}")]
    InvalidName(String),
    /// The `version` component does not match `[a-zA-Z0-9.+_-]+`.
    #[error("invalid agent version {0:?}")]
    InvalidVersion(String),
}

const fn is_name_head(c: char) -> bool {
    matches!(c, 'a'..='z' | '0'..='9')
}

const fn is_name_tail(c: char) -> bool {
    matches!(c, 'a'..='z' | '0'..='9' | '.' | '_' | '-')
}

const fn is_version_char(c: char) -> bool {
    matches!(c, 'a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '+' | '_' | '-')
}

fn validate_name(name: &str) -> Result<(), AgentRefParseError> {
    let mut chars = name.chars();
    let Some(head) = chars.next() else {
        return Err(AgentRefParseError::InvalidName(name.to_owned()));
    };
    if !is_name_head(head) {
        return Err(AgentRefParseError::InvalidName(name.to_owned()));
    }
    for c in chars {
        if !is_name_tail(c) {
            return Err(AgentRefParseError::InvalidName(name.to_owned()));
        }
    }
    Ok(())
}

fn validate_version(version: &str) -> Result<(), AgentRefParseError> {
    if version.is_empty() {
        return Err(AgentRefParseError::InvalidVersion(version.to_owned()));
    }
    for c in version.chars() {
        if !is_version_char(c) {
            return Err(AgentRefParseError::InvalidVersion(version.to_owned()));
        }
    }
    Ok(())
}

impl AgentRef {
    /// Parse an `agent` identifier per ARCP v1.1 §7.5.
    ///
    /// # Errors
    ///
    /// Returns [`AgentRefParseError`] when either the bare name or the
    /// version component violates its grammar.
    pub fn parse(input: &str) -> Result<Self, AgentRefParseError> {
        if let Some(at) = input.find('@') {
            let (name, rest) = input.split_at(at);
            // `rest` includes the `@`; skip it.
            let version = &rest[1..];
            validate_name(name)?;
            validate_version(version)?;
            Ok(Self {
                name: name.to_owned(),
                version: Some(version.to_owned()),
            })
        } else {
            validate_name(input)?;
            Ok(Self {
                name: input.to_owned(),
                version: None,
            })
        }
    }

    /// Format back to the wire `name` or `name@version` string.
    #[must_use]
    pub fn format(&self) -> String {
        self.version.as_ref().map_or_else(
            || self.name.clone(),
            |v| format!("{name}@{v}", name = self.name),
        )
    }
}

impl std::fmt::Display for AgentRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.format())
    }
}

impl std::str::FromStr for AgentRef {
    type Err = AgentRefParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl Serialize for AgentRef {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.format())
    }
}

impl<'de> Deserialize<'de> for AgentRef {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let raw = String::deserialize(deserializer)?;
        Self::parse(&raw).map_err(D::Error::custom)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod agent_ref_tests {
    use super::*;

    #[test]
    fn parse_bare_name() {
        let r = AgentRef::parse("code-refactor").unwrap();
        assert_eq!(r.name, "code-refactor");
        assert!(r.version.is_none());
    }

    #[test]
    fn parse_name_at_version() {
        let r = AgentRef::parse("code-refactor@2.0.0").unwrap();
        assert_eq!(r.name, "code-refactor");
        assert_eq!(r.version.as_deref(), Some("2.0.0"));
    }

    #[test]
    fn format_round_trips() {
        for s in ["a", "a-b", "a@1.0.0", "agent_x@v1.2.3+build.4"] {
            let r = AgentRef::parse(s).unwrap();
            assert_eq!(r.format(), s);
        }
    }

    #[test]
    fn rejects_uppercase_in_name() {
        assert!(AgentRef::parse("CodeRefactor").is_err());
        assert!(AgentRef::parse("Foo@1").is_err());
    }

    #[test]
    fn rejects_empty_version() {
        assert!(AgentRef::parse("ok@").is_err());
    }

    #[test]
    fn serde_round_trip() {
        let r = AgentRef::parse("web-research@1.0.0").unwrap();
        let json = serde_json::to_string(&r).unwrap();
        assert_eq!(json, "\"web-research@1.0.0\"");
        let back: AgentRef = serde_json::from_str(&json).unwrap();
        assert_eq!(back, r);
    }
}

/// Payload for `tool.invoke`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ToolInvokePayload {
    /// Tool identifier.
    pub tool: String,
    /// Tool-specific arguments.
    pub arguments: serde_json::Value,
    /// `cost.budget` lease capability for this job (ARCP v1.1 §9.6).
    /// When present, the runtime tracks per-currency counters and
    /// surfaces `BUDGET_EXHAUSTED` to the agent once any counter
    /// reaches zero.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_budget: Option<crate::messages::permissions::CostBudget>,
    /// Full ARCP v1.1 lease request. When both this and the legacy
    /// `cost_budget` field are present, this block takes precedence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lease_request: Option<crate::messages::permissions::LeaseRequest>,
}

impl ToolInvokePayload {
    /// New `tool.invoke` payload with no budget.
    #[must_use]
    pub fn new(tool: impl Into<String>, arguments: serde_json::Value) -> Self {
        Self {
            tool: tool.into(),
            arguments,
            cost_budget: None,
            lease_request: None,
        }
    }
}

/// Payload for `tool.result`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolResultPayload {
    /// Tool result, inline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,
    /// Tool result by reference (artifact).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_ref: Option<crate::messages::artifacts::ArtifactRef>,
}

/// Payload for `tool.error`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolErrorPayload {
    /// Canonical error code.
    pub code: ErrorCode,
    /// Whether the error is retryable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retryable: Option<bool>,
    /// Human-readable message.
    pub message: String,
    /// Optional structured detail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

/// Job state (RFC §10.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobState {
    /// `accepted` — runtime accepted the command but has not started work.
    Accepted,
    /// `queued` — work is waiting for capacity.
    Queued,
    /// `running` — work is actively executing.
    Running,
    /// `blocked` — work is waiting on permission / human input.
    Blocked,
    /// `paused` — work was intentionally suspended.
    Paused,
    /// `completed` — work finished successfully.
    Completed,
    /// `failed` — work reached a terminal error.
    Failed,
    /// `cancelled` — work was cancelled.
    Cancelled,
}

impl JobState {
    /// True if this state is a terminal state.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }

    /// Wire-level string (e.g. `"running"`) per ARCP §10.2.
    #[must_use]
    pub const fn wire_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Blocked => "blocked",
            Self::Paused => "paused",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

/// Payload for `job.accepted`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobAcceptedPayload {
    /// Newly minted job id.
    pub job_id: JobId,
    /// Lease-bound credentials issued for this job.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub credentials: Vec<crate::messages::credentials::ProvisionedCredential>,
    /// Final lease constraints accepted by the runtime.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lease: Option<crate::messages::permissions::LeaseRequest>,
}

/// Payload for `job.started`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobStartedPayload {
    /// Optional human-readable description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Payload for `job.progress` — the `progress` event body (ARCP v1.1
/// §8.2.1): `{ current, total?, units?, message? }`.
///
/// `current` MUST be a non-negative number. `total` is OPTIONAL; absent
/// means the work is indeterminate. When `total` is present, `current`
/// SHOULD be ≤ `total`. Use [`JobProgressPayload::validate`] to enforce
/// these invariants before emitting.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobProgressPayload {
    /// Units of work completed so far. MUST be non-negative.
    pub current: f64,
    /// Total units of work, if known. Absent means indeterminate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<f64>,
    /// Optional unit label (e.g. `"files"`, `"tokens"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub units: Option<String>,
    /// Optional human-readable message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl JobProgressPayload {
    /// Construct a progress body reporting `current` units done against an
    /// indeterminate total.
    #[must_use]
    pub const fn new(current: f64) -> Self {
        Self {
            current,
            total: None,
            units: None,
            message: None,
        }
    }

    /// Construct a progress body with a known `total`.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ARCPError::InvalidRequest`] if the values
    /// violate the §8.2.1 invariants (see [`Self::validate`]).
    pub fn with_total(current: f64, total: f64) -> Result<Self, crate::error::ARCPError> {
        let payload = Self {
            current,
            total: Some(total),
            units: None,
            message: None,
        };
        payload.validate()?;
        Ok(payload)
    }

    /// Attach a unit label, returning `self` for chaining.
    #[must_use]
    pub fn with_units(mut self, units: impl Into<String>) -> Self {
        self.units = Some(units.into());
        self
    }

    /// Attach a human-readable message, returning `self` for chaining.
    #[must_use]
    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }

    /// Validate the §8.2.1 progress invariants.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ARCPError::InvalidRequest`] if `current` is
    /// negative or non-finite, if `total` is negative or non-finite, or if
    /// `total` is present and `current` exceeds it.
    pub fn validate(&self) -> Result<(), crate::error::ARCPError> {
        if !self.current.is_finite() || self.current < 0.0 {
            return Err(crate::error::ARCPError::InvalidRequest {
                detail: format!(
                    "job.progress current must be non-negative, got {}",
                    self.current
                ),
            });
        }
        if let Some(total) = self.total {
            if !total.is_finite() || total < 0.0 {
                return Err(crate::error::ARCPError::InvalidRequest {
                    detail: format!("job.progress total must be non-negative, got {total}"),
                });
            }
            if self.current > total {
                return Err(crate::error::ARCPError::InvalidRequest {
                    detail: format!(
                        "job.progress current ({}) must not exceed total ({total})",
                        self.current
                    ),
                });
            }
        }
        Ok(())
    }
}

/// Payload for `job.heartbeat` (RFC §10.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobHeartbeatPayload {
    /// Monotonically increasing per-job sequence number.
    pub sequence: u64,
    /// Optional per-heartbeat deadline override (ms).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline_ms: Option<u64>,
    /// Current state at heartbeat time.
    pub state: JobState,
}

/// Payload for `job.checkpoint` (v0.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobCheckpointPayload {
    /// Checkpoint identifier.
    pub checkpoint_id: String,
    /// Opaque checkpoint data.
    pub data: serde_json::Value,
}

/// Payload for `job.completed`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobCompletedPayload {
    /// Optional inline result.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,
    /// Optional artifact reference for the result.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_ref: Option<crate::messages::artifacts::ArtifactRef>,
    /// Stable identifier for a streamed result (ARCP v1.1 §8.4).
    /// Present when the job emitted `job.result_chunk` events; references
    /// the assembled chunks rather than carrying the value inline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_id: Option<String>,
    /// Total decoded size in bytes of the streamed result (ARCP v1.1 §8.4).
    /// Optional; informational for clients rendering progress.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_size: Option<u64>,
    /// Optional human-readable summary, typically supplied by the agent
    /// alongside a streamed result (ARCP v1.1 §8.4 / §13.6).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

/// Payload for `job.failed`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobFailedPayload {
    /// Canonical error code.
    pub code: ErrorCode,
    /// Whether the error is retryable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retryable: Option<bool>,
    /// Human-readable message.
    pub message: String,
    /// Optional structured detail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

/// Payload for `job.cancelled`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobCancelledPayload {
    /// Free-form reason for cancellation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Payload for `job.schedule` (v0.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobSchedulePayload {
    /// Inner command envelope (e.g. `tool.invoke`).
    pub job: serde_json::Value,
    /// When to run (`at` / `every` / `after`).
    pub when: serde_json::Value,
}

/// Payload for `agent.delegate` (v0.2 stub).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentDelegatePayload {
    /// Target agent identifier.
    pub target: String,
    /// Task description.
    pub task: String,
    /// Optional inherited context.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<serde_json::Value>,
}

/// Payload for `agent.handoff` (v0.2 stub).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentHandoffPayload {
    /// Target runtime identity.
    pub runtime: serde_json::Value,
    /// Optional human-readable reason.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Payload for `workflow.start` (v0.2 stub).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowStartPayload {
    /// Workflow identifier.
    pub workflow: String,
    /// Workflow-specific arguments.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<serde_json::Value>,
}

/// Payload for `workflow.complete` (v0.2 stub).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowCompletePayload {
    /// Optional final value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod progress_tests {
    use super::*;

    #[test]
    fn progress_serializes_with_current_and_optional_total() {
        let payload = JobProgressPayload::with_total(47.0, 120.0)
            .unwrap()
            .with_units("files")
            .with_message("indexing");
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["current"], serde_json::json!(47.0));
        assert_eq!(json["total"], serde_json::json!(120.0));
        assert_eq!(json["units"], serde_json::json!("files"));
        assert_eq!(json["message"], serde_json::json!("indexing"));
        // `percent` is gone from the §8.2.1 body.
        assert!(json.get("percent").is_none());
    }

    #[test]
    fn indeterminate_progress_omits_total() {
        let payload = JobProgressPayload::new(5.0);
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["current"], serde_json::json!(5.0));
        assert!(json.get("total").is_none());
        payload.validate().unwrap();
    }

    #[test]
    fn negative_current_is_rejected() {
        let payload = JobProgressPayload::new(-1.0);
        assert!(payload.validate().is_err());
    }

    #[test]
    fn current_exceeding_total_is_rejected() {
        let err = JobProgressPayload::with_total(10.0, 5.0).unwrap_err();
        assert!(err.to_string().contains("must not exceed total"));
    }

    #[test]
    fn non_finite_current_is_rejected() {
        let payload = JobProgressPayload::new(f64::INFINITY);
        assert!(payload.validate().is_err());
    }
}
