//! Session handshake, authentication, and resume (split, #74).

#[allow(clippy::wildcard_imports)]
use super::*;

impl ARCPRuntime {
    pub(crate) async fn handle_session_open(
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
                state.principal = Some(principal.clone());
                state.phase = HandshakePhase::Accepted;
                self.send_accepted(
                    out,
                    correlation_id,
                    &session_id,
                    &negotiated,
                    Some(principal),
                )
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
    pub(crate) async fn handle_session_authenticate(
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
                    state.principal = Some(principal.clone());
                    state.phase = HandshakePhase::Accepted;
                    state.active_challenge = None;
                    self.send_accepted(
                        out,
                        correlation_id,
                        &state.session_id,
                        &state.capabilities,
                        Some(principal),
                    )
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
    /// Handle a `session.resume` (ARCP v1.1 §6.3). On success, returns the
    /// reattached [`SessionState`] and replays buffered events with
    /// `seq > last_event_seq`; on a stale token or uncovered sequence,
    /// emits `session.rejected` with `RESUME_WINDOW_EXPIRED` and returns
    /// `None`.
    pub(crate) async fn handle_session_resume(
        &self,
        out: &mpsc::Sender<Envelope>,
        fwd: &mpsc::Sender<Envelope>,
        correlation_id: MessageId,
        payload: arcp_core::messages::SessionResumePayload,
    ) -> Option<SessionState> {
        let arcp_core::messages::SessionResumePayload {
            resume_token,
            last_event_seq,
        } = payload;

        // Look up the presented token. A rotated / unknown token is stale.
        let Some(entry) = self
            .inner
            .resume_registry
            .get(&resume_token)
            .map(|r| r.value().clone())
        else {
            self.send_rejected(
                out,
                correlation_id,
                ErrorCode::ResumeWindowExpired,
                "resume token is unknown or has been rotated (ARCP v1.1 §6.3)".into(),
            )
            .await;
            return None;
        };

        // Coverage check (§6.3): the buffer must still cover the requested
        // sequence. With no eviction, the only uncovered case is a
        // last_event_seq beyond what was ever emitted for the session.
        let max_seq = self
            .inner
            .event_log
            .max_event_seq_for_session(entry.session_id.as_str())
            .await
            .unwrap_or(None)
            .unwrap_or(0);
        if last_event_seq > max_seq {
            self.send_rejected(
                out,
                correlation_id,
                ErrorCode::ResumeWindowExpired,
                format!(
                    "resume window does not cover last_event_seq={last_event_seq} \
                     (highest emitted={max_seq}) (ARCP v1.1 §6.3)"
                ),
            )
            .await;
            return None;
        }

        // Rotate the token: drop the presented one and mint a fresh one.
        self.inner.resume_registry.remove(&resume_token);
        let new_token = self.register_resume_token(
            &entry.session_id,
            entry.principal.clone(),
            &entry.capabilities,
        );
        self.inner
            .session_principals
            .insert(entry.session_id.clone(), entry.principal.clone());

        // Replay buffered events with seq > last_event_seq.
        let replay = self
            .inner
            .event_log
            .replay_session_events_after_seq(
                entry.session_id.as_str(),
                last_event_seq,
                REPLAY_LIMIT,
            )
            .await
            .unwrap_or_default();
        let replayed = !replay.is_empty();

        // Acknowledge the resume before replaying.
        let mut ack = Envelope::new(MessageType::SessionResumed(
            arcp_core::messages::SessionResumedPayload {
                session_id: entry.session_id.clone(),
                resume_token: new_token,
                replayed_from: last_event_seq,
                replayed,
            },
        ));
        ack.correlation_id = Some(correlation_id);
        ack.session_id = Some(entry.session_id.clone());
        let _ = out.send(ack).await;

        // Forward replayed events on the anti-echo channel (#82); they are
        // already-persisted copies and already carry the session id.
        for logged in replay {
            if let Ok(env) = logged.envelope.try_into_typed() {
                if fwd.send(env).await.is_err() {
                    break;
                }
            }
        }

        let mut state = SessionState::new(entry.session_id.clone(), entry.capabilities);
        state.principal = entry.principal;
        state.phase = HandshakePhase::Accepted;
        Some(state)
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
        principal: Option<String>,
    ) {
        let lease = self.inner.session_lease_seconds.map(|s| SessionLease {
            expires_at: chrono::Utc::now()
                + chrono::Duration::seconds(i64::try_from(s).unwrap_or(i64::MAX)),
        });
        // ARCP v1.1 §6.3: mint a fresh resume token on every welcome and
        // register it so a later `session.resume` can reattach.
        let resume_token = self.register_resume_token(session_id, principal, capabilities);
        let mut env = Envelope::new(MessageType::SessionAccepted(SessionAcceptedPayload {
            session_id: session_id.clone(),
            runtime: self.inner.runtime_identity.clone(),
            capabilities: capabilities.clone(),
            lease,
            resume_token: Some(resume_token),
        }));
        env.correlation_id = Some(correlation_id);
        env.session_id = Some(session_id.clone());
        let _ = out.send(env).await;
    }
    /// Mint and register a fresh `resume_token` for `session_id`,
    /// returning the token (ARCP v1.1 §6.3).
    fn register_resume_token(
        &self,
        session_id: &SessionId,
        principal: Option<String>,
        capabilities: &Capabilities,
    ) -> String {
        let token = format!("rt_{}", MessageId::new());
        self.inner.resume_registry.insert(
            token.clone(),
            ResumeEntry {
                session_id: session_id.clone(),
                principal,
                capabilities: capabilities.clone(),
            },
        );
        token
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
}

/// Intersect two boolean capability slots.
///
/// Returns `None` only when neither side advertised the capability — in
/// that case the field is elided on the wire, matching the ARCP v1.1 §6.2
/// "absent = not negotiated" intersection rule. When at least one side
/// advertised, the result is `Some(both_set)`.
const fn intersect_bool(a: Option<bool>, b: Option<bool>) -> Option<bool> {
    match (a, b) {
        (Some(true), Some(true)) => Some(true),
        (Some(_), _) | (_, Some(_)) => Some(false),
        (None, None) => None,
    }
}
