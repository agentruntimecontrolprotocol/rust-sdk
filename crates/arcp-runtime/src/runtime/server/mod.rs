//! ARCP runtime — the server side that drives the ARCP v1.1 §6.2
//! hello/welcome handshake (serialized as `session.open` / `session.accepted`)
//! and dispatches subsequent envelopes.

pub(crate) use std::collections::{HashSet, VecDeque};
pub(crate) use std::sync::atomic::{AtomicU64, Ordering};
pub(crate) use std::sync::Arc;
pub(crate) use std::time::Duration;

pub(crate) use dashmap::DashMap;
pub(crate) use tokio::sync::{mpsc, Notify};
pub(crate) use tokio::task::JoinHandle;
pub(crate) use tokio_util::sync::CancellationToken;

pub(crate) use super::artifact::ArtifactStore;
pub(crate) use super::context::ToolContext;
pub(crate) use super::credentials::{
    revoke_all_for_job, CredentialJobContext, CredentialLedger, CredentialProvisioner,
};
pub(crate) use super::job::{JobEntry, JobRegistry};
pub(crate) use super::session::{HandshakePhase, SessionState};
pub(crate) use super::subscription::SubscriptionManager;
pub(crate) use super::tools::ToolRegistry;
pub(crate) use crate::store::eventlog::EventLog;
pub(crate) use arcp_core::auth::{AuthOutcome, AuthRegistry, Authenticator};
pub(crate) use arcp_core::envelope::Envelope;
pub(crate) use arcp_core::error::{ARCPError, ErrorCode};
pub(crate) use arcp_core::extensions::ExtensionRegistry;
pub(crate) use arcp_core::ids::IdempotencyKey;
pub(crate) use arcp_core::ids::SubscriptionId;
pub(crate) use arcp_core::ids::{JobId, MessageId, SessionId};
pub(crate) use arcp_core::messages::{
    ArtifactFetchPayload, ArtifactPutPayload, ArtifactRefPayload, ArtifactReleasePayload,
    CancelPayload, CancelTargetKind, Capabilities, JobAcceptedPayload, JobCancelledPayload,
    JobCompletedPayload, JobFailedPayload, JobStartedPayload, JobState, JobSubscribePayload,
    JobSubscribedPayload, JobUnsubscribePayload, LeaseRequest, MessageType, NackPayload,
    RuntimeIdentity, SessionAcceptedPayload, SessionLease, SessionOpenPayload,
    SessionRejectedPayload, SessionUnauthenticatedPayload, SubscribeAcceptedPayload,
    SubscribeEventPayload, SubscribePayload, ToolInvokePayload, UnsubscribePayload,
};
pub(crate) use arcp_core::transport::Transport;
pub(crate) use arcp_core::{IMPL_KIND, IMPL_VERSION};

mod artifacts;
mod builder;
mod handshake;
mod jobs;
mod subscriptions;

pub use builder::RuntimeBuilder;

/// Fixed cadence for the background terminal-job maintenance sweep.
const MAINTENANCE_INTERVAL: Duration = Duration::from_secs(30);

/// Sweep terminal jobs older than `retention` and drop the idempotency
/// records bound to the swept jobs. Returns the number of jobs swept.
fn sweep_terminal_state(inner: &RuntimeInner, retention: Duration) -> usize {
    let swept = inner.jobs.sweep_terminals_older_than(retention);
    if !swept.is_empty() {
        let swept_set: HashSet<&JobId> = swept.iter().collect();
        inner
            .idempotency_index
            .retain(|_, rec| !swept_set.contains(&rec.accepted.job_id));
    }
    swept.len()
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
    /// Per-session authenticated principal. Used by `subscribe` /
    /// `job.subscribe` authorization (default policy: same-principal as the
    /// submitter). Shared (`Arc`) so per-subscription forwarder tasks can
    /// resolve a publishing session's principal at delivery time.
    session_principals: Arc<DashMap<SessionId, Option<String>>>,
    /// Optional provisioner for lease-bound upstream credentials.
    credential_provisioner: Option<Arc<dyn CredentialProvisioner>>,
    /// Runtime ledger of outstanding credential ids.
    credential_ledger: CredentialLedger,
    /// Logical idempotency index for `tool.invoke` (ARCP v1.1 §7.2).
    /// Keyed by `(principal-or-session, idempotency_key)`; resolves a
    /// repeat command intent to the original `JobAccepted` payload so
    /// retries return the same `job_id` instead of starting a duplicate
    /// job. Bounded by [`Self::terminal_retention`] via the maintenance
    /// sweep (#85).
    idempotency_index: DashMap<IdempotencyScope, IdempotencyRecord>,
    /// Retention window for terminal jobs and their idempotency records.
    terminal_retention: Duration,
    /// ARCP v1.1 §6.3 resume-token registry. Maps the most recently issued
    /// `resume_token` to the session it can resume. Rotated on every
    /// successful welcome / resume so a stale token no longer resolves.
    resume_registry: Arc<DashMap<String, ResumeEntry>>,
}

