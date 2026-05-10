//! `ARCPClient` and the type-state [`Session<S>`] (RFC §4.6, §8).

use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::Arc;

use dashmap::DashMap;
use tokio::sync::{oneshot, Mutex};

use crate::client::handlers::{HumanInputHandler, NoopHumanInputHandler};
use crate::envelope::Envelope;
use crate::error::{ARCPError, ErrorCode};
use crate::ids::{JobId, MessageId, SessionId};
use crate::messages::{
    CancelPayload, CancelTargetKind, Capabilities, ClientIdentity, Credentials,
    HumanChoiceResponsePayload, HumanInputResponsePayload, JobAcceptedPayload, JobCompletedPayload,
    JobFailedPayload, MessageType, SessionAcceptedPayload, SessionOpenPayload, ToolInvokePayload,
};
use crate::transport::Transport;

/// Marker trait sealed inside this module — only [`Unauthenticated`] and
/// [`Authenticated`] satisfy it.
mod sealed {
    pub trait State {}
    impl State for super::Unauthenticated {}
    impl State for super::Authenticated {}
}

/// Type-state marker: the session has not yet completed `session.accepted`.
#[derive(Debug)]
pub struct Unauthenticated;

/// Type-state marker: the session has completed `session.accepted`.
#[derive(Debug)]
pub struct Authenticated;

/// Type-state session handle.
///
/// `Session<Unauthenticated>` exposes only [`Session::authenticate`].
/// `Session<Authenticated>` exposes the rest of the protocol surface
/// (Phase 3+ adds `invoke`, `subscribe`, etc.).
pub struct Session<S: sealed::State, T: Transport + 'static> {
    inner: Arc<SessionInner<T>>,
    _state: PhantomData<S>,
}

struct SessionInner<T: Transport + 'static> {
    transport: Arc<dyn Transport>,
    session_id: Mutex<Option<SessionId>>,
    capabilities: Mutex<Capabilities>,
    /// Pending: `correlation_id` → notifier. The reader task resolves on terminal job events.
    pending_jobs: DashMap<MessageId, JobNotifier>,
    /// invoke→accepted: `correlation_id` → oneshot for the `job_id`.
    pending_accepted: DashMap<MessageId, oneshot::Sender<JobId>>,
    reader: Mutex<Option<tokio::task::JoinHandle<()>>>,
    human_handler: Arc<dyn HumanInputHandler>,
    _transport_kind: PhantomData<T>,
}

#[derive(Debug)]
enum JobNotifier {
    Pending(oneshot::Sender<Result<serde_json::Value, ARCPError>>),
    /// In-progress sentinel after the slot has been claimed.
    Taken,
}

impl JobNotifier {
    fn take(&mut self) -> Option<oneshot::Sender<Result<serde_json::Value, ARCPError>>> {
        match std::mem::replace(self, Self::Taken) {
            Self::Pending(tx) => Some(tx),
            Self::Taken => None,
        }
    }
}

impl<S: sealed::State, T: Transport + 'static> std::fmt::Debug for Session<S, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("state", &std::any::type_name::<S>())
            .finish_non_exhaustive()
    }
}

