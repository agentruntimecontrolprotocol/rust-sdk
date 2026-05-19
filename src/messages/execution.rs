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
}

impl ToolInvokePayload {
    /// New `tool.invoke` payload with no budget.
    #[must_use]
    pub fn new(tool: impl Into<String>, arguments: serde_json::Value) -> Self {
        Self {
            tool: tool.into(),
            arguments,
            cost_budget: None,
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

/// Encoding of [`JobResultChunkPayload::data`] (ARCP v1.1 §8.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResultChunkEncoding {
    /// Chunk payload is a UTF-8 string fragment.
    Utf8,
    /// Chunk payload is a base64-encoded binary fragment.
    Base64,
}

/// Payload for `job.result_chunk` (ARCP v1.1 §8.4 — `result_chunk`).
///
/// Streams the final result of a job in ordered fragments. The agent
/// MUST emit chunks for one `result_id` in `chunk_seq` order; the
/// terminating `job.completed` references the same `result_id`. Implementations
/// MUST NOT mix inline result and `result_chunk` for the same job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobResultChunkPayload {
    /// Stable identifier for the assembled result. Generated by the
    /// runtime (or agent) when streaming begins.
    pub result_id: String,
    /// 0-based monotonic chunk index per `result_id`.
    pub chunk_seq: u64,
    /// Chunk payload (text or base64-encoded bytes; see `encoding`).
    pub data: String,
    /// Wire-level encoding of `data`.
    pub encoding: ResultChunkEncoding,
    /// `true` when more chunks follow; `false` on the terminal chunk.
    pub more: bool,
}

/// Helper that accumulates [`JobResultChunkPayload`] fragments for a
/// single `result_id` and assembles the final payload when `more: false`
/// arrives.
///
/// Chunks must be supplied in `chunk_seq` order — out-of-order chunks
/// surface as [`ResultChunkError::OutOfOrder`]. Mixing encodings for the
/// same `result_id` surfaces as [`ResultChunkError::EncodingMismatch`].
#[derive(Debug, Default)]
pub struct ResultChunkAssembler {
    result_id: Option<String>,
    encoding: Option<ResultChunkEncoding>,
    next_seq: u64,
    buffer: Vec<u8>,
    finished: bool,
}

/// Errors returned by [`ResultChunkAssembler::push`] and
/// [`ResultChunkAssembler::finish`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ResultChunkError {
    /// A chunk arrived with a `chunk_seq` that did not match the next
    /// expected sequence.
    #[error("result_chunk out of order: expected seq {expected}, got {got}")]
    OutOfOrder {
        /// Expected next `chunk_seq`.
        expected: u64,
        /// Actual `chunk_seq` of the offending chunk.
        got: u64,
    },
    /// A chunk's `result_id` differs from previously buffered chunks.
    #[error("result_chunk result_id mismatch: expected {expected:?}, got {got:?}")]
    ResultIdMismatch {
        /// Expected `result_id` from the first buffered chunk.
        expected: String,
        /// Actual `result_id`.
        got: String,
    },
    /// A chunk's `encoding` differs from previously buffered chunks.
    #[error("result_chunk encoding mismatch: expected {expected:?}, got {got:?}")]
    EncodingMismatch {
        /// Encoding selected by the first chunk.
        expected: ResultChunkEncoding,
        /// Encoding on the offending chunk.
        got: ResultChunkEncoding,
    },
    /// A base64 fragment failed to decode.
    #[error("result_chunk base64 decode failed at seq {seq}")]
    Base64Decode {
        /// `chunk_seq` of the offending chunk.
        seq: u64,
    },
    /// More chunks were pushed after a terminal `more: false`.
    #[error("result_chunk: chunk pushed after final chunk")]
    AfterFinal,
    /// `finish` called before any terminal chunk arrived.
    #[error("result_chunk: not yet final")]
    NotFinal,
}