/// Registration backing a `resume_token` (ARCP v1.1 §6.3).
#[derive(Debug, Clone)]
struct ResumeEntry {
    session_id: SessionId,
    principal: Option<String>,
    capabilities: Capabilities,
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

    /// Run one terminal-job maintenance sweep using the configured
    /// retention window and return the number of jobs evicted.
    ///
    /// Drops terminal jobs whose terminal instant is older than the
    /// retention window from the [`JobRegistry`] and removes the
    /// idempotency records bound to them (#72, #85). The runtime also runs
    /// this periodically in the background; this entry point exists for
    /// callers (and tests) that want to force a sweep deterministically.
    #[must_use]
    pub fn sweep_terminal_jobs(&self) -> usize {
        sweep_terminal_state(&self.inner, self.inner.terminal_retention)
    }

    /// Number of jobs currently retained in the registry.
    #[must_use]
    pub fn job_count(&self) -> usize {
        self.inner.jobs.len()
    }

    /// Snapshot a job's public-facing state, if it is still registered.
    /// Exposes registry visibility for inspection and tests (e.g. asserting
    /// a job survives `session.close` per §6.7).
    #[must_use]
    pub fn job_snapshot(&self, job_id: &JobId) -> Option<super::job::JobSnapshot> {
        self.inner.jobs.snapshot(job_id)
    }