impl<T: Transport + 'static> Session<Unauthenticated, T> {
    /// Drive the four-step handshake (RFC §8.1) and, on success, return a
    /// [`Session<Authenticated>`].
    ///
    /// On success a background reader task is spawned to dispatch incoming
    /// envelopes (job terminal events, etc.) into the session's pending
    /// registry.
    ///
    /// # Errors
    ///
    /// Returns [`ARCPError::Unauthenticated`] if the runtime emits
    /// `session.rejected` or `session.unauthenticated`,
    /// [`ARCPError::Unavailable`] if the transport closes mid-handshake,
    /// [`ARCPError::Internal`] for protocol violations.
    pub async fn authenticate(
        self,
        creds: Credentials,
        client: ClientIdentity,
        caps: Capabilities,
    ) -> Result<Session<Authenticated, T>, ARCPError> {
        let open = Envelope::new(MessageType::SessionOpen(SessionOpenPayload {
            auth: creds.clone(),
            client,
            capabilities: caps,
        }));
        let open_id = open.id.clone();
        self.inner.transport.send(open).await?;

        let env = self
            .inner
            .transport
            .recv()
            .await?
            .ok_or_else(|| ARCPError::Unavailable {
                detail: "transport closed during handshake".into(),
            })?;
        match env.payload {
            MessageType::SessionAccepted(SessionAcceptedPayload {
                session_id,
                capabilities,
                ..
            }) => {
                *self.inner.session_id.lock().await = Some(session_id);
                *self.inner.capabilities.lock().await = capabilities;

                // Spawn the reader task for post-handshake envelopes.
                let inner_for_reader = Arc::clone(&self.inner);
                let reader = tokio::spawn(async move {
                    Self::reader_loop(inner_for_reader).await;
                });
                *self.inner.reader.lock().await = Some(reader);

                Ok(Session {
                    inner: self.inner.clone(),
                    _state: PhantomData,
                })
            }
            MessageType::SessionRejected(p) => Err(ARCPError::Unauthenticated {
                detail: format!("session.rejected ({}): {}", p.code, p.message),
            }),
            MessageType::SessionUnauthenticated(p) => Err(ARCPError::Unauthenticated {
                detail: format!("session.unauthenticated ({}): {}", p.code, p.message),
            }),
            MessageType::SessionChallenge(p) => Err(ARCPError::Unauthenticated {
                detail: format!(
                    "runtime issued a challenge (\"{}\") but Phase 2 client cannot respond; \
                     correlation_id={}",
                    p.challenge, open_id
                ),
            }),
            other => Err(ARCPError::Internal {
                detail: format!("unexpected handshake response: type={}", other.type_name()),
            }),
        }
    }

    async fn reader_loop(inner: Arc<SessionInner<T>>) {
        while let Ok(Some(env)) = inner.transport.recv().await {
            // Human-input requests don't carry a correlation_id matching a
            // pending job entry; forward them to the user-supplied handler.
            match env.payload.clone() {
                MessageType::HumanInputRequest(payload) => {
                    let handler = Arc::clone(&inner.human_handler);
                    let transport = Arc::clone(&inner.transport);
                    let request_id = env.id.clone();
                    let session_id = env.session_id.clone();
                    tokio::spawn(async move {
                        let value = handler.input(payload).await;
                        let mut response = Envelope::new(MessageType::HumanInputResponse(
                            HumanInputResponsePayload {
                                value,
                                responded_by: "client-handler".into(),
                                responded_at: chrono::Utc::now(),
                            },
                        ));
                        response.correlation_id = Some(request_id);
                        response.session_id = session_id;
                        let _ = transport.send(response).await;
                    });
                    continue;
                }
                MessageType::HumanChoiceRequest(payload) => {
                    let handler = Arc::clone(&inner.human_handler);
                    let transport = Arc::clone(&inner.transport);
                    let request_id = env.id.clone();
                    let session_id = env.session_id.clone();
                    tokio::spawn(async move {
                        let choice_id = handler.choice(payload).await;
                        let mut response = Envelope::new(MessageType::HumanChoiceResponse(
                            HumanChoiceResponsePayload {
                                choice_id,
                                responded_by: "client-handler".into(),
                                responded_at: chrono::Utc::now(),
                            },
                        ));
                        response.correlation_id = Some(request_id);
                        response.session_id = session_id;
                        let _ = transport.send(response).await;
                    });
                    continue;
                }
                _ => {}
            }

            let Some(corr) = env.correlation_id.clone() else {
                continue;
            };
            match env.payload {
                MessageType::JobAccepted(JobAcceptedPayload { job_id }) => {
                    if let Some((_, tx)) = inner.pending_accepted.remove(&corr) {
                        let _ = tx.send(job_id);
                    }
                }
                MessageType::JobCompleted(JobCompletedPayload { value, .. }) => {
                    if let Some(mut entry) = inner.pending_jobs.get_mut(&corr) {
                        if let Some(tx) = entry.take() {
                            let _ = tx.send(Ok(value.unwrap_or(serde_json::Value::Null)));
                        }
                    }
                }
                MessageType::JobFailed(JobFailedPayload { code, message, .. }) => {
                    if let Some(mut entry) = inner.pending_jobs.get_mut(&corr) {
                        if let Some(tx) = entry.take() {
                            let _ = tx.send(Err(ARCPError::Unknown {
                                detail: format!("job failed ({code}): {message}"),
                            }));
                        }
                    }
                }
                MessageType::JobCancelled(p) => {
                    if let Some(mut entry) = inner.pending_jobs.get_mut(&corr) {
                        if let Some(tx) = entry.take() {
                            let reason = p.reason.unwrap_or_default();
                            let _ = tx.send(Err(ARCPError::Cancelled { reason }));
                        }
                    }
                }
                _ => { /* ignore intermediate events for now */ }
            }
        }
        // Drain pending entries with an Unavailable error.
        let keys: Vec<MessageId> = inner.pending_jobs.iter().map(|r| r.key().clone()).collect();
        for k in keys {
            if let Some(mut entry) = inner.pending_jobs.get_mut(&k) {
                if let Some(tx) = entry.take() {
                    let _ = tx.send(Err(ARCPError::Unavailable {
                        detail: "transport closed".into(),
                    }));
                }
            }
        }
    }
}