impl ResultChunkAssembler {
    /// Construct an empty assembler.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            result_id: None,
            encoding: None,
            next_seq: 0,
            buffer: Vec::new(),
            finished: false,
        }
    }

    /// Append `chunk`. Returns `Ok(true)` if this chunk was terminal
    /// (`more: false`), `Ok(false)` otherwise.
    ///
    /// # Errors
    ///
    /// Returns a [`ResultChunkError`] when the chunk violates ordering,
    /// `result_id`, or encoding invariants, or when called after a
    /// terminal chunk.
    pub fn push(&mut self, chunk: &JobResultChunkPayload) -> Result<bool, ResultChunkError> {
        if self.finished {
            return Err(ResultChunkError::AfterFinal);
        }
        if chunk.chunk_seq != self.next_seq {
            return Err(ResultChunkError::OutOfOrder {
                expected: self.next_seq,
                got: chunk.chunk_seq,
            });
        }
        if let Some(rid) = self.result_id.as_deref() {
            if rid != chunk.result_id {
                return Err(ResultChunkError::ResultIdMismatch {
                    expected: rid.to_owned(),
                    got: chunk.result_id.clone(),
                });
            }
        } else {
            self.result_id = Some(chunk.result_id.clone());
        }
        if let Some(enc) = self.encoding {
            if enc != chunk.encoding {
                return Err(ResultChunkError::EncodingMismatch {
                    expected: enc,
                    got: chunk.encoding,
                });
            }
        } else {
            self.encoding = Some(chunk.encoding);
        }
        match chunk.encoding {
            ResultChunkEncoding::Utf8 => {
                self.buffer.extend_from_slice(chunk.data.as_bytes());
            }
            ResultChunkEncoding::Base64 => {
                let decoded =
                    decode_base64(&chunk.data).map_err(|()| ResultChunkError::Base64Decode {
                        seq: chunk.chunk_seq,
                    })?;
                self.buffer.extend_from_slice(&decoded);
            }
        }
        self.next_seq += 1;
        if !chunk.more {
            self.finished = true;
        }
        Ok(!chunk.more)
    }

    /// True once a terminal chunk has been pushed.
    #[must_use]
    pub const fn is_finished(&self) -> bool {
        self.finished
    }

    /// The selected encoding, if any chunks have arrived.
    #[must_use]
    pub const fn encoding(&self) -> Option<ResultChunkEncoding> {
        self.encoding
    }

    /// The `result_id` of the buffered stream, if any chunks have
    /// arrived.
    #[must_use]
    pub fn result_id(&self) -> Option<&str> {
        self.result_id.as_deref()
    }

    /// Consume the assembler and return the assembled bytes.
    ///
    /// # Errors
    ///
    /// Returns [`ResultChunkError::NotFinal`] if no terminal chunk has
    /// arrived yet.
    pub fn finish(self) -> Result<Vec<u8>, ResultChunkError> {
        if !self.finished {
            return Err(ResultChunkError::NotFinal);
        }
        Ok(self.buffer)
    }

    /// Consume the assembler and decode the buffer as UTF-8.
    ///
    /// # Errors
    ///
    /// Returns [`ResultChunkError::NotFinal`] if no terminal chunk has
    /// arrived yet, or [`ResultChunkError::Base64Decode`] (with
    /// `seq: u64::MAX`) if the assembled buffer is not valid UTF-8.
    pub fn finish_utf8(self) -> Result<String, ResultChunkError> {
        let bytes = self.finish()?;
        String::from_utf8(bytes).map_err(|_| ResultChunkError::Base64Decode { seq: u64::MAX })
    }
}

