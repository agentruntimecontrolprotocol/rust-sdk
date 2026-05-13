//! ARCP runtime — the server side that drives the handshake (RFC §8.1)
//! and dispatches subsequent envelopes.

use std::collections::HashSet;
use std::sync::Arc;

use dashmap::DashMap;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use super::artifact::ArtifactStore;
use super::context::{HumanResponse, ToolContext};
use super::job::{JobEntry, JobRegistry};
use super::session::{HandshakePhase, SessionState};
use super::subscription::SubscriptionManager;
use super::tools::ToolRegistry;
use crate::auth::{AuthOutcome, AuthRegistry, Authenticator};
use crate::envelope::Envelope;
use crate::error::{ARCPError, ErrorCode};
use crate::extensions::ExtensionRegistry;
use crate::ids::SubscriptionId;
use crate::ids::{JobId, MessageId, SessionId};
use crate::messages::{
    ArtifactFetchPayload, ArtifactPutPayload, ArtifactRefPayload, ArtifactReleasePayload,
    CancelPayload, CancelTargetKind, Capabilities, HumanChoiceResponsePayload,
    HumanInputCancelledPayload, HumanInputResponsePayload, JobAcceptedPayload, JobCancelledPayload,
    JobCompletedPayload, JobFailedPayload, JobStartedPayload, JobState, MessageType, NackPayload,
    RuntimeIdentity, SessionAcceptedPayload, SessionLease, SessionOpenPayload,
    SessionRejectedPayload, SessionUnauthenticatedPayload, SubscribeAcceptedPayload,
    SubscribeEventPayload, SubscribePayload, ToolInvokePayload, UnsubscribePayload,
};
use crate::store::eventlog::EventLog;
use crate::transport::Transport;
use crate::{IMPL_KIND, IMPL_VERSION};

/// Runtime configuration.
pub struct RuntimeBuilder {
    auth: AuthRegistry,
    tools: ToolRegistry,
    advertised_capabilities: Capabilities,
    runtime_identity: RuntimeIdentity,
    session_lease_seconds: Option<u64>,
}

impl Default for RuntimeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for RuntimeBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RuntimeBuilder")
            .field("advertised_capabilities", &self.advertised_capabilities)
            .field("runtime_identity", &self.runtime_identity)
            .field("session_lease_seconds", &self.session_lease_seconds)
            .finish_non_exhaustive()
    }
}

impl RuntimeBuilder {
    /// New builder with empty auth registry, default capabilities, and the
    /// crate's identity (`arcp-rs`).
    #[must_use]
    pub fn new() -> Self {
        Self {
            auth: AuthRegistry::new(),
            tools: ToolRegistry::new(),
            advertised_capabilities: Capabilities::default(),
            runtime_identity: RuntimeIdentity {
                kind: IMPL_KIND.to_owned(),
                version: IMPL_VERSION.to_owned(),
                fingerprint: None,
                trust_level: Some("trusted".into()),
            },
            session_lease_seconds: Some(3600),
        }
    }

    /// Register one authenticator. Multiple may be added (one per scheme).
    #[must_use]
    pub fn with_authenticator(mut self, auth: Box<dyn Authenticator>) -> Self {
        self.auth.register(auth);
        self
    }

    /// Set the tool registry (replaces any previously set).
    #[must_use]
    pub fn with_tools(mut self, tools: ToolRegistry) -> Self {
        self.tools = tools;
        self
    }

    /// Set the capability set the runtime advertises.
    #[must_use]
    pub fn with_capabilities(mut self, caps: Capabilities) -> Self {
        self.advertised_capabilities = caps;
        self
    }

    /// Override the runtime identity.
    #[must_use]
    pub fn with_identity(mut self, ident: RuntimeIdentity) -> Self {
        self.runtime_identity = ident;
        self
    }

    /// Override the default session lease length.
    #[must_use]
    pub const fn with_session_lease_seconds(mut self, seconds: u64) -> Self {
        self.session_lease_seconds = Some(seconds);
        self
    }