impl<T: Transport + 'static> Session<Authenticated, T> {
    /// Return the negotiated session id.
    ///
    /// # Errors
    ///
    /// Returns [`ARCPError::Internal`] if called on a session that somehow
    /// reached the `Authenticated` state without an id (cannot happen in
    /// well-formed code).
    pub async fn id(&self) -> Result<SessionId, ARCPError> {
        self.inner
            .session_id
            .lock()
            .await
            .clone()
            .ok_or_else(|| ARCPError::Internal {
                detail: "authenticated session missing id".into(),
            })
    }

    /// Return the negotiated capability set.
    pub async fn capabilities(&self) -> Capabilities {
        self.inner.capabilities.lock().await.clone()
    }

    /// Invoke a tool by name. Returns a [`JobHandle`] the caller can await
    /// for the terminal result, and which carries the runtime-assigned
    /// [`JobId`] once `job.accepted` arrives (RFC §10).
    ///
    /// # Errors
    ///
    /// Returns [`ARCPError::Unavailable`] if the transport closes before
    /// the runtime acknowledges the invocation.
    pub async fn invoke(
        &self,
        tool: impl Into<String>,
        arguments: serde_json::Value,
    ) -> Result<JobHandle, ARCPError> {
        let session_id = self.id().await?;
        let mut env = Envelope::new(MessageType::ToolInvoke(ToolInvokePayload {
            tool: tool.into(),
            arguments,
        }));
        env.session_id = Some(session_id);
        let correlation_id = env.id.clone();

        let (acc_tx, acc_rx) = oneshot::channel::<JobId>();
        let (term_tx, term_rx) = oneshot::channel::<Result<serde_json::Value, ARCPError>>();
        self.inner
            .pending_accepted
            .insert(correlation_id.clone(), acc_tx);
        self.inner
            .pending_jobs
            .insert(correlation_id.clone(), JobNotifier::Pending(term_tx));

        self.inner.transport.send(env).await?;

        let job_id = acc_rx.await.map_err(|_| ARCPError::Unavailable {
            detail: "runtime closed before job.accepted".into(),
        })?;

        Ok(JobHandle {
            job_id,
            correlation_id,
            terminal: Mutex::new(Some(term_rx)),
            transport: Arc::clone(&self.inner.transport),
            session_id: self.id().await?,
        })
    }
}

