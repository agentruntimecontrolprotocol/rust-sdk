//! ARCP runtime — the server side that drives the handshake (RFC §8.1)
//! and dispatches subsequent envelopes.

use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use dashmap::DashMap;
use tokio::sync::{mpsc, Notify};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use super::artifact::ArtifactStore;
use super::context::ToolContext;
use super::credentials::{
    revoke_all_for_job, CredentialJobContext, CredentialLedger, CredentialProvisioner,
};
use super::job::{JobEntry, JobRegistry};
use super::session::{HandshakePhase, SessionState};
use super::subscription::SubscriptionManager;
use super::tools::ToolRegistry;
use crate::store::eventlog::EventLog;
use arcp_core::auth::{AuthOutcome, AuthRegistry, Authenticator};
use arcp_core::envelope::Envelope;
use arcp_core::error::{ARCPError, ErrorCode};
use arcp_core::extensions::ExtensionRegistry;
use arcp_core::ids::IdempotencyKey;
use arcp_core::ids::SubscriptionId;
use arcp_core::ids::{JobId, MessageId, SessionId};
use arcp_core::messages::{
    ArtifactFetchPayload, ArtifactPutPayload, ArtifactRefPayload, ArtifactReleasePayload,
    CancelPayload, CancelTargetKind, Capabilities, JobAcceptedPayload, JobCancelledPayload,
    JobCompletedPayload, JobFailedPayload, JobStartedPayload, JobState, JobSubscribePayload,
    JobSubscribedPayload, JobUnsubscribePayload, LeaseRequest, MessageType, NackPayload,
    RuntimeIdentity, SessionAcceptedPayload, SessionLease, SessionOpenPayload,
    SessionRejectedPayload, SessionUnauthenticatedPayload, SubscribeAcceptedPayload,
    SubscribeEventPayload, SubscribePayload, ToolInvokePayload, UnsubscribePayload,
};
use arcp_core::transport::Transport;
use arcp_core::{IMPL_KIND, IMPL_VERSION};

/// Runtime configuration.
pub struct RuntimeBuilder {
    auth: AuthRegistry,
    tools: ToolRegistry,
    advertised_capabilities: Capabilities,
    runtime_identity: RuntimeIdentity,
    session_lease_seconds: Option<u64>,
    ack_window: Option<u64>,
    credential_provisioner: Option<Arc<dyn CredentialProvisioner>>,
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
            ack_window: None,
            credential_provisioner: None,
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

    /// Set the size of the `session.ack` sliding window (ARCP v1.1 §6.5).
    ///
    /// When set, the writer will pause outbound countable envelopes once
    /// `emitted - last_processed_seq >= window` and resume on the next
    /// `session.ack`. Set to `None` (default) to disable window-based
    /// flow control entirely.
    ///
    /// A window of `0` makes the gate immediately unsatisfiable for the
    /// very first countable event and is normalized to `None`
    /// (disabled) rather than installing a guaranteed deadlock.
    #[must_use]
    pub const fn with_ack_window(mut self, window: u64) -> Self {
        self.ack_window = if window == 0 { None } else { Some(window) };
        self
    }

