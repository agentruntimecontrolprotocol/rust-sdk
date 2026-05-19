//! Per-job context handed to [`crate::runtime::ToolHandler::invoke`].
//!
//! Carries the cancellation token plus channels back to the runtime for
//! issuing human-input requests and (later) recording metrics, opening
//! streams, etc.

use std::sync::Arc;

use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use crate::envelope::Envelope;
use crate::error::{ARCPError, ErrorCode};
use crate::ids::{JobId, MessageId, SessionId};
use crate::messages::{
    HumanChoiceRequestPayload, HumanInputRequestPayload, JobResultChunkPayload, MessageType,
    ResultChunkEncoding,
};

/// Per-job dispatch context.
pub struct ToolContext {
    /// Cooperative cancellation token. Handlers MUST poll this.
    pub cancel: CancellationToken,
    pub(crate) job_id: JobId,
    pub(crate) session_id: SessionId,
    pub(crate) correlation_id: MessageId,
    pub(crate) out: mpsc::Sender<Envelope>,
    pub(crate) pending_human: Arc<dashmap::DashMap<MessageId, oneshot::Sender<HumanResponse>>>,
}

impl std::fmt::Debug for ToolContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolContext")
            .field("job_id", &self.job_id)
            .field("session_id", &self.session_id)
            .finish_non_exhaustive()
    }
}

/// Human-input response variants delivered to a waiting handler.
#[derive(Debug, Clone)]
pub enum HumanResponse {
    /// Operator-supplied value (corresponds to `human.input.response`).
    Value(serde_json::Value),
    /// Operator-supplied choice id (corresponds to `human.choice.response`).
    Choice(String),
    /// Request was cancelled or timed out.
    Cancelled(ErrorCode),
}

impl ToolContext {
    /// Send a `human.input.request` and await the response.
    ///
    /// # Errors
    ///
    /// Returns [`ARCPError::Cancelled`] if the request is cancelled or its
    /// deadline elapses; [`ARCPError::Unavailable`] if the connection is
    /// torn down before a response arrives.
    pub async fn request_human_input(
        &self,
        payload: HumanInputRequestPayload,
    ) -> Result<serde_json::Value, ARCPError> {
        let (tx, rx) = oneshot::channel();
        let mut env = Envelope::new(MessageType::HumanInputRequest(payload));
        env.session_id = Some(self.session_id.clone());
        env.job_id = Some(self.job_id.clone());
        let req_id = env.id.clone();
        self.pending_human.insert(req_id.clone(), tx);
        self.out
            .send(env)
            .await
            .map_err(|_| ARCPError::Unavailable {
                detail: "outbound channel closed".into(),
            })?;
        match rx.await {
            Ok(HumanResponse::Value(v)) => Ok(v),
            Ok(HumanResponse::Choice(_)) => Err(ARCPError::InvalidArgument {
                detail: "expected human.input.response, got human.choice.response".into(),
            }),
            Ok(HumanResponse::Cancelled(code)) => Err(ARCPError::Cancelled {
                reason: format!("human input cancelled: {code}"),
            }),
            Err(_) => Err(ARCPError::Unavailable {
                detail: "human-input pending channel dropped".into(),
            }),
        }
    }

    /// Send a `human.choice.request` and await the chosen option id.
    ///
    /// # Errors
    ///
    /// Same as [`Self::request_human_input`].
    pub async fn request_human_choice(
        &self,
        payload: HumanChoiceRequestPayload,
    ) -> Result<String, ARCPError> {
        let (tx, rx) = oneshot::channel();
        let mut env = Envelope::new(MessageType::HumanChoiceRequest(payload));
        env.session_id = Some(self.session_id.clone());
        env.job_id = Some(self.job_id.clone());
        let req_id = env.id.clone();
        self.pending_human.insert(req_id.clone(), tx);
        self.out
            .send(env)
            .await
            .map_err(|_| ARCPError::Unavailable {
                detail: "outbound channel closed".into(),
            })?;
        match rx.await {
            Ok(HumanResponse::Choice(c)) => Ok(c),
            Ok(HumanResponse::Value(_)) => Err(ARCPError::InvalidArgument {
                detail: "expected human.choice.response, got human.input.response".into(),
            }),
            Ok(HumanResponse::Cancelled(code)) => Err(ARCPError::Cancelled {
                reason: format!("human choice cancelled: {code}"),
            }),
            Err(_) => Err(ARCPError::Unavailable {
                detail: "human-choice pending channel dropped".into(),
            }),
        }
    }

    /// The id of the originating `tool.invoke`.
    #[must_use]
    pub const fn correlation_id(&self) -> &MessageId {
        &self.correlation_id
    }

    /// The job id the runtime assigned.
    #[must_use]
    pub const fn job_id(&self) -> &JobId {
        &self.job_id
    }

