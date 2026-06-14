//! Generic subscribe and job.subscribe handling (split, #74).

#[allow(clippy::wildcard_imports)]
use super::*;

impl ARCPRuntime {
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn handle_subscribe(
        out: &mpsc::Sender<Envelope>,
        manager: &SubscriptionManager,
        session_principals: &Arc<DashMap<SessionId, Option<String>>>,
        connection_subs: &Arc<DashMap<SubscriptionId, JoinHandle<()>>>,
        correlation_id: MessageId,
        session_id: SessionId,
        principal: Option<String>,
        payload: SubscribePayload,
    ) {
        let SubscribePayload { filter, since: _ } = payload;
        // ARCP v1.1 §14: generic subscriptions MUST default to
        // "same principal only" and MUST NOT let an authenticated session
        // observe another principal's events without explicit policy.
        //
        // An explicit session-scoped filter may only name sessions owned by
        // the caller's own principal (the caller's session always
        // qualifies). Any other principal's session is rejected up front.
        for named in &filter.session_id {
            if *named == session_id {
                continue;
            }
            let named_principal = session_principals
                .get(named)
                .and_then(|p| p.value().clone());
            let permitted = match (&principal, &named_principal) {
                (Some(caller), Some(owner)) => caller == owner,
                _ => false,
            };
            if !permitted {
                let mut err = Envelope::new(MessageType::Nack(NackPayload {
                    code: ErrorCode::PermissionDenied,
                    message: "subscription filter names a session owned by another principal"
                        .into(),
                    details: None,
                }));
                err.correlation_id = Some(correlation_id);
                err.session_id = Some(session_id);
                let _ = out.send(err).await;
                return;
            }
        }
        let (subscription_id, mut rx) = manager.register(filter, session_id.clone());
        // Acknowledge the subscription.
        let mut accepted =
            Envelope::new(MessageType::SubscribeAccepted(SubscribeAcceptedPayload {
                subscription_id: subscription_id.clone(),
            }));
        accepted.correlation_id = Some(correlation_id);
        accepted.session_id = Some(session_id.clone());
        accepted.subscription_id = Some(subscription_id.clone());
        let _ = out.send(accepted).await;

        // Spawn a forwarder task that wraps each delivered envelope in a
        // subscribe.event and pushes to the outbound channel. Backfill
        // (the §13.3 boundary marker) is left for a follow-up.
        //
        // ARCP v1.1 §14 same-principal scoping is enforced HERE, at
        // delivery: an envelope is forwarded only if its publishing
        // session belongs to the subscriber's own session or to a session
        // owned by the same authenticated principal. This is checked at
        // delivery (not just registration) so sessions that appear after
        // the subscription, and anonymous principals, are handled
        // correctly.
        let out_clone = out.clone();
        let sub_id = subscription_id.clone();
        let principals = Arc::clone(session_principals);
        let subscriber_session = session_id.clone();
        let subscriber_principal = principal;
        let join = tokio::spawn(async move {
            while let Some(event) = rx.next().await {
                if !subscription_scope_permits(
                    &principals,
                    &subscriber_session,
                    subscriber_principal.as_deref(),
                    event.session_id.as_ref(),
                ) {
                    continue;
                }
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
    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    pub(crate) async fn handle_job_subscribe(
        out: &mpsc::Sender<Envelope>,
        fwd: &mpsc::Sender<Envelope>,
        manager: &SubscriptionManager,
        jobs: &JobRegistry,
        event_log: &EventLog,
        session_principals: &DashMap<SessionId, Option<String>>,
        connection_job_subs: &Arc<DashMap<JobId, JoinHandle<()>>>,
        correlation_id: MessageId,
        subscriber_session: SessionId,
        subscriber_principal: Option<String>,
        payload: JobSubscribePayload,
    ) {
        let JobSubscribePayload {
            job_id,
            from_event_seq,
            history,
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

        // Build a filter that selects only this job's envelopes. Register
        // the live subscription BEFORE reading replay history so no live
        // event emitted during replay is lost (the broadcast buffer holds
        // them until the forwarder attaches; duplicates are deduped by
        // `event_seq` below).
        let filter = arcp_core::messages::SubscriptionFilter {
            job_id: vec![job_id.clone()],
            ..arcp_core::messages::SubscriptionFilter::default()
        };
        let (_internal_id, mut rx) = manager.register(filter, subscriber_session.clone());

        // ARCP v1.1 §7.6 history replay: when the subscriber requests
        // `history: true` with `from_event_seq`, replay buffered events
        // with `event_seq > from_event_seq` before live streaming.
        let replay_from = from_event_seq.unwrap_or(0);
        let replay_events = if history {
            event_log
                .replay_job_events_after_seq(job_id.as_str(), replay_from, REPLAY_LIMIT)
                .await
                .unwrap_or_else(|e| {
                    tracing::warn!(error = %e, job_id = %job_id, "job.subscribe history replay failed");
                    Vec::new()
                })
        } else {
            Vec::new()
        };
        let replay_high_water = replay_events
            .iter()
            .filter_map(|ev| ev.envelope.event_seq)
            .max()
            .unwrap_or(replay_from);
        let replayed = !replay_events.is_empty();

        // Acknowledge. `replayed` reflects whether buffered events were
        // replayed; `subscribed_from` is the seq live streaming resumes
        // after (the replay high-water mark when replaying).
        let ack = JobSubscribedPayload {
            job_id: job_id.clone(),
            current_status: snap.state.wire_str().to_owned(),
            agent: snap.agent.clone(),
            parent_job_id: snap.parent_job_id.clone(),
            trace_id: None,
            subscribed_from: if replayed {
                replay_high_water
            } else {
                snap.last_event_seq
            },
            replayed,
        };
        let mut ack_env = Envelope::new(MessageType::JobSubscribed(ack));
        ack_env.correlation_id = Some(correlation_id);
        ack_env.session_id = Some(subscriber_session.clone());
        ack_env.job_id = Some(job_id.clone());
        let _ = out.send(ack_env).await;

        // Replay buffered events (in seq order) before live streaming.
        // These are already-persisted copies, so they go on the dedicated
        // forwarding channel (§82 anti-echo) with the session id rewritten.
        for logged in replay_events {
            let Ok(mut env) = logged.envelope.try_into_typed() else {
                continue;
            };
            if !is_forwardable_job_event(&env.payload) {
                continue;
            }
            env.session_id = Some(subscriber_session.clone());
            if fwd.send(env).await.is_err() {
                return;
            }
        }

        // Spawn forwarder: rewrites session_id to the subscriber's so
        // client-side parsers route correctly. The originating session's
        // own writer is responsible for the submitter's copy; here we
        // only fan out a clone to the subscriber.
        //
        // Forwarded copies are sent on the dedicated forwarding channel
        // (`fwd`), NOT the main outbound channel. The writer delivers them
        // verbatim without re-publishing to the subscription bus, so a
        // forwarded `job.completed` cannot re-match this filter and
        // amplify into an echo storm (#82, §7.6).
        let fwd_clone = fwd.clone();
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
                // Dedup against replayed history: any countable event whose
                // seq was already replayed must not be delivered twice.
                if let Some(seq) = env.event_seq {
                    if seq <= replay_high_water {
                        continue;
                    }
                }
                env.session_id = Some(subscriber_session_clone.clone());
                if fwd_clone.send(env).await.is_err() {
                    break;
                }
            }
            // Forwarder exited (job terminal or unsubscribe).
            connection_job_subs_clone.remove(&job_id_clone);
        });
        connection_job_subs.insert(job_id, join);
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

/// ARCP v1.1 §14 same-principal subscription scope check.
///
/// Returns `true` when an envelope published by `event_session` may be
/// delivered to a generic subscriber identified by `subscriber_session` /
/// `subscriber_principal`. The subscriber always sees its own session; it
/// also sees sessions owned by the same non-anonymous principal. Anonymous
/// subscribers (no principal) see only their own session, and envelopes
/// with no session id are never delivered.
fn subscription_scope_permits(
    session_principals: &DashMap<SessionId, Option<String>>,
    subscriber_session: &SessionId,
    subscriber_principal: Option<&str>,
    event_session: Option<&SessionId>,
) -> bool {
    let Some(event_session) = event_session else {
        return false;
    };
    if event_session == subscriber_session {
        return true;
    }
    let Some(subscriber_principal) = subscriber_principal else {
        return false;
    };
    session_principals
        .get(event_session)
        .and_then(|p| p.value().clone())
        .is_some_and(|owner| owner == subscriber_principal)
}
