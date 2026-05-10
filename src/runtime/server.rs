//! ARCP runtime — the server side that drives the handshake (RFC §8.1)
//! and dispatches subsequent envelopes.

use std::collections::HashSet;
use std::sync::Arc;

use tokio::task::JoinHandle;

use super::session::{HandshakePhase, SessionState};
use crate::auth::{AuthOutcome, AuthRegistry, Authenticator};
use crate::envelope::Envelope;
use crate::error::{ARCPError, ErrorCode};
use crate::extensions::ExtensionRegistry;
use crate::ids::{MessageId, SessionId};
use crate::messages::{
    Capabilities, MessageType, RuntimeIdentity, SessionAcceptedPayload, SessionLease,
    SessionOpenPayload, SessionRejectedPayload, SessionUnauthenticatedPayload,
};
use crate::store::eventlog::EventLog;
use crate::transport::Transport;
use crate::{IMPL_KIND, IMPL_VERSION};

/// Runtime configuration.
pub struct RuntimeBuilder {
    auth: AuthRegistry,
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
                advertised_capabilities: self.advertised_capabilities,
                runtime_identity: self.runtime_identity,
                session_lease_seconds: self.session_lease_seconds,
                extension_registry: ExtensionRegistry::new(),
                event_log,
            }),
        })
    }
}

struct RuntimeInner {
    auth: AuthRegistry,
    advertised_capabilities: Capabilities,
    runtime_identity: RuntimeIdentity,
    session_lease_seconds: Option<u64>,
    #[allow(dead_code)]
    extension_registry: ExtensionRegistry,
    event_log: EventLog,
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
    pub async fn run_connection<T: Transport>(&self, transport: T) -> Result<(), ARCPError> {
        let mut state: Option<SessionState> = None;
        let mut seen_ids: HashSet<MessageId> = HashSet::new();

        loop {
            let Some(envelope) = transport.recv().await? else {
                return Ok(());
            };

            // Transport-level idempotency check: drop replays.
            if !seen_ids.insert(envelope.id.clone()) {
                tracing::debug!(id = %envelope.id, "dropping replayed envelope");
                continue;
            }

            // Persist before dispatch so the event log records every accepted
            // envelope (later phases use this for backfill / resume).
            self.inner.event_log.append(&envelope).await?;

            // Handshake phase: only handshake messages are permitted before
            // session.accepted. Drop anything else with a log line per §8.1.
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
                        self.handle_session_open(&transport, envelope.id.clone(), payload)
                            .await?,
                    );
                }
                MessageType::SessionAuthenticate(payload) => {
                    if let Some(s) = state.as_mut() {
                        self.handle_session_authenticate(
                            &transport,
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
                    return Ok(());
                }
                _ if in_handshake => {
                    // Other handshake-classified messages from the client
                    // (session.challenge, session.accepted, etc.) are not
                    // expected from the client side — drop with a log line.
                    tracing::warn!(
                        type_name = envelope.payload.type_name(),
                        "unexpected handshake message direction",
                    );
                }
                _ => {
                    // Phase 2 stops here — Phase 3+ wires job/stream/etc.
                    // dispatch through this match.
                    tracing::debug!(
                        type_name = envelope.payload.type_name(),
                        "dispatch arm not yet implemented in Phase 2",
                    );
                }
            }
        }
    }

    async fn handle_session_open<T: Transport>(
        &self,
        transport: &T,
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
                transport,
                correlation_id,
                ErrorCode::Unauthenticated,
                format!("auth scheme {:?} not configured", auth.scheme),
            )
            .await?;
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
                self.send_accepted(transport, correlation_id, &session_id, &negotiated)
                    .await?;
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
                transport.send(env).await?;
            }
            AuthOutcome::Reject { reason } => {
                self.send_rejected(
                    transport,
                    correlation_id,
                    ErrorCode::Unauthenticated,
                    reason,
                )
                .await?;
                state.phase = HandshakePhase::Closed;
            }
        }
        Ok(state)
    }

    async fn handle_session_authenticate<T: Transport>(
        &self,
        transport: &T,
        correlation_id: MessageId,
        state: &mut SessionState,
        response: &str,
    ) -> Result<(), ARCPError> {
        let Some(challenge) = state.active_challenge.clone() else {
            tracing::warn!("session.authenticate without active challenge; dropping");
            return Ok(());
        };
        // Phase 2's bearer/none/jwt schemes don't actually use challenges,
        // but the structure is here for §8.4 re-auth and future schemes.
        // For now treat the response itself as a token retry.
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
                    self.send_accepted(
                        transport,
                        correlation_id,
                        &state.session_id,
                        &state.capabilities,
                    )
                    .await?;
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
        transport.send(env).await?;
        Ok(())
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

    async fn send_accepted<T: Transport>(
        &self,
        transport: &T,
        correlation_id: MessageId,
        session_id: &SessionId,
        capabilities: &Capabilities,
    ) -> Result<(), ARCPError> {
        let lease = self.inner.session_lease_seconds.map(|s| SessionLease {
            // Saturate at i64::MAX seconds so an absurd configured lease
            // can't wrap to a negative chrono::Duration.
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
        transport.send(env).await
    }

    async fn send_rejected<T: Transport>(
        &self,
        transport: &T,
        correlation_id: MessageId,
        code: ErrorCode,
        message: String,
    ) -> Result<(), ARCPError> {
        let mut env = Envelope::new(MessageType::SessionRejected(SessionRejectedPayload {
            code,
            message,
        }));
        env.correlation_id = Some(correlation_id);
        transport.send(env).await
    }
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
