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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolInvokePayload {
    /// Tool identifier.
    pub tool: String,
    /// Tool-specific arguments.
    pub arguments: serde_json::Value,
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
}

/// Payload for `job.accepted`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobAcceptedPayload {
    /// Newly minted job id.
    pub job_id: JobId,
}

/// Payload for `job.started`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobStartedPayload {
    /// Optional human-readable description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Payload for `job.progress`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobProgressPayload {
    /// Percent complete, 0.0 to 100.0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub percent: Option<f64>,
    /// Optional human-readable message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
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