/// Minimal base64 decoder for [`ResultChunkAssembler`] so the crate need
/// not pull in a base64 dependency.
fn decode_base64(input: &str) -> Result<Vec<u8>, ()> {
    const fn val(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let bytes: Vec<u8> = input.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    let (data, pad) = bytes
        .iter()
        .position(|&b| b == b'=')
        .map_or((bytes.as_slice(), 0), |p| (&bytes[..p], bytes.len() - p));
    if (data.len() + pad) % 4 != 0 {
        return Err(());
    }
    let mut out = Vec::with_capacity(data.len() * 3 / 4);
    let mut chunk = [0u8; 4];
    let mut filled = 0;
    for &b in data {
        let v = val(b).ok_or(())?;
        chunk[filled] = v;
        filled += 1;
        if filled == 4 {
            out.push((chunk[0] << 2) | (chunk[1] >> 4));
            out.push((chunk[1] << 4) | (chunk[2] >> 2));
            out.push((chunk[2] << 6) | chunk[3]);
            filled = 0;
        }
    }
    match filled {
        0 => {}
        2 => out.push((chunk[0] << 2) | (chunk[1] >> 4)),
        3 => {
            out.push((chunk[0] << 2) | (chunk[1] >> 4));
            out.push((chunk[1] << 4) | (chunk[2] >> 2));
        }
        _ => return Err(()),
    }
    Ok(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod result_chunk_tests {
    use super::*;

    #[test]
    fn utf8_chunks_assemble_in_order() {
        let mut a = ResultChunkAssembler::new();
        for (seq, fragment, more) in [(0u64, "hello ", true), (1, "world", false)] {
            let done = a
                .push(&JobResultChunkPayload {
                    result_id: "res_x".into(),
                    chunk_seq: seq,
                    data: fragment.into(),
                    encoding: ResultChunkEncoding::Utf8,
                    more,
                })
                .unwrap();
            assert_eq!(done, !more);
        }
        assert!(a.is_finished());
        assert_eq!(a.finish_utf8().unwrap(), "hello world");
    }

    #[test]
    fn out_of_order_chunks_rejected() {
        let mut a = ResultChunkAssembler::new();
        let _ = a
            .push(&JobResultChunkPayload {
                result_id: "r".into(),
                chunk_seq: 0,
                data: "a".into(),
                encoding: ResultChunkEncoding::Utf8,
                more: true,
            })
            .unwrap();
        let err = a
            .push(&JobResultChunkPayload {
                result_id: "r".into(),
                chunk_seq: 2,
                data: "c".into(),
                encoding: ResultChunkEncoding::Utf8,
                more: false,
            })
            .unwrap_err();
        assert!(matches!(
            err,
            ResultChunkError::OutOfOrder {
                expected: 1,
                got: 2
            }
        ));
    }

    #[test]
    fn encoding_mismatch_rejected() {
        let mut a = ResultChunkAssembler::new();
        let _ = a
            .push(&JobResultChunkPayload {
                result_id: "r".into(),
                chunk_seq: 0,
                data: "a".into(),
                encoding: ResultChunkEncoding::Utf8,
                more: true,
            })
            .unwrap();
        let err = a
            .push(&JobResultChunkPayload {
                result_id: "r".into(),
                chunk_seq: 1,
                data: "AA==".into(),
                encoding: ResultChunkEncoding::Base64,
                more: false,
            })
            .unwrap_err();
        assert!(matches!(err, ResultChunkError::EncodingMismatch { .. }));
    }

    #[test]
    fn base64_chunks_assemble() {
        let mut a = ResultChunkAssembler::new();
        // "hi" = 0x68, 0x69; base64 = "aGk="
        a.push(&JobResultChunkPayload {
            result_id: "r".into(),
            chunk_seq: 0,
            data: "aGk=".into(),
            encoding: ResultChunkEncoding::Base64,
            more: false,
        })
        .unwrap();
        assert_eq!(a.finish().unwrap(), b"hi");
    }

    #[test]
    fn finish_before_terminal_is_error() {
        let mut a = ResultChunkAssembler::new();
        a.push(&JobResultChunkPayload {
            result_id: "r".into(),
            chunk_seq: 0,
            data: "x".into(),
            encoding: ResultChunkEncoding::Utf8,
            more: true,
        })
        .unwrap();
        assert!(matches!(a.finish(), Err(ResultChunkError::NotFinal)));
    }

    #[test]
    fn payload_round_trips_through_serde() {
        let p = JobResultChunkPayload {
            result_id: "res_01J".into(),
            chunk_seq: 7,
            data: "fragment".into(),
            encoding: ResultChunkEncoding::Utf8,
            more: true,
        };
        let j = serde_json::to_string(&p).unwrap();
        let back: JobResultChunkPayload = serde_json::from_str(&j).unwrap();
        assert_eq!(p, back);
    }
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