/// Handle to an in-flight job (RFC §10).
pub struct JobHandle {
    /// Server-assigned job identifier.
    pub job_id: JobId,
    /// Correlation id of the originating `tool.invoke`.
    pub correlation_id: MessageId,
    terminal: Mutex<Option<oneshot::Receiver<Result<serde_json::Value, ARCPError>>>>,
    transport: Arc<dyn Transport>,
    session_id: SessionId,
}

impl std::fmt::Debug for JobHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JobHandle")
            .field("job_id", &self.job_id)
            .field("correlation_id", &self.correlation_id)
            .finish_non_exhaustive()
    }
}

impl JobHandle {
    /// Await the terminal job event.
    ///
    /// # Errors
    ///
    /// Returns [`ARCPError::Cancelled`] if the job ended via
    /// `job.cancelled`, [`ARCPError::Unknown`] for `job.failed`, or
    /// [`ARCPError::Unavailable`] if the connection ended before the
    /// terminal event was observed.
    pub async fn join(&self) -> Result<serde_json::Value, ARCPError> {
        let rx =
            self.terminal
                .lock()
                .await
                .take()
                .ok_or_else(|| ARCPError::FailedPrecondition {
                    detail: "JobHandle::join called twice".into(),
                })?;
        rx.await.unwrap_or(Err(ARCPError::Unavailable {
            detail: "runtime channel closed before terminal event".into(),
        }))
    }

    /// Send a `cancel` envelope for this job. Does not await the
    /// `cancel.accepted` reply; the next [`Self::join`] reflects the
    /// cancellation outcome.
    ///
    /// # Errors
    ///
    /// Returns [`ARCPError::Unavailable`] if the transport is already
    /// closed.
    pub async fn cancel(&self, reason: impl Into<String>) -> Result<(), ARCPError> {
        let mut env = Envelope::new(MessageType::Cancel(CancelPayload {
            target: CancelTargetKind::Job,
            target_id: self.job_id.to_string(),
            reason: Some(reason.into()),
            deadline_ms: Some(5000),
        }));
        env.session_id = Some(self.session_id.clone());
        self.transport.send(env).await
    }
}

/// Client-side entry point.
pub struct ARCPClient<T: Transport + 'static> {
    transport: Option<T>,
    human_handler: Arc<dyn HumanInputHandler>,
}

impl<T: Transport + 'static> std::fmt::Debug for ARCPClient<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ARCPClient")
            .field("attached", &self.transport.is_some())
            .finish_non_exhaustive()
    }
}

impl<T: Transport + 'static> ARCPClient<T> {
    /// Construct over an attached transport.
    #[must_use]
    pub fn new(transport: T) -> Self {
        Self {
            transport: Some(transport),
            human_handler: Arc::new(NoopHumanInputHandler),
        }
    }

    /// Replace the default no-op handler with `handler`.
    #[must_use]
    pub fn with_human_input_handler(mut self, handler: Arc<dyn HumanInputHandler>) -> Self {
        self.human_handler = handler;
        self
    }

    /// Open an unauthenticated session.
    ///
    /// # Errors
    ///
    /// Returns [`ARCPError`] with code [`ErrorCode::FailedPrecondition`] if
    /// the client has already opened its session (the underlying transport
    /// is consumed at that point).
    pub fn open(mut self) -> Result<Session<Unauthenticated, T>, ARCPError> {
        let transport = self
            .transport
            .take()
            .ok_or_else(|| ARCPError::FailedPrecondition {
                detail: "client transport has already been consumed".into(),
            })?;
        let _ = ErrorCode::FailedPrecondition;
        Ok(Session {
            inner: Arc::new(SessionInner {
                transport: Arc::new(transport),
                session_id: Mutex::new(None),
                capabilities: Mutex::new(Capabilities::default()),
                pending_jobs: DashMap::new(),
                pending_accepted: DashMap::new(),
                reader: Mutex::new(None),
                human_handler: Arc::clone(&self.human_handler),
                _transport_kind: PhantomData,
            }),
            _state: PhantomData,
        })
    }
}

// HashMap import is required for the future Phase 4 handler maps.
const _: fn() = || {
    let _: HashMap<u8, u8>;
};