    /// Register a provisioner for ARCP v1.1 lease-bound credentials.
    #[must_use]
    pub fn with_credential_provisioner(
        mut self,
        provisioner: Arc<dyn CredentialProvisioner>,
    ) -> Self {
        self.credential_provisioner = Some(provisioner);
        self.advertised_capabilities.model_use = Some(true);
        self.advertised_capabilities.provisioned_credentials = Some(true);
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
        if self.advertised_capabilities.provisioned_credentials == Some(true)
            && self.credential_provisioner.is_none()
        {
            return Err(ARCPError::FailedPrecondition {
                detail: "provisioned_credentials advertised without a CredentialProvisioner".into(),
            });
        }
        let event_log = EventLog::in_memory().await?;
        Ok(ARCPRuntime {
            inner: Arc::new(RuntimeInner {
                auth: self.auth,
                tools: self.tools,
                advertised_capabilities: self.advertised_capabilities,
                runtime_identity: self.runtime_identity,
                session_lease_seconds: self.session_lease_seconds,
                ack_window: self.ack_window,
                extension_registry: ExtensionRegistry::new(),
                event_log,
                artifacts: ArtifactStore::new(),
                subscriptions: SubscriptionManager::new(),
                jobs: JobRegistry::new(),
                session_principals: DashMap::new(),
                credential_provisioner: self.credential_provisioner,
                credential_ledger: CredentialLedger::new(),
                idempotency_index: DashMap::new(),
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
    /// Size of the `session.ack` sliding window, in countable events.
    /// `None` disables window-based flow control (default).
    ack_window: Option<u64>,
    #[allow(dead_code)]
    extension_registry: ExtensionRegistry,
    event_log: EventLog,
    artifacts: ArtifactStore,
    subscriptions: SubscriptionManager,
    /// Runtime-wide job registry. Shared across connections so a
    /// `job.subscribe` (ARCP v1.1 §7.6) from a different session can
    /// observe jobs submitted elsewhere.
    jobs: JobRegistry,
    /// Per-session authenticated principal. Used by `job.subscribe`
    /// authorization (default policy: same-principal as the submitter).
    session_principals: DashMap<SessionId, Option<String>>,
    /// Optional provisioner for lease-bound upstream credentials.
    credential_provisioner: Option<Arc<dyn CredentialProvisioner>>,
    /// Runtime ledger of outstanding credential ids.
    credential_ledger: CredentialLedger,
    /// Logical idempotency index for `tool.invoke` (ARCP v1.1 §6.4).
    /// Keyed by `(principal-or-session, idempotency_key)`; resolves a
    /// repeat command intent to the original `JobAccepted` payload so
    /// retries return the same `job_id` instead of starting a duplicate
    /// job.
    idempotency_index: DashMap<IdempotencyScope, IdempotencyRecord>,
}

/// Scope key for logical idempotency. Authenticated requests scope by
/// principal so a retry across a reconnect resolves to the same job;
/// anonymous sessions fall back to the session id.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct IdempotencyScope {
    principal_or_session: String,
    idempotency_key: IdempotencyKey,
}

#[derive(Debug, Clone)]
struct IdempotencyRecord {
    accepted: JobAcceptedPayload,
    tool: String,
    arguments_canonical: String,
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
        // ARCP v1.1 §6.5 flow-control state. `emitted` increments per
        // countable outbound envelope; `last_ack` is updated when the
        // client sends `session.ack`. The writer waits on `ack_notify`
        // when the in-flight window is full.
        let ack_window = self.inner.ack_window;
        let emitted = Arc::new(AtomicU64::new(0));
        let last_ack = Arc::new(AtomicU64::new(0));
        let ack_notify = Arc::new(Notify::new());
        let writer_emitted = Arc::clone(&emitted);
        let writer_last_ack = Arc::clone(&last_ack);
        let writer_ack_notify = Arc::clone(&ack_notify);
        let writer_jobs = self.inner.jobs.clone();
        let writer = tokio::spawn(async move {
            while let Some(mut env) = out_rx.recv().await {
                // Flow control (§6.5): for countable events, gate on the
                // sliding window BEFORE persistence / publishing so an
                // envelope blocked by backpressure isn't logged as
                // already delivered. Non-countable envelopes (handshake,
                // heartbeat, ack, control) bypass the gate.
                let is_countable = env.payload.is_countable_event();
                if is_countable {
                    if let Some(window) = ack_window {
                        loop {
                            let in_flight = writer_emitted
                                .load(Ordering::Acquire)
                                .saturating_sub(writer_last_ack.load(Ordering::Acquire));
                            if in_flight < window {
                                break;
                            }
                            // Wait for either a new ack or for the
                            // channel to close. run_connection drops
                            // out_tx and then notifies us so we can
                            // observe the closed channel and exit
                            // instead of parking forever (§6.5).
                            writer_ack_notify.notified().await;
                            if out_rx.is_closed() {
                                return;
                            }
                        }
                    }
                    // Stamp the session-scoped sequence number (§6.5 /
                    // §6.6). For job-scoped events, also bump the
                    // job's high-water mark so session.list_jobs and
                    // job.subscribed report the actual last value the
                    // subscriber can ack from.
                    let seq = writer_emitted.fetch_add(1, Ordering::AcqRel) + 1;
                    env.event_seq = Some(seq);
                    if let Some(job_id) = env.job_id.as_ref() {
                        writer_jobs.record_event_seq(job_id, seq);
                    }
                }
                if let Err(e) = event_log.append(&env).await {
                    tracing::warn!(error = %e, "failed to persist outbound envelope");
                }
                // Publish outbound envelopes too so subscribers see
                // job.* / tool.* / stream.* events that originate on the
                // server side (RFC §13). Skip subscribe.event itself so
                // the wrapper isn't re-broadcast, which would cause an
                // echo storm whenever a filter matches subscribe.event.
                if !matches!(env.payload, MessageType::SubscribeEvent(_)) {
                    let publish_env = redact_for_subscribers(&env);
                    let _ = writer_subs.publish(&publish_env);
                }
                if let Err(e) = writer_transport.send(env).await {
                    tracing::warn!(error = %e, "transport send failed; closing writer");
                    break;
                }
            }
        });

        let jobs = self.inner.jobs.clone();
        // Subscriptions owned by this connection, so we can drop them on
        // close even if the SubscriptionManager is shared across sessions.
        let connection_subs: Arc<DashMap<SubscriptionId, JoinHandle<()>>> =
            Arc::new(DashMap::new());
        // Per-connection `job.subscribe` (ARCP v1.1 §7.6) forwarders,
        // keyed by `job_id`.
        let connection_job_subs: Arc<DashMap<JobId, JoinHandle<()>>> = Arc::new(DashMap::new());
        let mut state: Option<SessionState> = None;
        let mut seen_ids: HashSet<MessageId> = HashSet::new();
        // ARCP v1.1 durable-job semantics (§10.1, README §"Reconnect"):
        // a normal transport drop must NOT cancel in-flight jobs. We
        // only tear down jobs when the client sends `session.close`.
        let mut explicit_close = false;

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
                    explicit_close = true;
                    break Ok(());
                }
                MessageType::ToolInvoke(payload) => {
                    if let Some(s) = state.as_ref() {
                        self.spawn_tool_invoke(
                            &out_tx,
                            &jobs,
                            envelope.id.clone(),
                            s.session_id.clone(),
                            s.principal.clone(),
                            envelope.idempotency_key.clone(),
                            payload,
                        )
                        .await;
                    }
                }
                MessageType::Cancel(payload) => {
                    if let Some(s) = state.as_ref() {
                        self.handle_cancel(&out_tx, &jobs, envelope.id.clone(), s, &payload)
                            .await;
                    }
                }
                MessageType::Ping(_) => {
                    let mut env = Envelope::new(MessageType::Pong(
                        arcp_core::messages::PongPayload::default(),
                    ));
                    env.correlation_id = Some(envelope.id.clone());
                    if let Some(s) = state.as_ref() {
                        env.session_id = Some(s.session_id.clone());
                    }
                    let _ = out_tx.send(env).await;
                }
                MessageType::SessionPing(payload) => {
                    // ARCP v1.1 §6.4: echo the nonce as `ping_nonce` in
                    // `session.pong` and stamp `received_at`.
                    let mut env = Envelope::new(MessageType::SessionPong(
                        arcp_core::messages::SessionPongPayload {
                            ping_nonce: payload.nonce,
                            received_at: chrono::Utc::now(),
                        },
                    ));
                    env.correlation_id = Some(envelope.id.clone());
                    if let Some(s) = state.as_ref() {
                        env.session_id = Some(s.session_id.clone());
                    }
                    let _ = out_tx.send(env).await;
                }
                MessageType::SessionPong(_) => {
                    // Heartbeat replies are observed by the client driver
                    // (see `client::heartbeat`); the runtime treats them as
                    // liveness evidence implicitly via transport.recv().
                }
                MessageType::SessionAck(payload) => {
                    // ARCP v1.1 §6.5: monotonically advance the
                    // last-acked counter and wake the writer.
                    let cur = last_ack.load(Ordering::Acquire);
                    if payload.last_processed_seq > cur {
                        last_ack.store(payload.last_processed_seq, Ordering::Release);
                        ack_notify.notify_waiters();
                    }
                }
                MessageType::SessionListJobs(payload) => {
                    // ARCP v1.1 §6.6: read-only job inventory scoped to
                    // the current session's principal. The Rust SDK
                    // scopes by session_id; cross-session listing is a
                    // deployment-policy extension.
                    if let Some(s) = state.as_ref() {
                        let jobs_list =
                            jobs.list_for_session(&s.session_id, payload.filter.as_ref());
                        let response =
                            MessageType::SessionJobs(arcp_core::messages::SessionJobsPayload {
                                request_id: envelope.id.to_string(),
                                jobs: jobs_list,
                                next_cursor: None,
                            });
                        let mut env = Envelope::new(response);
                        env.correlation_id = Some(envelope.id.clone());
                        env.session_id = Some(s.session_id.clone());
                        let _ = out_tx.send(env).await;
                    }
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
                MessageType::JobSubscribe(payload) => {
                    if let Some(s) = state.as_ref() {
                        Self::handle_job_subscribe(
                            &out_tx,
                            &self.inner.subscriptions,
                            &self.inner.jobs,
                            &self.inner.session_principals,
                            &connection_job_subs,
                            envelope.id.clone(),
                            s.session_id.clone(),
                            s.principal.clone(),
                            payload,
                        )
                        .await;
                    }
                }
                MessageType::JobUnsubscribe(JobUnsubscribePayload { job_id }) => {
                    if let Some((_, join)) = connection_job_subs.remove(&job_id) {
                        join.abort();
                    }
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

        // Tear down: stop per-connection subscription forwarders and
        // drop the out_tx so the writer drains. Per ARCP v1.1 durable
        // semantics (§10.1), in-flight jobs survive a transport drop —
        // they are only cancelled when the client sends `session.close`.
        if explicit_close {
            if let Some(s) = state.as_ref() {
                for snap in jobs.list_for_session(&s.session_id, None) {
                    let _ = jobs.cancel(&snap.job_id);
                }
            }
        }
        if let Some(s) = state.as_ref() {
            self.inner.session_principals.remove(&s.session_id);
        }
        for entry in connection_subs.iter() {
            entry.value().abort();
        }
        connection_subs.clear();
        for entry in connection_job_subs.iter() {
            entry.value().abort();
        }
        connection_job_subs.clear();
        drop(out_tx);
        // Wake the writer if it's currently parked on the ack window so
        // it can observe the closed channel and exit.
        ack_notify.notify_waiters();
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
                self.inner
                    .session_principals
                    .insert(session_id.clone(), Some(principal.clone()));
                state.principal = Some(principal);
                state.phase = HandshakePhase::Accepted;
                self.send_accepted(out, correlation_id, &session_id, &negotiated)
                    .await;
            }
            AuthOutcome::Challenge { challenge } => {
                state.active_challenge = Some(challenge.clone());
                state.phase = HandshakePhase::Challenged;
                let mut env = Envelope::new(MessageType::SessionChallenge(
                    arcp_core::messages::SessionChallengePayload {
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
            arcp_core::messages::AuthScheme::Bearer,
            arcp_core::messages::AuthScheme::SignedJwt,
        ] {
            let Some(authenticator) = self.inner.auth.get(&scheme) else {
                continue;
            };
            let outcome = authenticator
                .verify_challenge_response(&challenge, response)
                .await?;
            match outcome {
                AuthOutcome::Accept { principal } => {
                    self.inner
                        .session_principals
                        .insert(state.session_id.clone(), Some(principal.clone()));
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

    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    async fn spawn_tool_invoke(
        &self,
        out: &mpsc::Sender<Envelope>,
        jobs: &JobRegistry,
        correlation_id: MessageId,
        session_id: SessionId,
        principal: Option<String>,
        idempotency_key: Option<IdempotencyKey>,
        payload: ToolInvokePayload,
    ) {
        // ARCP v1.1 §6.4 logical idempotency: a retry of the same
        // command intent (same scope + key + tool + arguments) MUST
        // resolve to the original `job.accepted`. A conflicting payload
        // under the same key is rejected with FAILED_PRECONDITION.
        let idempotency_scope = idempotency_key.as_ref().map(|key| IdempotencyScope {
            principal_or_session: principal.clone().unwrap_or_else(|| session_id.to_string()),
            idempotency_key: key.clone(),
        });
        let canonical_args = serde_json::to_string(&payload.arguments).unwrap_or_default();
        if let Some(scope) = idempotency_scope.as_ref() {
            if let Some(record) = self.inner.idempotency_index.get(scope) {
                if record.tool == payload.tool && record.arguments_canonical == canonical_args {
                    let mut accepted =
                        Envelope::new(MessageType::JobAccepted(record.accepted.clone()));
                    accepted.correlation_id = Some(correlation_id);
                    accepted.session_id = Some(session_id);
                    accepted.job_id = Some(record.accepted.job_id.clone());
                    accepted.idempotency_key = idempotency_key;
                    let _ = out.send(accepted).await;
                    return;
                }
                let mut err = Envelope::new(MessageType::JobFailed(JobFailedPayload {
                    code: ErrorCode::FailedPrecondition,
                    retryable: Some(false),
                    message: format!(
                        "idempotency key {} already bound to a different command intent",
                        scope.idempotency_key
                    ),
                    details: None,
                }));
                err.correlation_id = Some(correlation_id);
                err.session_id = Some(session_id);
                err.idempotency_key = idempotency_key;
                let _ = out.send(err).await;
                return;
            }
        }
        let job_id = JobId::new();

        // ARCP v1.1 §7.5: parse the requested tool/agent as an
        // AgentRef so a `name@version` reference resolves correctly.
        let agent_ref = match arcp_core::messages::AgentRef::parse(&payload.tool) {
            Ok(r) => r,
            Err(e) => {
                let mut err = Envelope::new(MessageType::JobFailed(JobFailedPayload {
                    code: ErrorCode::InvalidArgument,
                    retryable: Some(false),
                    message: format!("invalid agent reference {}: {e}", payload.tool),
                    details: None,
                }));
                err.correlation_id = Some(correlation_id);
                err.session_id = Some(session_id);
                err.job_id = Some(job_id);
                let _ = out.send(err).await;
                return;
            }
        };
        let lease = effective_lease(&payload);
        let defer_accepted = self.inner.credential_provisioner.is_some() && lease.is_some();
        let accepted_sent = if defer_accepted {
            false
        } else {
            let mut accepted = Envelope::new(MessageType::JobAccepted(JobAcceptedPayload {
                job_id: job_id.clone(),
                credentials: vec![],
                lease: lease.clone(),
            }));
            accepted.correlation_id = Some(correlation_id.clone());
            accepted.session_id = Some(session_id.clone());
            accepted.job_id = Some(job_id.clone());
            let _ = out.send(accepted).await;
            true
        };

        // If a version is pinned, the advertised inventory MUST satisfy
        // it (§7.5). Surface AGENT_VERSION_NOT_AVAILABLE on miss.
        if agent_ref.version.is_some() {
            let advertised = &self.inner.advertised_capabilities.agents;
            let satisfied = advertised
                .as_ref()
                .is_some_and(|inv| inv.satisfies(&agent_ref));
            if !satisfied {
                let mut err = Envelope::new(MessageType::JobFailed(JobFailedPayload {
                    code: ErrorCode::AgentVersionNotAvailable,
                    retryable: Some(false),
                    message: format!("agent version not available: {}", agent_ref.format()),
                    details: None,
                }));
                err.correlation_id = Some(correlation_id);
                err.session_id = Some(session_id);
                err.job_id = Some(job_id);
                let _ = out.send(err).await;
                return;
            }
        }

        let Some(handler) = self.inner.tools.get(&agent_ref.name) else {
            let mut err = Envelope::new(MessageType::JobFailed(JobFailedPayload {
                code: ErrorCode::NotFound,
                retryable: Some(false),
                message: format!("tool not registered: {}", agent_ref.name),
                details: None,
            }));
            err.correlation_id = Some(correlation_id);
            err.session_id = Some(session_id);
            err.job_id = Some(job_id);
            let _ = out.send(err).await;
            return;
        };

        let credentials = if let (Some(provisioner), Some(lease_ref)) =
            (&self.inner.credential_provisioner, lease.as_ref())
        {
            let ctx = CredentialJobContext {
                job_id: job_id.clone(),
                session_id: session_id.clone(),
                principal: principal.clone(),
                parent_job_id: None,
            };
            match provisioner.issue(lease_ref, &ctx).await {
                Ok(credentials) => {
                    self.inner
                        .credential_ledger
                        .record_issued(&job_id, &credentials);
                    credentials
                }
                Err(e) => {
                    let mut err = Envelope::new(MessageType::JobFailed(JobFailedPayload {
                        code: e.code(),
                        retryable: Some(e.retryable()),
                        message: e.to_string(),
                        details: None,
                    }));
                    err.correlation_id = Some(correlation_id);
                    err.session_id = Some(session_id);
                    err.job_id = Some(job_id);
                    let _ = out.send(err).await;
                    return;
                }
            }
        } else {
            Vec::new()
        };

        // job.accepted
        if !accepted_sent {
            let mut accepted = Envelope::new(MessageType::JobAccepted(JobAcceptedPayload {
                job_id: job_id.clone(),
                credentials: credentials.clone(),
                lease: lease.clone(),
            }));
            accepted.correlation_id = Some(correlation_id.clone());
            accepted.session_id = Some(session_id.clone());
            accepted.job_id = Some(job_id.clone());
            let _ = out.send(accepted).await;
        }

        // §10.1 durable jobs outlive the transport, so the job's
        // cancel token must NOT be a child of the connection token.
        // Authorized cancel envelopes and explicit `session.close`
        // drive cancellation explicitly through `jobs.cancel`.
        let cancel = CancellationToken::new();
        let entry = JobEntry {
            job_id: job_id.clone(),
            session_id: session_id.clone(),
            correlation_id: correlation_id.clone(),
            cancel: cancel.clone(),
            state: JobState::Accepted,
            // §7.5: listings show the resolved `name@version` string.
            agent: agent_ref.format(),
            created_at: chrono::Utc::now(),
            last_event_seq: 0,
            parent_job_id: None,
            credential_ids: self.inner.credential_ledger.outstanding_for_job(&job_id),
            lease: lease.clone(),
        };

        let out_clone = out.clone();
        let jobs_clone = jobs.clone();
        let provisioner_clone = self.inner.credential_provisioner.clone();
        let credential_ledger_clone = self.inner.credential_ledger.clone();
        let cancel_for_task = cancel;
        // ARCP v1.1 §9.6: seed the per-job budget tracker from the
        // `cost_budget` field on `tool.invoke`. Absent / empty means
        // budgeting is disabled for this job.
        let budget_tracker = lease
            .as_ref()
            .and_then(|lease| lease.cost_budget.as_ref())
            .map_or_else(crate::runtime::context::BudgetTracker::new, |budget| {
                crate::runtime::context::BudgetTracker::from_budget(budget)
            });

        // Record the accepted payload against the (scope, key) tuple so
        // a future retry resolves to this same job_id instead of
        // spawning a duplicate (§6.4).
        if let Some(scope) = idempotency_scope {
            self.inner.idempotency_index.insert(
                scope,
                IdempotencyRecord {
                    accepted: JobAcceptedPayload {
                        job_id: job_id.clone(),
                        credentials: credentials.clone(),
                        lease: lease.clone(),
                    },
                    tool: agent_ref.format(),
                    arguments_canonical: canonical_args,
                },
            );
        }

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
                budget: budget_tracker,
                lease,
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
                    // Allow agents that stream results to indicate the
                    // terminating job.completed should reference a
                    // `result_id` (ARCP v1.1 §8.4) by returning the
                    // sentinel shape `{ "$arcp_streamed_result": {
                    // result_id, result_size?, summary? } }`. Everything
                    // else flows through as `value` (the v1.0 path).
                    let completed = streamed_result_from_value(value);
                    MessageType::JobCompleted(completed)
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
            term.job_id = Some(job_id.clone());
            let _ = out_clone.send(term).await;
            if let Some(provisioner) = provisioner_clone.as_ref() {
                if let Err(e) =
                    revoke_all_for_job(&credential_ledger_clone, provisioner, &job_id).await
                {
                    tracing::warn!(error = %e, job_id = %job_id, "failed to revoke credentials");
                }
            }
        });

        jobs.insert(entry, join);
    }

    async fn handle_cancel(
        &self,
        out: &mpsc::Sender<Envelope>,
        jobs: &JobRegistry,
        correlation_id: MessageId,
        requester: &SessionState,
        payload: &CancelPayload,
    ) {
        let CancelPayload {
            target, target_id, ..
        } = payload;
        match target {
            CancelTargetKind::Job => {
                #[allow(clippy::option_if_let_else)] // map_or_else nests too deeply here
                let response_payload = if let Ok(job_id) = target_id.parse::<JobId>() {
                    if let Some(snap) = jobs.snapshot(&job_id) {
                        // ARCP v1.1 §7.6 / §10: cancel authority is
                        // bound to the owning session or the same
                        // authenticated principal. A subscriber that
                        // merely knows another session's job id MUST
                        // NOT be able to cancel it.
                        let authorized = snap.session_id == requester.session_id
                            || cancel_principal_matches(
                                &self.inner.session_principals,
                                &snap.session_id,
                                requester.principal.as_deref(),
                            );
                        if authorized {
                            if jobs.cancel(&job_id) {
                                MessageType::CancelAccepted(
                                    arcp_core::messages::CancelAcceptedPayload {
                                        target_id: Some(target_id.clone()),
                                    },
                                )
                            } else {
                                MessageType::CancelRefused(
                                    arcp_core::messages::CancelRefusedPayload {
                                        target_id: target_id.clone(),
                                        reason: "job is no longer in-flight".into(),
                                    },
                                )
                            }
                        } else {
                            MessageType::CancelRefused(arcp_core::messages::CancelRefusedPayload {
                                target_id: target_id.clone(),
                                reason: "permission denied: not authorized to cancel this job"
                                    .into(),
                            })
                        }
                    } else {
                        MessageType::CancelRefused(arcp_core::messages::CancelRefusedPayload {
                            target_id: target_id.clone(),
                            reason: "no such in-flight job".into(),
                        })
                    }
                } else {
                    MessageType::CancelRefused(arcp_core::messages::CancelRefusedPayload {
                        target_id: target_id.clone(),
                        reason: "malformed job id".into(),
                    })
                };
                let mut env = Envelope::new(response_payload);
                env.correlation_id = Some(correlation_id);
                env.session_id = Some(requester.session_id.clone());
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
            model_use: intersect_bool(runtime_caps.model_use, client_caps.model_use),
            provisioned_credentials: intersect_bool(
                runtime_caps.provisioned_credentials,
                client_caps.provisioned_credentials,
            ),
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
            // ARCP v1.1 §7.5 — pass the runtime's agent inventory
            // through to the negotiated capability block. Clients
            // typically do not advertise agents, so this is a
            // server-side pass-through.
            agents: runtime_caps.agents.clone(),
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

    #[allow(clippy::too_many_arguments)]
    async fn handle_job_subscribe(
        out: &mpsc::Sender<Envelope>,
        manager: &SubscriptionManager,
        jobs: &JobRegistry,
        session_principals: &DashMap<SessionId, Option<String>>,
        connection_job_subs: &Arc<DashMap<JobId, JoinHandle<()>>>,
        correlation_id: MessageId,
        subscriber_session: SessionId,
        subscriber_principal: Option<String>,
        payload: JobSubscribePayload,
    ) {
        let JobSubscribePayload {
            job_id,
            from_event_seq: _,
            history: _,
        } = payload;

        let Some(snap) = jobs.snapshot(&job_id) else {
            let mut err = Envelope::new(MessageType::Nack(NackPayload {
                code: ErrorCode::NotFound,
                message: format!("no such job: {job_id}"),
                details: None,
            }));
            err.correlation_id = Some(correlation_id);
            err.session_id = Some(subscriber_session);
            let _ = out.send(err).await;
            return;
        };

        // Authorization (§7.6): subscribing session's principal must
        // match the submitter's principal. The submitter is always
        // permitted (same session_id).
        if snap.session_id != subscriber_session {
            let submitter_principal = session_principals
                .get(&snap.session_id)
                .and_then(|p| p.value().clone());
            let permitted = match (&submitter_principal, &subscriber_principal) {
                (Some(a), Some(b)) => a == b,
                _ => false,
            };
            if !permitted {
                let mut err = Envelope::new(MessageType::Nack(NackPayload {
                    code: ErrorCode::PermissionDenied,
                    message: "principal not authorized to subscribe to this job".into(),
                    details: None,
                }));
                err.correlation_id = Some(correlation_id);
                err.session_id = Some(subscriber_session);
                err.job_id = Some(job_id);
                let _ = out.send(err).await;
                return;
            }
        }

        // Build a filter that selects only this job's envelopes.
        let filter = arcp_core::messages::SubscriptionFilter {
            job_id: vec![job_id.clone()],
            ..arcp_core::messages::SubscriptionFilter::default()
        };
        let (_internal_id, mut rx) = manager.register(filter, subscriber_session.clone());

        // Acknowledge.
        let ack = JobSubscribedPayload {
            job_id: job_id.clone(),
            current_status: snap.state.wire_str().to_owned(),
            agent: snap.agent.clone(),
            parent_job_id: snap.parent_job_id.clone(),
            trace_id: None,
            subscribed_from: snap.last_event_seq,
            // History replay is not yet implemented in this SDK; the ack
            // always carries `replayed: false`, matching live-only
            // semantics (§7.6 permits `history: false`).
            replayed: false,
        };
        let mut ack_env = Envelope::new(MessageType::JobSubscribed(ack));
        ack_env.correlation_id = Some(correlation_id);
        ack_env.session_id = Some(subscriber_session.clone());
        ack_env.job_id = Some(job_id.clone());
        let _ = out.send(ack_env).await;

        // Spawn forwarder: rewrites session_id to the subscriber's so
        // client-side parsers route correctly. The originating session's
        // own writer is responsible for the submitter's copy; here we
        // only fan out a clone to the subscriber.
        let out_clone = out.clone();
        let subscriber_session_clone = subscriber_session;
        let job_id_clone = job_id.clone();
        let connection_job_subs_clone = Arc::clone(connection_job_subs);
        let join = tokio::spawn(async move {
            while let Some(mut env) = rx.next().await {
                // Only forward server-originated, job-scoped envelopes.
                // Skip subscriber's own client-to-server messages (e.g.
                // tool.invoke, cancel) which can appear on the bus.
                if !is_forwardable_job_event(&env.payload) {
                    continue;
                }
                env.session_id = Some(subscriber_session_clone.clone());
                if out_clone.send(env).await.is_err() {
                    break;
                }
            }
            // Forwarder exited (job terminal or unsubscribe).
            connection_job_subs_clone.remove(&job_id_clone);
        });
        connection_job_subs.insert(job_id, join);
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

fn effective_lease(payload: &ToolInvokePayload) -> Option<LeaseRequest> {
    if let Some(lease) = payload.lease_request.clone() {
        return Some(lease);
    }
    payload.cost_budget.clone().map(|cost_budget| LeaseRequest {
        cost_budget: Some(cost_budget),
        model_use: None,
        expires_at: None,
        extra: std::collections::BTreeMap::new(),
    })
}

fn redact_for_subscribers(env: &Envelope) -> Envelope {
    let mut out = env.clone();
    if let MessageType::JobAccepted(payload) = &mut out.payload {
        payload.credentials.clear();
    }
    out
}

/// Sentinel key for streamed-result agents (ARCP v1.1 §8.4).
///
/// When an agent's returned value is a single-entry object keyed by this
/// constant, the runtime promotes the payload (`result_id`,
/// `result_size`, `summary`) onto the terminating `job.completed` rather
/// than carrying the sentinel through as `value`.
pub const STREAMED_RESULT_SENTINEL: &str = "$arcp_streamed_result";

/// Build a [`JobCompletedPayload`] from a tool's returned value,
/// recognising the streaming-result sentinel.
fn streamed_result_from_value(value: serde_json::Value) -> JobCompletedPayload {
    if let Some(obj) = value.as_object() {
        if obj.len() == 1 {
            if let Some(inner) = obj.get(STREAMED_RESULT_SENTINEL) {
                let result_id = inner
                    .get("result_id")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let result_size = inner.get("result_size").and_then(serde_json::Value::as_u64);
                let summary = inner
                    .get("summary")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                if result_id.is_some() {
                    return JobCompletedPayload {
                        value: None,
                        result_ref: None,
                        result_id,
                        result_size,
                        summary,
                    };
                }
            }
        }
    }
    JobCompletedPayload {
        value: Some(value),
        result_ref: None,
        result_id: None,
        result_size: None,
        summary: None,
    }
}

/// True when an envelope is a server-emitted job event suitable for
/// `job.subscribe` forwarding (ARCP v1.1 §7.6).
///
/// Filters out client-to-server commands that happen to carry `job_id`
/// (e.g. `cancel`, `tool.invoke`).
const fn is_forwardable_job_event(payload: &MessageType) -> bool {
    matches!(
        payload,
        MessageType::JobAccepted(_)
            | MessageType::JobStarted(_)
            | MessageType::JobProgress(_)
            | MessageType::JobHeartbeat(_)
            | MessageType::JobCompleted(_)
            | MessageType::JobFailed(_)
            | MessageType::JobCancelled(_)
            | MessageType::JobResultChunk(_)
            | MessageType::ToolResult(_)
            | MessageType::ToolError(_)
            | MessageType::Log(_)
            | MessageType::Metric(_)
            | MessageType::StreamOpen(_)
            | MessageType::StreamChunk(_)
            | MessageType::StreamClose(_)
            | MessageType::StreamError(_)
            | MessageType::ArtifactRef(_)
    )
}

/// Same-principal authorization helper for cross-session cancel
/// (ARCP v1.1 §10). Returns `true` only when the requesting session's
/// principal is non-anonymous and matches the principal that originally
/// submitted the job.
fn cancel_principal_matches(
    session_principals: &DashMap<SessionId, Option<String>>,
    owning_session: &SessionId,
    requester_principal: Option<&str>,
) -> bool {
    let Some(requester_principal) = requester_principal else {
        return false;
    };
    session_principals
        .get(owning_session)
        .and_then(|p| p.value().clone())
        .is_some_and(|owner| owner == requester_principal)
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
