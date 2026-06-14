//! Job invocation, cancellation, and result finalization (split, #74).

#[allow(clippy::wildcard_imports)]
use super::*;

impl ARCPRuntime {
    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    pub(crate) async fn spawn_tool_invoke(
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
                // ARCP v1.1 §7.2 / §12: a reused idempotency key with
                // conflicting parameters returns DUPLICATE_KEY (non-retryable).
                let mut err = Envelope::new(MessageType::JobFailed(JobFailedPayload {
                    code: ErrorCode::DuplicateKey,
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

        // ARCP v1.1 §9.5: a lease `expires_at` MUST be UTC and strictly in
        // the future at submission. Past / equal-to-now values are rejected
        // with INVALID_REQUEST before any job.accepted is emitted.
        if let Some(lease_ref) = lease.as_ref() {
            if let Err(e) = lease_ref.validate() {
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
            // ARCP v1.1 §12: an unregistered agent name is AGENT_NOT_AVAILABLE,
            // distinct from the generic NOT_FOUND and from the
            // version-mismatch sibling AGENT_VERSION_NOT_AVAILABLE.
            let mut err = Envelope::new(MessageType::JobFailed(JobFailedPayload {
                code: ErrorCode::AgentNotAvailable,
                retryable: Some(false),
                message: format!("agent not available: {}", agent_ref.name),
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
        // ARCP v1.1 §9.5: capture the lease deadline (if any) so the task
        // can race the handler against it and proactively surface
        // LEASE_EXPIRED. `DateTime<Utc>` is `Copy`, so this does not move
        // `lease` (which is consumed by the ToolContext below).
        let lease_expires_at = lease.as_ref().and_then(|l| l.expires_at);
        // ARCP v1.1 §8.4: shared result-stream state so the runtime can
        // enforce chunk ordering and the stream-then-inline ban at
        // completion. One handle goes into the ToolContext; the other
        // stays here for the completion check.
        let result_stream = Arc::new(std::sync::Mutex::new(
            crate::runtime::context::ResultStreamState::default(),
        ));
        let result_stream_for_ctx = Arc::clone(&result_stream);
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
                result_stream: result_stream_for_ctx,
            };

            // §9.5: a future that resolves when the lease deadline passes.
            // Absent `expires_at` parks forever so the arm never fires.
            let lease_expiry_future = async move {
                match lease_expires_at {
                    Some(expires_at) => {
                        let remaining = expires_at - chrono::Utc::now();
                        let delay = remaining.to_std().unwrap_or(std::time::Duration::ZERO);
                        tokio::time::sleep(delay).await;
                        expires_at
                    }
                    None => std::future::pending::<chrono::DateTime<chrono::Utc>>().await,
                }
            };

            let outcome = tokio::select! {
                () = cancel_for_task.cancelled() => Outcome::Cancelled("cancellation token fired".into()),
                expires_at = lease_expiry_future => Outcome::LeaseExpired { expires_at },
                result = handler.invoke(payload.arguments, ctx) => match result {
                    Ok(value) => Outcome::Completed(value),
                    Err(ARCPError::Cancelled { reason }) => Outcome::Cancelled(reason),
                    Err(e) => Outcome::Failed(e),
                },
            };

            let terminal = match outcome {
                Outcome::Completed(value) => {
                    // Allow agents that stream results to indicate the
                    // terminating job.completed should reference a
                    // `result_id` (ARCP v1.1 §8.4) by returning the
                    // sentinel shape `{ "$arcp_streamed_result": {
                    // result_id, result_size?, summary? } }`. Everything
                    // else flows through as `value` (the v1.0 path).
                    //
                    // §8.4: if the handler emitted any result_chunk, the
                    // completion MUST reference the same result_id and MUST
                    // NOT be inline. The runtime enforces this here.
                    match finalize_streamed_completion(&result_stream, value) {
                        Ok(completed) => {
                            jobs_clone.set_state(&job_id, JobState::Completed);
                            MessageType::JobCompleted(completed)
                        }
                        Err(e) => {
                            jobs_clone.set_state(&job_id, JobState::Failed);
                            MessageType::JobFailed(JobFailedPayload {
                                code: e.code(),
                                retryable: Some(e.retryable()),
                                message: e.to_string(),
                                details: None,
                            })
                        }
                    }
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
                Outcome::LeaseExpired { expires_at } => {
                    // ARCP v1.1 §9.5: at or after expires_at the runtime MUST
                    // surface LEASE_EXPIRED with retryable:false; §9.5 also
                    // permits proactive termination of jobs whose leases have
                    // expired. Renewal is NOT supported.
                    jobs_clone.set_state(&job_id, JobState::Failed);
                    MessageType::JobFailed(JobFailedPayload {
                        code: ErrorCode::LeaseExpired,
                        retryable: Some(false),
                        message: format!(
                            "lease expired at {expires_at} (ARCP v1.1 §9.5); renewal is NOT \
                             supported — resubmit with a fresh lease"
                        ),
                        details: None,
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
    pub(crate) async fn handle_cancel(
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
}

enum Outcome {
    Completed(serde_json::Value),
    Failed(ARCPError),
    Cancelled(String),
    /// ARCP v1.1 §9.5: the lease's `expires_at` was reached while the job
    /// was still active.
    LeaseExpired {
        expires_at: chrono::DateTime<chrono::Utc>,
    },
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

/// Build the terminal [`JobCompletedPayload`], enforcing the ARCP v1.1
/// §8.4 stream invariants against the recorded result-stream state.
///
/// If the handler emitted any `result_chunk`, the completion MUST
/// reference the same `result_id` (via the streaming sentinel) and the
/// stream MUST have been terminated with `more: false`; a plain inline
/// value after streaming is a protocol violation. When no chunk was
/// emitted, the value flows through the normal inline / sentinel path.
fn finalize_streamed_completion(
    result_stream: &std::sync::Mutex<crate::runtime::context::ResultStreamState>,
    value: serde_json::Value,
) -> Result<JobCompletedPayload, ARCPError> {
    let guard = result_stream.lock().map_err(|_| ARCPError::Internal {
        detail: "result stream mutex poisoned".into(),
    })?;
    let Some(active_id) = guard.active_result_id() else {
        // No chunks streamed — inline or sentinel completion is fine.
        return Ok(streamed_result_from_value(value));
    };
    let completed = streamed_result_from_value(value);
    match completed.result_id.as_deref() {
        Some(rid) if rid == active_id => {
            if guard.is_finished() {
                Ok(completed)
            } else {
                Err(ARCPError::FailedPrecondition {
                    detail: format!(
                        "result stream {active_id} was not terminated with a more:false chunk \
                         before job.completed (§8.4)"
                    ),
                })
            }
        }
        _ => Err(ARCPError::FailedPrecondition {
            detail: format!(
                "job streamed result_chunk(s) for {active_id} but completed with an inline / \
                 mismatched result; the terminal result MUST carry the matching result_id (§8.4)"
            ),
        }),
    }
}

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