    /// Emit one `job.result_chunk` fragment (ARCP v1.1 §8.4).
    ///
    /// `chunk_seq` is the caller's responsibility — start at 0 and
    /// increment per chunk for the same `result_id`. The terminal chunk
    /// MUST set `more: false`; the job's terminal `job.completed`
    /// SHOULD then carry the same `result_id`.
    ///
    /// # Errors
    ///
    /// Returns [`ARCPError::Unavailable`] if the outbound channel is
    /// closed.
    pub async fn emit_result_chunk(
        &self,
        result_id: impl Into<String>,
        chunk_seq: u64,
        data: impl Into<String>,
        encoding: ResultChunkEncoding,
        more: bool,
    ) -> Result<(), ARCPError> {
        let mut env = Envelope::new(MessageType::JobResultChunk(JobResultChunkPayload {
            result_id: result_id.into(),
            chunk_seq,
            data: data.into(),
            encoding,
            more,
        }));
        env.session_id = Some(self.session_id.clone());
        env.job_id = Some(self.job_id.clone());
        env.correlation_id = Some(self.correlation_id.clone());
        self.out
            .send(env)
            .await
            .map_err(|_| ARCPError::Unavailable {
                detail: "outbound channel closed".into(),
            })
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
    use chrono::Utc;
    use dashmap::DashMap;
    use tokio::sync::mpsc;

    use super::*;
    use crate::messages::{ChoiceOption, HumanChoiceRequestPayload, HumanInputRequestPayload};

    fn build_ctx() -> (
        ToolContext,
        mpsc::Receiver<Envelope>,
        Arc<DashMap<MessageId, oneshot::Sender<HumanResponse>>>,
    ) {
        let (out_tx, out_rx) = mpsc::channel(8);
        let pending: Arc<DashMap<MessageId, oneshot::Sender<HumanResponse>>> =
            Arc::new(DashMap::new());
        let ctx = ToolContext {
            cancel: CancellationToken::new(),
            job_id: JobId::new(),
            session_id: SessionId::new(),
            correlation_id: MessageId::new(),
            out: out_tx,
            pending_human: Arc::clone(&pending),
        };
        (ctx, out_rx, pending)
    }

    fn input_request() -> HumanInputRequestPayload {
        HumanInputRequestPayload {
            prompt: "?".into(),
            response_schema: serde_json::json!({}),
            default: None,
            expires_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn accessors_return_internal_ids() {
        let (ctx, _rx, _pending) = build_ctx();
        // Just exercise the const accessors so they're covered.
        assert!(ctx.correlation_id().as_str().starts_with("msg_"));
        assert!(ctx.job_id().as_str().starts_with("job_"));
    }

    #[tokio::test]
    async fn input_round_trip_resolves_via_pending_map() {
        let (ctx, mut rx, pending) = build_ctx();
        let task = tokio::spawn(async move { ctx.request_human_input(input_request()).await });
        let env = rx.recv().await.expect("envelope");
        let id = env.id.clone();
        let (_, tx) = pending.remove(&id).expect("pending entry");
        tx.send(HumanResponse::Value(serde_json::json!({"ok": true})))
            .expect("send");
        let result = task.await.expect("join");
        assert_eq!(result.expect("ok"), serde_json::json!({"ok": true}));
    }

    #[tokio::test]
    async fn input_returns_invalid_argument_on_choice_response() {
        let (ctx, mut rx, pending) = build_ctx();
        let task = tokio::spawn(async move { ctx.request_human_input(input_request()).await });
        let env = rx.recv().await.expect("envelope");
        let (_, tx) = pending.remove(&env.id).expect("pending");
        tx.send(HumanResponse::Choice("nope".into())).expect("send");
        let err = task.await.expect("join").expect_err("must error");
        assert!(matches!(err, ARCPError::InvalidArgument { .. }));
    }

    #[tokio::test]
    async fn input_propagates_cancellation_code() {
        let (ctx, mut rx, pending) = build_ctx();
        let task = tokio::spawn(async move { ctx.request_human_input(input_request()).await });
        let env = rx.recv().await.expect("envelope");
        let (_, tx) = pending.remove(&env.id).expect("pending");
        tx.send(HumanResponse::Cancelled(ErrorCode::DeadlineExceeded))
            .expect("send");
        let err = task.await.expect("join").expect_err("must error");
        assert!(matches!(err, ARCPError::Cancelled { .. }));
    }

    #[tokio::test]
    async fn choice_round_trip_resolves_via_pending_map() {
        let (ctx, mut rx, pending) = build_ctx();
        let payload = HumanChoiceRequestPayload {
            prompt: "?".into(),
            options: vec![ChoiceOption {
                id: "x".into(),
                label: "X".into(),
            }],
            expires_at: Utc::now(),
        };
        let task = tokio::spawn(async move { ctx.request_human_choice(payload).await });
        let env = rx.recv().await.expect("envelope");
        let (_, tx) = pending.remove(&env.id).expect("pending");
        tx.send(HumanResponse::Choice("x".into())).expect("send");
        let chosen = task.await.expect("join").expect("ok");
        assert_eq!(chosen, "x");
    }

    #[tokio::test]
    async fn choice_returns_invalid_argument_on_value_response() {
        let (ctx, mut rx, pending) = build_ctx();
        let payload = HumanChoiceRequestPayload {
            prompt: "?".into(),
            options: vec![],
            expires_at: Utc::now(),
        };
        let task = tokio::spawn(async move { ctx.request_human_choice(payload).await });
        let env = rx.recv().await.expect("envelope");
        let (_, tx) = pending.remove(&env.id).expect("pending");
        tx.send(HumanResponse::Value(serde_json::json!(null)))
            .expect("send");
        let err = task.await.expect("join").expect_err("must error");
        assert!(matches!(err, ARCPError::InvalidArgument { .. }));
    }
}