    /// Number of live idempotency records (ARCP v1.1 §7.2). Bounded by the
    /// terminal-retention sweep (#85).
    #[must_use]
    pub fn idempotency_index_len(&self) -> usize {
        self.inner.idempotency_index.len()
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
        // Forwarding channel for `job.subscribe` (ARCP v1.1 §7.6): events
        // that arrive here are already-published copies fanned out to this
        // connection from the shared subscription bus. The writer sends them
        // straight to the transport and MUST NOT re-publish or re-log them —
        // otherwise the writer's republish would re-match the subscriber's
        // own filter and amplify into an unbounded echo storm (#82).
        let (fwd_tx, mut fwd_rx) = mpsc::channel::<Envelope>(256);
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
            loop {
                // Bias toward locally-originated envelopes (`out_rx`) so
                // session/job lifecycle ordering is preserved; forwarded
                // observer copies (`fwd_rx`) are delivered as they arrive.
                // The connection holds the primary `fwd_tx`, so `fwd_rx`
                // only closes during teardown (when `out_tx` is dropped
                // too), which the `out_rx` arm observes and exits on.
                let env = tokio::select! {
                    biased;
                    maybe = out_rx.recv() => {
                        let Some(mut env) = maybe else { break };
                        // Flow control (§6.5): for countable events, gate on
                        // the sliding window BEFORE persistence / publishing
                        // so an envelope blocked by backpressure isn't logged
                        // as already delivered. Non-countable envelopes
                        // (handshake, heartbeat, ack, control) bypass the gate.
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
                            // §6.6). For job-scoped events, also bump the job's
                            // high-water mark so session.list_jobs and
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
                        // job.* / tool.* / stream.* events that originate on
                        // the server side (ARCP v1.1 §7.6). Skip subscribe.event
                        // itself so the wrapper isn't re-broadcast, which would
                        // cause an echo storm whenever a filter matches
                        // subscribe.event.
                        if !matches!(env.payload, MessageType::SubscribeEvent(_)) {
                            let publish_env = redact_for_subscribers(&env);
                            let _ = writer_subs.publish(&publish_env);
                        }
                        env
                    }
                    // Forwarded `job.subscribe` copies (§7.6). These were
                    // already persisted and published by the originating
                    // connection, so they are sent verbatim to the transport
                    // WITHOUT re-logging or re-publishing — breaking the
                    // amplification loop (#82).
                    maybe = fwd_rx.recv() => {
                        let Some(env) = maybe else { break };
                        env
                    }
                };
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
        // Transport-level dedup over a bounded sliding window of recent
        // message ids (#86). A long-lived durable session can receive an
        // unbounded number of messages, so we cap the window instead of
        // retaining every id ever seen.
        let mut seen_ids = RecentIdSet::new(DEDUP_WINDOW);
        // ARCP v1.1 durable-job semantics (§10.1, README §"Reconnect"):
        // a normal transport drop must NOT cancel in-flight jobs. We
        // only tear down jobs when the client sends `session.close`.
        let mut explicit_close = false;

        let result = loop {
            let Some(envelope) = transport.recv().await? else {
                break Ok(());
            };

            // Transport-level idempotency check over the recent-id window.
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
                MessageType::SessionResume(payload) => {
                    // ARCP v1.1 §6.3: reconnect via resume token. Allowed
                    // in place of session.open on a fresh connection.
                    if let Some(resumed) = self
                        .handle_session_resume(&out_tx, &fwd_tx, envelope.id.clone(), payload)
                        .await
                    {
                        state = Some(resumed);
                    }
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
                            &self.inner.session_principals,
                            &connection_subs,
                            envelope.id.clone(),
                            s.session_id.clone(),
                            s.principal.clone(),
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
                            &fwd_tx,
                            &self.inner.subscriptions,
                            &self.inner.jobs,
                            &self.inner.event_log,
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
                    // Unknown post-handshake envelope. Logging at debug
                    // for now; a future revision should NACK with
                    // INVALID_REQUEST so the peer sees the rejection.
                    tracing::debug!(
                        type_name = envelope.payload.type_name(),
                        "no dispatch arm for this envelope type",
                    );
                }
            }
        };

        // Tear down: stop per-connection subscription forwarders and
        // drop the out_tx so the writer drains. Per ARCP v1.1 §6.7 and the
        // durable-job semantics of §10.1, a graceful `session.close`
        // terminates the SESSION only — in-flight jobs are NOT affected:
        // they keep running in the shared JobRegistry and remain resumable
        // within the resume window. We acknowledge the close with
        // `session.closed` and tear down only connection-local state; jobs
        // are cancelled solely by an explicit authorized `job.cancel`.
        if explicit_close {
            if let Some(s) = state.as_ref() {
                let mut closed = Envelope::new(MessageType::SessionClosed(
                    arcp_core::messages::SessionClosedPayload { reason: None },
                ));
                closed.session_id = Some(s.session_id.clone());
                let _ = out_tx.send(closed).await;
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
        // Drop the forwarding channel too so the writer's select loop
        // observes both channels closed and exits.
        drop(fwd_tx);
        // Wake the writer if it's currently parked on the ack window so
        // it can observe the closed channel and exit.
        ack_notify.notify_waiters();
        let _ = writer.await;
        result
    }
}

/// Upper bound on the number of buffered events replayed for one
/// `job.subscribe` history request (ARCP v1.1 §7.6).
const REPLAY_LIMIT: i64 = 10_000;

/// Size of the per-connection transport-dedup window (#86). Replays older
/// than this many distinct received ids are no longer detected; in practice
/// replays only target a recent window.
const DEDUP_WINDOW: usize = 8192;

/// Bounded set of recently-seen message ids with FIFO eviction.
///
/// Backs transport-level dedup with O(1) membership and a fixed memory
/// ceiling of `cap` ids, replacing an unbounded `HashSet` (#86).
struct RecentIdSet {
    set: HashSet<MessageId>,
    order: VecDeque<MessageId>,
    cap: usize,
}

impl RecentIdSet {
    fn new(cap: usize) -> Self {
        Self {
            set: HashSet::new(),
            order: VecDeque::new(),
            cap: cap.max(1),
        }
    }

    /// Record `id`. Returns `true` if it was newly inserted, `false` if it
    /// is a duplicate within the current window. Evicts the oldest id when
    /// the window is full.
    fn insert(&mut self, id: MessageId) -> bool {
        if self.set.contains(&id) {
            return false;
        }
        if self.order.len() >= self.cap {
            if let Some(old) = self.order.pop_front() {
                self.set.remove(&old);
            }
        }
        self.set.insert(id.clone());
        self.order.push_back(id);
        true
    }
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

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod recent_id_set_tests {
    use super::{MessageId, RecentIdSet};

    #[test]
    fn detects_duplicates_within_window() {
        let mut set = RecentIdSet::new(4);
        let id = MessageId::new();
        assert!(set.insert(id.clone()));
        assert!(!set.insert(id), "same id is a duplicate");
    }

    #[test]
    fn evicts_oldest_when_capacity_exceeded() {
        let mut set = RecentIdSet::new(2);
        let a = MessageId::new();
        let b = MessageId::new();
        let c = MessageId::new();
        assert!(set.insert(a.clone()));
        assert!(set.insert(b.clone()));
        // Inserting c evicts a (oldest); window is now [b, c].
        assert!(set.insert(c.clone()));
        assert_eq!(set.order.len(), 2);
        assert_eq!(set.set.len(), 2);
        // b and c are still within the window → duplicates.
        assert!(!set.insert(b), "b still tracked");
        assert!(!set.insert(c), "c still tracked");
        // a fell out of the window, so it is treated as new again.
        assert!(set.insert(a), "a was evicted and is new again");
    }
}
