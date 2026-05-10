//! Execution messages — tools, jobs, agents, workflows (RFC §10).

use serde::{Deserialize, Serialize};

use crate::error::ErrorCode;
use crate::ids::JobId;

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
