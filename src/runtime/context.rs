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
use crate::messages::{HumanChoiceRequestPayload, HumanInputRequestPayload, MessageType};

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
}
