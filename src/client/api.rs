//! `ARCPClient` and the type-state [`Session<S>`] (RFC §4.6, §8).

use std::marker::PhantomData;
use std::sync::Arc;

use dashmap::DashMap;
use tokio::sync::{mpsc, oneshot, Mutex};

use crate::envelope::Envelope;
use crate::error::{ARCPError, ErrorCode};
use crate::ids::{ArtifactId, JobId, MessageId, SessionId, SubscriptionId};
use crate::messages::{
    ArtifactFetchPayload, ArtifactPutPayload, ArtifactRef, ArtifactReleasePayload, CancelPayload,
    CancelTargetKind, Capabilities, ClientIdentity, Credentials, JobAcceptedPayload,
    JobCompletedPayload, JobFailedPayload,
    MessageType, NackPayload, SessionAcceptedPayload, SessionOpenPayload, SubscribePayload,
    SubscriptionFilter, SubscriptionSince, ToolInvokePayload, UnsubscribePayload,
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
    /// Pending artifact responses (put → `ArtifactRef`, fetch → bytes).
    pending_artifact: DashMap<MessageId, oneshot::Sender<ArtifactReply>>,
    /// Active subscriptions: `subscription_id` → forwarder channel.
    /// Wrapped in `Arc` so [`SubscriptionHandle`]'s Drop can drop only
    /// its own slot without holding a reference to the whole inner.
    active_subscriptions: Arc<DashMap<SubscriptionId, mpsc::UnboundedSender<Envelope>>>,
    /// `correlation_id` for `subscribe` → `oneshot` for `subscribe.accepted`.
    pending_subscribe: DashMap<MessageId, oneshot::Sender<SubscriptionId>>,
    reader: Mutex<Option<tokio::task::JoinHandle<()>>>,
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

/// Reply variants the runtime can send for an artifact request.
#[derive(Debug)]
enum ArtifactReply {
    /// `artifact.put` succeeded; carries the canonical reference.
    Ref(ArtifactRef),
    /// `artifact.fetch` succeeded; carries the inline base64 body and
    /// media type.
    Inline { data: String, media_type: String },
    /// Runtime returned a `nack` (`NOT_FOUND`, `INVALID_ARGUMENT`, etc.).
    Nack(NackPayload),
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
            // Subscription delivery doesn't need a correlation_id; route
            // by subscription_id from the envelope metadata.
            if let MessageType::SubscribeEvent(p) = &env.payload {
                if let Some(sub_id) = env.subscription_id.as_ref() {
                    if let Some(forwarder) = inner.active_subscriptions.get(sub_id) {
                        // The wrapped event is a JSON value; deserialise to
                        // an Envelope so the subscriber gets typed access.
                        if let Ok(inner_env) = serde_json::from_value::<Envelope>(p.event.clone()) {
                            let _ = forwarder.send(inner_env);
                        }
                    }
                }
                continue;
            }

            let Some(corr) = env.correlation_id.clone() else {
                continue;
            };
            match env.payload {
                MessageType::JobAccepted(JobAcceptedPayload { job_id, .. }) => {
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
                MessageType::ArtifactRef(crate::messages::ArtifactRefPayload { artifact }) => {
                    if let Some((_, tx)) = inner.pending_artifact.remove(&corr) {
                        let _ = tx.send(ArtifactReply::Ref(artifact));
                    }
                }
                MessageType::ArtifactPut(ArtifactPutPayload {
                    media_type, data, ..
                }) => {
                    if let Some((_, tx)) = inner.pending_artifact.remove(&corr) {
                        let _ = tx.send(ArtifactReply::Inline { data, media_type });
                    }
                }
                MessageType::Nack(payload) => {
                    // A nack might resolve a pending artifact request.
                    if let Some((_, tx)) = inner.pending_artifact.remove(&corr) {
                        let _ = tx.send(ArtifactReply::Nack(payload));
                    }
                }
                MessageType::SubscribeAccepted(p) => {
                    if let Some((_, tx)) = inner.pending_subscribe.remove(&corr) {
                        let _ = tx.send(p.subscription_id);
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
        let mut env = Envelope::new(MessageType::ToolInvoke(ToolInvokePayload::new(
            tool, arguments,
        )));
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

    /// Upload an artifact (RFC §16.2). Returns the canonical
    /// [`ArtifactRef`] the runtime minted.
    ///
    /// `data` must be base64-encoded; the caller is responsible for
    /// chunking inputs that exceed the runtime's inline cap.
    ///
    /// # Errors
    ///
    /// Returns [`ARCPError`] for transport failures, or whatever code the
    /// runtime returns in a `nack` (e.g. [`ErrorCode::InvalidArgument`]
    /// for malformed base64).
    pub async fn put_artifact(
        &self,
        media_type: impl Into<String>,
        data: impl Into<String>,
        retain_seconds: Option<u64>,
    ) -> Result<ArtifactRef, ARCPError> {
        let session_id = self.id().await?;
        let mut env = Envelope::new(MessageType::ArtifactPut(ArtifactPutPayload {
            media_type: media_type.into(),
            data: data.into(),
            sha256: None,
            retain_seconds,
        }));
        env.session_id = Some(session_id);
        let correlation_id = env.id.clone();

        let (tx, rx) = oneshot::channel::<ArtifactReply>();
        self.inner.pending_artifact.insert(correlation_id, tx);
        self.inner.transport.send(env).await?;

        match rx.await {
            Ok(ArtifactReply::Ref(reference)) => Ok(reference),
            Ok(ArtifactReply::Inline { .. }) => Err(ARCPError::Internal {
                detail: "expected artifact.ref, got inline body".into(),
            }),
            Ok(ArtifactReply::Nack(p)) => Err(map_nack(p)),
            Err(_) => Err(ARCPError::Unavailable {
                detail: "artifact.put response channel dropped".into(),
            }),
        }
    }

    /// Fetch an artifact by id. Returns `(base64_body, media_type)`.
    ///
    /// # Errors
    ///
    /// Returns [`ARCPError::NotFound`] when the runtime has no such id;
    /// [`ARCPError::Unavailable`] for transport failures.
    pub async fn fetch_artifact(
        &self,
        artifact_id: ArtifactId,
    ) -> Result<(String, String), ARCPError> {
        let session_id = self.id().await?;
        let mut env = Envelope::new(MessageType::ArtifactFetch(ArtifactFetchPayload {
            artifact_id,
        }));
        env.session_id = Some(session_id);
        let correlation_id = env.id.clone();

        let (tx, rx) = oneshot::channel::<ArtifactReply>();
        self.inner.pending_artifact.insert(correlation_id, tx);
        self.inner.transport.send(env).await?;

        match rx.await {
            Ok(ArtifactReply::Inline { data, media_type }) => Ok((data, media_type)),
            Ok(ArtifactReply::Ref(_)) => Err(ARCPError::Internal {
                detail: "expected inline body, got artifact.ref".into(),
            }),
            Ok(ArtifactReply::Nack(p)) => Err(map_nack(p)),
            Err(_) => Err(ARCPError::Unavailable {
                detail: "artifact.fetch response channel dropped".into(),
            }),
        }
    }

    /// Release (delete) an artifact (RFC §16.2). The runtime does not
    /// acknowledge releases; this is fire-and-forget.
    ///
    /// # Errors
    ///
    /// Returns [`ARCPError::Unavailable`] for transport failures.
    pub async fn release_artifact(&self, artifact_id: ArtifactId) -> Result<(), ARCPError> {
        let session_id = self.id().await?;
        let mut env = Envelope::new(MessageType::ArtifactRelease(ArtifactReleasePayload {
            artifact_id,
        }));
        env.session_id = Some(session_id);
        self.inner.transport.send(env).await
    }

    /// Subscribe to runtime events (RFC §13). Returns a
    /// [`SubscriptionHandle`] yielding live envelopes that match `filter`.
    ///
    /// # Errors
    ///
    /// Returns [`ARCPError::Unavailable`] if the transport closes before
    /// the runtime acknowledges the subscription.
    pub async fn subscribe(
        &self,
        filter: SubscriptionFilter,
    ) -> Result<SubscriptionHandle, ARCPError> {
        let session_id = self.id().await?;
        let mut env = Envelope::new(MessageType::Subscribe(SubscribePayload {
            filter,
            since: None,
        }));
        env.session_id = Some(session_id.clone());
        let correlation_id = env.id.clone();

        let (acc_tx, acc_rx) = oneshot::channel::<SubscriptionId>();
        self.inner.pending_subscribe.insert(correlation_id, acc_tx);
        self.inner.transport.send(env).await?;

        let subscription_id = acc_rx.await.map_err(|_| ARCPError::Unavailable {
            detail: "runtime closed before subscribe.accepted".into(),
        })?;

        let (fwd_tx, fwd_rx) = mpsc::unbounded_channel::<Envelope>();
        self.inner
            .active_subscriptions
            .insert(subscription_id.clone(), fwd_tx);
        Ok(SubscriptionHandle {
            subscription_id,
            session_id,
            transport: Arc::clone(&self.inner.transport),
            inbox: Mutex::new(fwd_rx),
            forwarders: Arc::clone(&self.inner.active_subscriptions),
        })
    }
}

/// Handle to a live subscription (RFC §13).
///
/// Dropping the handle removes the client-side forwarder and stops local
/// delivery; it does **not** send an `unsubscribe` envelope. Call
/// [`Self::unsubscribe`] to shut down gracefully with an explicit
/// `unsubscribe` on the wire.
pub struct SubscriptionHandle {
    /// The subscription's id.
    pub subscription_id: SubscriptionId,
    session_id: SessionId,
    transport: Arc<dyn Transport>,
    inbox: Mutex<mpsc::UnboundedReceiver<Envelope>>,
    forwarders: Arc<DashMap<SubscriptionId, mpsc::UnboundedSender<Envelope>>>,
}

impl std::fmt::Debug for SubscriptionHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SubscriptionHandle")
            .field("subscription_id", &self.subscription_id)
            .finish_non_exhaustive()
    }
}

impl SubscriptionHandle {
    /// Receive the next envelope, or `None` when the subscription is
    /// torn down.
    pub async fn next(&self) -> Option<Envelope> {
        self.inbox.lock().await.recv().await
    }

    /// Send an `unsubscribe` envelope and detach locally. The handle is
    /// also detached on Drop, but explicit `unsubscribe` is the polite
    /// shutdown.
    ///
    /// # Errors
    ///
    /// Returns [`ARCPError::Unavailable`] if the transport is already
    /// closed.
    pub async fn unsubscribe(self) -> Result<(), ARCPError> {
        let mut env = Envelope::new(MessageType::Unsubscribe(UnsubscribePayload {
            subscription_id: self.subscription_id.clone(),
        }));
        env.session_id = Some(self.session_id.clone());
        let result = self.transport.send(env).await;
        // Drop will run after this returns and remove the forwarder slot.
        result
    }
}

impl Drop for SubscriptionHandle {
    fn drop(&mut self) {
        self.forwarders.remove(&self.subscription_id);
    }
}

#[allow(dead_code)] // SubscriptionSince is wired through Phase 5 follow-up.
fn _since_marker(_x: SubscriptionSince) {}

fn map_nack(p: NackPayload) -> ARCPError {
    match p.code {
        ErrorCode::NotFound => ARCPError::NotFound {
            kind: "artifact",
            id: p.message,
        },
        ErrorCode::InvalidArgument => ARCPError::InvalidArgument { detail: p.message },
        other => ARCPError::Unknown {
            detail: format!("nack ({other}): {}", p.message),
        },
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
        }
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
                pending_artifact: DashMap::new(),
                active_subscriptions: Arc::new(DashMap::new()),
                pending_subscribe: DashMap::new(),
                reader: Mutex::new(None),
                _transport_kind: PhantomData,
            }),
            _state: PhantomData,
        })
    }
}