    /// Construct an [`ARCPRuntime`] sharing this configuration. The
    /// returned runtime is cheap to clone.
    ///
    /// # Errors
    ///
    /// Returns [`ARCPError::Storage`] if the in-memory event log cannot be
    /// initialised (extremely unlikely; signals `SQLite` link failure).
    pub async fn build(self) -> Result<ARCPRuntime, ARCPError> {
        let event_log = EventLog::in_memory().await?;
        Ok(ARCPRuntime {
            inner: Arc::new(RuntimeInner {
                auth: self.auth,
                tools: self.tools,
                advertised_capabilities: self.advertised_capabilities,
                runtime_identity: self.runtime_identity,
                session_lease_seconds: self.session_lease_seconds,
                extension_registry: ExtensionRegistry::new(),
                event_log,
                artifacts: ArtifactStore::new(),
                subscriptions: SubscriptionManager::new(),
            }),
        })
    }
}

struct RuntimeInner {
    auth: AuthRegistry,
    tools: ToolRegistry,
    advertised_capabilities: Capabilities,
    runtime_identity: RuntimeIdentity,
    session_lease_seconds: Option<u64>,
    #[allow(dead_code)]
    extension_registry: ExtensionRegistry,
    event_log: EventLog,
    artifacts: ArtifactStore,
    subscriptions: SubscriptionManager,
}

/// The ARCP runtime. Cheap to clone; share across tasks.
#[derive(Clone)]
pub struct ARCPRuntime {
    inner: Arc<RuntimeInner>,
}

impl std::fmt::Debug for ARCPRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ARCPRuntime").finish_non_exhaustive()
    }
}

impl ARCPRuntime {
    /// Construct via [`RuntimeBuilder`].
    #[must_use]
    pub fn builder() -> RuntimeBuilder {
        RuntimeBuilder::new()
    }

    /// Borrow the runtime's event log.
    #[must_use]
    pub fn event_log(&self) -> &EventLog {
        &self.inner.event_log
    }

    /// Borrow the runtime's artifact store.
    #[must_use]
    pub fn artifacts(&self) -> &ArtifactStore {
        &self.inner.artifacts
    }

    /// Borrow the runtime's subscription manager.
    #[must_use]
    pub fn subscriptions(&self) -> &SubscriptionManager {
        &self.inner.subscriptions
    }

    /// Spawn a per-connection task that drives the handshake and then
    /// dispatches subsequent envelopes. The returned [`JoinHandle`] is
    /// owned by the caller — Phase 2 doesn't yet integrate with a
    /// connection registry.
    #[must_use]
    pub fn serve_connection<T: Transport + 'static>(&self, transport: T) -> JoinHandle<()> {
        let runtime = self.clone();
        tokio::spawn(async move {
            if let Err(e) = runtime.run_connection(transport).await {
                tracing::warn!(error = %e, "connection terminated with error");
            }
        })
    }

    /// Drive one connection synchronously (in the caller's task).
    ///
    /// # Errors
    ///
    /// Returns [`ARCPError`] for transport / serialisation failures or for
    /// internal protocol errors (rare).
    #[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
    pub async fn run_connection<T: Transport + 'static>(
        &self,
        transport: T,
    ) -> Result<(), ARCPError> {
        let transport = Arc::new(transport);
        // Out-going envelope channel — both the dispatcher and per-job
        // tasks publish here. A dedicated writer task owns the transport
        // send side so we never have two callers contending on it.
        let (out_tx, mut out_rx) = mpsc::channel::<Envelope>(256);
        let writer_transport = Arc::clone(&transport);
        let event_log = self.inner.event_log.clone();
        let writer_subs = self.inner.subscriptions.clone();
        let writer = tokio::spawn(async move {
            while let Some(env) = out_rx.recv().await {
                if let Err(e) = event_log.append(&env).await {
                    tracing::warn!(error = %e, "failed to persist outbound envelope");
                }
                // Publish outbound envelopes too so subscribers see
                // job.* / tool.* / stream.* events that originate on the
                // server side (RFC §13). Skip subscribe.event itself so
                // the wrapper isn't re-broadcast, which would cause an
                // echo storm whenever a filter matches subscribe.event.
                if !matches!(env.payload, MessageType::SubscribeEvent(_)) {
                    let _ = writer_subs.publish(&env);
                }
                if let Err(e) = writer_transport.send(env).await {
                    tracing::warn!(error = %e, "transport send failed; closing writer");
                    break;
                }
            }
        });

        let connection_token = CancellationToken::new();
        let jobs = JobRegistry::new();
        let pending_human: Arc<DashMap<MessageId, oneshot::Sender<HumanResponse>>> =
            Arc::new(DashMap::new());
        // Subscriptions owned by this connection, so we can drop them on
        // close even if the SubscriptionManager is shared across sessions.
        let connection_subs: Arc<DashMap<SubscriptionId, JoinHandle<()>>> =
            Arc::new(DashMap::new());
        let mut state: Option<SessionState> = None;
        let mut seen_ids: HashSet<MessageId> = HashSet::new();

        let result = loop {
            let Some(envelope) = transport.recv().await? else {
                break Ok(());
            };

            // Transport-level idempotency check.
            if !seen_ids.insert(envelope.id.clone()) {
                tracing::debug!(id = %envelope.id, "dropping replayed envelope");
                continue;
            }

            // Persist incoming envelope.
            self.inner.event_log.append(&envelope).await?;
            // Publish to subscribers (lossy on backpressure).
            let _ = self.inner.subscriptions.publish(&envelope);

            let in_handshake = state.as_ref().is_none_or(|s| !s.is_accepted());
            if in_handshake && !envelope.payload.is_handshake() {
                tracing::warn!(
                    id = %envelope.id,
                    type_name = envelope.payload.type_name(),
                    "dropping non-handshake message before session.accepted",
                );
                continue;
            }

            match envelope.payload.clone() {
                MessageType::SessionOpen(payload) => {
                    state = Some(
                        self.handle_session_open(&out_tx, envelope.id.clone(), payload)
                            .await?,
                    );
                }
                MessageType::SessionAuthenticate(payload) => {
                    if let Some(s) = state.as_mut() {
                        self.handle_session_authenticate(
                            &out_tx,
                            envelope.id.clone(),
                            s,
                            &payload.response,
                        )
                        .await?;
                    } else {
                        tracing::warn!("session.authenticate before session.open; dropping");
                    }
                }
                MessageType::SessionClose(_) => {
                    tracing::info!("session.close received");
                    break Ok(());
                }
                MessageType::ToolInvoke(payload) => {
                    if let Some(s) = state.as_ref() {
                        self.spawn_tool_invoke(
                            &out_tx,
                            &jobs,
                            &pending_human,
                            &connection_token,
                            envelope.id.clone(),
                            s.session_id.clone(),
                            payload,
                        )
                        .await;
                    }
                }
                MessageType::Cancel(payload) => {
                    self.handle_cancel(&out_tx, &jobs, envelope.id.clone(), &payload)
                        .await;
                }
                MessageType::HumanInputResponse(HumanInputResponsePayload { value, .. }) => {
                    if let Some(corr) = envelope.correlation_id.clone() {
                        if let Some((_, tx)) = pending_human.remove(&corr) {
                            let _ = tx.send(HumanResponse::Value(value));
                        }
                    }
                }
                MessageType::HumanChoiceResponse(HumanChoiceResponsePayload {
                    choice_id, ..
                }) => {
                    if let Some(corr) = envelope.correlation_id.clone() {
                        if let Some((_, tx)) = pending_human.remove(&corr) {
                            let _ = tx.send(HumanResponse::Choice(choice_id));
                        }
                    }
                }
                MessageType::HumanInputCancelled(HumanInputCancelledPayload { code, .. }) => {
                    if let Some(corr) = envelope.correlation_id.clone() {
                        if let Some((_, tx)) = pending_human.remove(&corr) {
                            let _ = tx.send(HumanResponse::Cancelled(code));
                        }
                    }
                }
                MessageType::Ping(_) => {
                    let mut env =
                        Envelope::new(MessageType::Pong(crate::messages::PongPayload::default()));
                    env.correlation_id = Some(envelope.id.clone());
                    if let Some(s) = state.as_ref() {
                        env.session_id = Some(s.session_id.clone());
                    }
                    let _ = out_tx.send(env).await;
                }
                MessageType::Subscribe(payload) => {
                    if let Some(s) = state.as_ref() {
                        Self::handle_subscribe(
                            &out_tx,
                            &self.inner.subscriptions,
                            &connection_subs,
                            envelope.id.clone(),
                            s.session_id.clone(),
                            payload,
                        )
                        .await;
                    }
                }
                MessageType::Unsubscribe(UnsubscribePayload { subscription_id }) => {
                    if let Some((_, join)) = connection_subs.remove(&subscription_id) {
                        join.abort();
                    }
                    let _ = self.inner.subscriptions.unsubscribe(&subscription_id);
                }
                MessageType::ArtifactPut(payload) => {
                    if let Some(s) = state.as_ref() {
                        Self::handle_artifact_put(
                            &out_tx,
                            &self.inner.artifacts,
                            envelope.id.clone(),
                            s.session_id.clone(),
                            payload,
                        )
                        .await;
                    }
                }
                MessageType::ArtifactFetch(payload) => {
                    if let Some(s) = state.as_ref() {
                        Self::handle_artifact_fetch(
                            &out_tx,
                            &self.inner.artifacts,
                            envelope.id.clone(),
                            s.session_id.clone(),
                            payload,
                        )
                        .await;
                    }
                }
                MessageType::ArtifactRelease(ArtifactReleasePayload { artifact_id }) => {
                    self.inner.artifacts.release(&artifact_id);
                }
                _ if in_handshake => {
                    tracing::warn!(
                        type_name = envelope.payload.type_name(),
                        "unexpected handshake message direction",
                    );
                }
                _ => {
                    tracing::debug!(
                        type_name = envelope.payload.type_name(),
                        "dispatch arm not yet implemented",
                    );
                }
            }
        };

        // Tear down: cancel all jobs, abort all subscription forwarder
        // tasks, drop the out_tx so the writer drains remaining envelopes,
        // then await the writer.
        connection_token.cancel();
        for r in jobs.inner_iter() {
            r.cancel();
        }
        for entry in connection_subs.iter() {
            entry.value().abort();
        }
        connection_subs.clear();
        drop(out_tx);
        let _ = writer.await;
        result
    }

    async fn handle_session_open(
        &self,
        out: &mpsc::Sender<Envelope>,
        correlation_id: MessageId,
        payload: SessionOpenPayload,
    ) -> Result<SessionState, ARCPError> {
        let SessionOpenPayload {
            auth,
            client,
            capabilities: client_caps,
        } = payload;

        let negotiated = self.negotiate_capabilities(&client_caps);
        let session_id = SessionId::new();
        let mut state = SessionState::new(session_id.clone(), negotiated.clone());

        let Some(authenticator) = self.inner.auth.get(&auth.scheme) else {
            self.send_rejected(
                out,
                correlation_id,
                ErrorCode::Unauthenticated,
                format!("auth scheme {:?} not configured", auth.scheme),
            )
            .await;
            state.phase = HandshakePhase::Closed;
            return Ok(state);
        };

        let outcome = authenticator
            .authenticate(&auth, &client, &negotiated)
            .await?;

        match outcome {
            AuthOutcome::Accept { principal } => {
                state.principal = Some(principal);
                state.phase = HandshakePhase::Accepted;
                self.send_accepted(out, correlation_id, &session_id, &negotiated)
                    .await;
            }
            AuthOutcome::Challenge { challenge } => {
                state.active_challenge = Some(challenge.clone());
                state.phase = HandshakePhase::Challenged;
                let mut env = Envelope::new(MessageType::SessionChallenge(
                    crate::messages::SessionChallengePayload {
                        challenge: challenge.clone(),
                    },
                ));
                env.correlation_id = Some(correlation_id);
                env.session_id = Some(session_id);
                let _ = out.send(env).await;
            }
            AuthOutcome::Reject { reason } => {
                self.send_rejected(out, correlation_id, ErrorCode::Unauthenticated, reason)
                    .await;
                state.phase = HandshakePhase::Closed;
            }
        }
        Ok(state)
    }

    async fn handle_session_authenticate(
        &self,
        out: &mpsc::Sender<Envelope>,
        correlation_id: MessageId,
        state: &mut SessionState,
        response: &str,
    ) -> Result<(), ARCPError> {
        let Some(challenge) = state.active_challenge.clone() else {
            tracing::warn!("session.authenticate without active challenge; dropping");
            return Ok(());
        };
        for scheme in [
            crate::messages::AuthScheme::Bearer,
            crate::messages::AuthScheme::SignedJwt,
        ] {
            let Some(authenticator) = self.inner.auth.get(&scheme) else {
                continue;
            };
            let outcome = authenticator
                .verify_challenge_response(&challenge, response)
                .await?;
            match outcome {
                AuthOutcome::Accept { principal } => {
                    state.principal = Some(principal);
                    state.phase = HandshakePhase::Accepted;
                    state.active_challenge = None;
                    self.send_accepted(out, correlation_id, &state.session_id, &state.capabilities)
                        .await;
                    return Ok(());
                }
                AuthOutcome::Challenge { .. } | AuthOutcome::Reject { .. } => {}
            }
        }
        let mut env = Envelope::new(MessageType::SessionUnauthenticated(
            SessionUnauthenticatedPayload {
                code: ErrorCode::Unauthenticated,
                message: "challenge response did not validate".into(),
            },
        ));
        env.correlation_id = Some(correlation_id);
        env.session_id = Some(state.session_id.clone());
        let _ = out.send(env).await;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn spawn_tool_invoke(
        &self,
        out: &mpsc::Sender<Envelope>,
        jobs: &JobRegistry,
        pending_human: &Arc<DashMap<MessageId, oneshot::Sender<HumanResponse>>>,
        connection_token: &CancellationToken,
        correlation_id: MessageId,
        session_id: SessionId,
        payload: ToolInvokePayload,
    ) {
        let job_id = JobId::new();

        // job.accepted
        let mut accepted = Envelope::new(MessageType::JobAccepted(JobAcceptedPayload {
            job_id: job_id.clone(),
        }));
        accepted.correlation_id = Some(correlation_id.clone());
        accepted.session_id = Some(session_id.clone());
        accepted.job_id = Some(job_id.clone());
        let _ = out.send(accepted).await;

        let Some(handler) = self.inner.tools.get(&payload.tool) else {
            let mut err = Envelope::new(MessageType::JobFailed(JobFailedPayload {
                code: ErrorCode::NotFound,
                retryable: Some(false),
                message: format!("tool not registered: {}", payload.tool),
                details: None,
            }));
            err.correlation_id = Some(correlation_id);
            err.session_id = Some(session_id);
            err.job_id = Some(job_id);
            let _ = out.send(err).await;
            return;
        };

        let cancel = connection_token.child_token();
        let entry = JobEntry {
            job_id: job_id.clone(),
            session_id: session_id.clone(),
            correlation_id: correlation_id.clone(),
            cancel: cancel.clone(),
            state: JobState::Accepted,
        };

        let out_clone = out.clone();
        let jobs_clone = jobs.clone();
        let pending_human_clone = Arc::clone(pending_human);
        let cancel_for_task = cancel;

        let join = tokio::spawn(async move {
            // job.started
            let mut started = Envelope::new(MessageType::JobStarted(JobStartedPayload {
                description: Some(format!("invoking {}", payload.tool)),
            }));
            started.correlation_id = Some(correlation_id.clone());
            started.session_id = Some(session_id.clone());
            started.job_id = Some(job_id.clone());
            let _ = out_clone.send(started).await;
            jobs_clone.set_state(&job_id, JobState::Running);

            let ctx = ToolContext {
                cancel: cancel_for_task.clone(),
                job_id: job_id.clone(),
                session_id: session_id.clone(),
                correlation_id: correlation_id.clone(),
                out: out_clone.clone(),
                pending_human: pending_human_clone,
            };

            let outcome = tokio::select! {
                () = cancel_for_task.cancelled() => Outcome::Cancelled("cancellation token fired".into()),
                result = handler.invoke(payload.arguments, ctx) => match result {
                    Ok(value) => Outcome::Completed(value),
                    Err(ARCPError::Cancelled { reason }) => Outcome::Cancelled(reason),
                    Err(e) => Outcome::Failed(e),
                },
            };

            let terminal = match outcome {
                Outcome::Completed(value) => {
                    jobs_clone.set_state(&job_id, JobState::Completed);
                    MessageType::JobCompleted(JobCompletedPayload {
                        value: Some(value),
                        result_ref: None,
                    })
                }
                Outcome::Failed(e) => {
                    jobs_clone.set_state(&job_id, JobState::Failed);
                    MessageType::JobFailed(JobFailedPayload {
                        code: e.code(),
                        retryable: Some(e.retryable()),
                        message: e.to_string(),
                        details: None,
                    })
                }
                Outcome::Cancelled(reason) => {
                    jobs_clone.set_state(&job_id, JobState::Cancelled);
                    MessageType::JobCancelled(JobCancelledPayload {
                        reason: Some(reason),
                    })
                }
            };
            let mut term = Envelope::new(terminal);
            term.correlation_id = Some(correlation_id);
            term.session_id = Some(session_id);
            term.job_id = Some(job_id);
            let _ = out_clone.send(term).await;
        });

        jobs.insert(entry, join);
    }

    async fn handle_cancel(
        &self,
        out: &mpsc::Sender<Envelope>,
        jobs: &JobRegistry,
        correlation_id: MessageId,
        payload: &CancelPayload,
    ) {
        let CancelPayload {
            target, target_id, ..
        } = payload;
        match target {
            CancelTargetKind::Job => {
                #[allow(clippy::option_if_let_else)] // map_or_else nests too deeply here
                let response_payload = if let Ok(job_id) = target_id.parse::<JobId>() {
                    if jobs.cancel(&job_id) {
                        MessageType::CancelAccepted(crate::messages::CancelAcceptedPayload {
                            target_id: Some(target_id.clone()),
                        })
                    } else {
                        MessageType::CancelRefused(crate::messages::CancelRefusedPayload {
                            target_id: target_id.clone(),
                            reason: "no such in-flight job".into(),
                        })
                    }
                } else {
                    MessageType::CancelRefused(crate::messages::CancelRefusedPayload {
                        target_id: target_id.clone(),
                        reason: "malformed job id".into(),
                    })
                };
                let mut env = Envelope::new(response_payload);
                env.correlation_id = Some(correlation_id);
                let _ = out.send(env).await;
            }
            CancelTargetKind::Stream | CancelTargetKind::Session => {
                tracing::warn!(?target, "cancel target not yet implemented");
            }
        }
    }

    fn negotiate_capabilities(&self, client_caps: &Capabilities) -> Capabilities {
        // Intersection: a capability is enabled only if both sides set it.
        let runtime_caps = &self.inner.advertised_capabilities;
        Capabilities {
            streaming: intersect_bool(runtime_caps.streaming, client_caps.streaming),
            durable_jobs: intersect_bool(runtime_caps.durable_jobs, client_caps.durable_jobs),
            checkpoints: intersect_bool(runtime_caps.checkpoints, client_caps.checkpoints),
            binary_streams: intersect_bool(runtime_caps.binary_streams, client_caps.binary_streams),
            agent_handoff: intersect_bool(runtime_caps.agent_handoff, client_caps.agent_handoff),
            human_input: intersect_bool(runtime_caps.human_input, client_caps.human_input),
            artifacts: intersect_bool(runtime_caps.artifacts, client_caps.artifacts),
            subscriptions: intersect_bool(runtime_caps.subscriptions, client_caps.subscriptions),
            scheduled_jobs: intersect_bool(runtime_caps.scheduled_jobs, client_caps.scheduled_jobs),
            interrupt: intersect_bool(runtime_caps.interrupt, client_caps.interrupt),
            anonymous: intersect_bool(runtime_caps.anonymous, client_caps.anonymous),
            heartbeat_recovery: runtime_caps.heartbeat_recovery.clone(),
            binary_encoding: runtime_caps.binary_encoding.clone(),
            extensions: runtime_caps
                .extensions
                .iter()
                .filter(|e| client_caps.extensions.contains(e))
                .cloned()
                .collect(),
            artifact_retention: runtime_caps.artifact_retention.clone(),
            extra: std::collections::BTreeMap::new(),
        }
    }

    async fn send_accepted(
        &self,
        out: &mpsc::Sender<Envelope>,
        correlation_id: MessageId,
        session_id: &SessionId,
        capabilities: &Capabilities,
    ) {
        let lease = self.inner.session_lease_seconds.map(|s| SessionLease {
            expires_at: chrono::Utc::now()
                + chrono::Duration::seconds(i64::try_from(s).unwrap_or(i64::MAX)),
        });
        let mut env = Envelope::new(MessageType::SessionAccepted(SessionAcceptedPayload {
            session_id: session_id.clone(),
            runtime: self.inner.runtime_identity.clone(),
            capabilities: capabilities.clone(),
            lease,
        }));
        env.correlation_id = Some(correlation_id);
        env.session_id = Some(session_id.clone());
        let _ = out.send(env).await;
    }

    async fn send_rejected(
        &self,
        out: &mpsc::Sender<Envelope>,
        correlation_id: MessageId,
        code: ErrorCode,
        message: String,
    ) {
        let mut env = Envelope::new(MessageType::SessionRejected(SessionRejectedPayload {
            code,
            message,
        }));
        env.correlation_id = Some(correlation_id);
        let _ = out.send(env).await;
    }

    async fn handle_subscribe(
        out: &mpsc::Sender<Envelope>,
        manager: &SubscriptionManager,
        connection_subs: &Arc<DashMap<SubscriptionId, JoinHandle<()>>>,
        correlation_id: MessageId,
        session_id: SessionId,
        payload: SubscribePayload,
    ) {
        let SubscribePayload { filter, since: _ } = payload;
        // PLAN.md §A4.10 reserves richer authorisation; for v0.1 we accept
        // any filter from an authenticated session.
        let (subscription_id, mut rx) = manager.register(filter, session_id.clone());
        // Acknowledge the subscription.
        let mut accepted =
            Envelope::new(MessageType::SubscribeAccepted(SubscribeAcceptedPayload {
                subscription_id: subscription_id.clone(),
            }));
        accepted.correlation_id = Some(correlation_id);
        accepted.session_id = Some(session_id);
        accepted.subscription_id = Some(subscription_id.clone());
        let _ = out.send(accepted).await;

        // Spawn a forwarder task that wraps each delivered envelope in a
        // subscribe.event and pushes to the outbound channel. Backfill
        // (the §13.3 boundary marker) is left for a follow-up.
        let out_clone = out.clone();
        let sub_id = subscription_id.clone();
        let join = tokio::spawn(async move {
            while let Some(event) = rx.next().await {
                let value = match serde_json::to_value(&event) {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::warn!(error = %e, "subscribe.event serialise failed");
                        continue;
                    }
                };
                let mut wrapper =
                    Envelope::new(MessageType::SubscribeEvent(SubscribeEventPayload {
                        event: value,
                    }));
                wrapper.subscription_id = Some(sub_id.clone());
                if out_clone.send(wrapper).await.is_err() {
                    break;
                }
            }
        });
        connection_subs.insert(subscription_id, join);
    }

    async fn handle_artifact_put(
        out: &mpsc::Sender<Envelope>,
        store: &ArtifactStore,
        correlation_id: MessageId,
        session_id: SessionId,
        payload: ArtifactPutPayload,
    ) {
        let ArtifactPutPayload {
            media_type,
            data,
            sha256,
            retain_seconds,
        } = payload;
        let mut env = match store.put(media_type, &data, retain_seconds, sha256) {
            Ok(reference) => Envelope::new(MessageType::ArtifactRef(ArtifactRefPayload {
                artifact: reference,
            })),
            Err(e) => Envelope::new(MessageType::Nack(NackPayload {
                code: e.code(),
                message: e.to_string(),
                details: None,
            })),
        };
        env.correlation_id = Some(correlation_id);
        env.session_id = Some(session_id);
        let _ = out.send(env).await;
    }

    async fn handle_artifact_fetch(
        out: &mpsc::Sender<Envelope>,
        store: &ArtifactStore,
        correlation_id: MessageId,
        session_id: SessionId,
        payload: ArtifactFetchPayload,
    ) {
        let ArtifactFetchPayload { artifact_id } = payload;
        let mut env = match store.fetch(&artifact_id) {
            Ok((data, media_type)) => Envelope::new(MessageType::ArtifactPut(ArtifactPutPayload {
                media_type,
                data,
                sha256: None,
                retain_seconds: None,
            })),
            Err(e) => Envelope::new(MessageType::Nack(NackPayload {
                code: e.code(),
                message: e.to_string(),
                details: None,
            })),
        };
        env.correlation_id = Some(correlation_id);
        env.session_id = Some(session_id);
        let _ = out.send(env).await;
    }
}

enum Outcome {
    Completed(serde_json::Value),
    Failed(ARCPError),
    Cancelled(String),
}

/// Intersect two boolean capability slots.
///
/// Returns `None` only when neither side advertised the capability — in
/// that case the field is elided on the wire, matching RFC §7's "absent =
/// false" semantics. When at least one side advertised, the result is
/// `Some(both_set)`.
const fn intersect_bool(a: Option<bool>, b: Option<bool>) -> Option<bool> {
    match (a, b) {
        (Some(true), Some(true)) => Some(true),
        (Some(_), _) | (_, Some(_)) => Some(false),
        (None, None) => None,
    }
}
