//! Unit tests for the central message module.
//!
//! Split out of `mod.rs` to keep that module under the audit
//! file-length threshold (#75).

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_panics_doc
)]

use super::*;

#[test]
fn ping_round_trips_through_serde() {
    let m = MessageType::Ping(PingPayload {
        nonce: Some("abc".into()),
    });
    let json = serde_json::to_string(&m).expect("serialize");
    let back: MessageType = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(m, back);
}

#[test]
fn session_ping_round_trips_through_serde() {
    let now = chrono::Utc::now();
    let m = MessageType::SessionPing(SessionPingPayload {
        nonce: "p_01J".into(),
        sent_at: now,
    });
    let json = serde_json::to_string(&m).expect("serialize");
    let back: MessageType = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(m, back);
}

#[test]
fn session_ack_round_trips_through_serde() {
    let m = MessageType::SessionAck(SessionAckPayload {
        last_processed_seq: 1827,
    });
    let json = serde_json::to_string(&m).expect("serialize");
    let back: MessageType = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(m, back);
    let v: serde_json::Value = serde_json::from_str(&json).expect("value");
    assert_eq!(
        v,
        serde_json::json!({
            "type": "session.ack",
            "payload": { "last_processed_seq": 1827 },
        })
    );
}

#[test]
fn session_list_jobs_round_trips_through_serde() {
    let m = MessageType::SessionListJobs(SessionListJobsPayload {
        filter: Some(SessionListJobsFilter {
            status: vec!["running".into()],
            agent: Some("echo".into()),
            created_after: None,
            created_before: None,
        }),
        limit: Some(50),
        cursor: None,
    });
    let json = serde_json::to_string(&m).expect("serialize");
    let back: MessageType = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(m, back);
}

#[test]
fn session_jobs_round_trips_through_serde() {
    let now = chrono::Utc::now();
    let m = MessageType::SessionJobs(SessionJobsPayload {
        request_id: "msg_x".into(),
        jobs: vec![JobListEntry {
            job_id: crate::ids::JobId::new(),
            agent: "echo@1.0.0".into(),
            status: "running".into(),
            parent_job_id: None,
            created_at: now,
            trace_id: None,
            last_event_seq: 0,
        }],
        next_cursor: None,
    });
    let json = serde_json::to_string(&m).expect("serialize");
    let back: MessageType = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(m, back);
}

#[test]
fn countable_event_classification_excludes_session_control() {
    let now = chrono::Utc::now();
    assert!(!MessageType::SessionPing(SessionPingPayload {
        nonce: "n".into(),
        sent_at: now,
    })
    .is_countable_event());
    assert!(!MessageType::SessionAck(SessionAckPayload {
        last_processed_seq: 0,
    })
    .is_countable_event());
    assert!(!MessageType::Ping(PingPayload::default()).is_countable_event());
    assert!(
        MessageType::JobAccepted(crate::messages::JobAcceptedPayload {
            job_id: crate::ids::JobId::new(),
            credentials: vec![],
            lease: None,
        })
        .is_countable_event()
    );
}

#[test]
fn session_pong_round_trips_through_serde() {
    let now = chrono::Utc::now();
    let m = MessageType::SessionPong(SessionPongPayload {
        ping_nonce: "p_01J".into(),
        received_at: now,
    });
    let json = serde_json::to_string(&m).expect("serialize");
    let back: MessageType = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(m, back);
}

#[test]
fn ping_wire_shape_matches_rfc() {
    let m = MessageType::Ping(PingPayload { nonce: None });
    let json = serde_json::to_value(&m).expect("serialize");
    assert_eq!(json, serde_json::json!({"type": "ping", "payload": {}}));
}

#[test]
fn metric_with_standard_name_round_trips() {
    let m = MessageType::Metric(MetricPayload {
        name: standard_names::TOKENS_USED.into(),
        value: 1432.0,
        unit: "tokens".into(),
        dims: Some(serde_json::json!({"model": "claude-3.5", "kind": "input"})),
    });
    let json = serde_json::to_string(&m).expect("serialize");
    let back: MessageType = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(m, back);
}

#[test]
fn type_name_matches_wire_discriminator() {
    let cases = [
        (MessageType::Ping(PingPayload::default()), "ping"),
        (MessageType::Pong(PongPayload::default()), "pong"),
        (
            MessageType::EventEmit(EventEmitPayload {
                name: "x".into(),
                data: None,
            }),
            "event.emit",
        ),
    ];
    for (m, expected) in cases {
        assert_eq!(m.type_name(), expected);
    }
}

#[test]
fn capabilities_default_is_empty() {
    let c = Capabilities::default();
    assert!(!c.has(CapabilityName::Streaming));
    assert!(!c.has(CapabilityName::Anonymous));
}

#[test]
fn capabilities_with_flat_agents_round_trips() {
    // v1.0-compatible shape: agents as a flat array of names.
    let json = serde_json::json!({
        "agents": ["code-refactor", "web-research"],
    });
    let c: Capabilities = serde_json::from_value(json.clone()).expect("deserialize");
    match c.agents.as_ref().expect("agents present") {
        AgentInventory::Flat(names) => {
            assert_eq!(
                names,
                &vec!["code-refactor".to_owned(), "web-research".into()]
            );
        }
        AgentInventory::Rich(_) => panic!("expected flat shape"),
    }
    let re = serde_json::to_value(&c).expect("serialize");
    assert_eq!(re["agents"], json["agents"]);
}

#[test]
fn capabilities_with_rich_agents_round_trips() {
    // v1.1 rich shape: agents as a list of {name, versions, default}.
    let json = serde_json::json!({
        "agents": [
            { "name": "code-refactor", "versions": ["1.0.0", "2.0.0"], "default": "2.0.0" },
            { "name": "indexer", "versions": ["0.9.0"] },
        ],
    });
    let c: Capabilities = serde_json::from_value(json).expect("deserialize");
    let inv = c.agents.as_ref().expect("agents present");
    match inv {
        AgentInventory::Rich(entries) => {
            assert_eq!(entries.len(), 2);
            assert_eq!(entries[0].name, "code-refactor");
            assert_eq!(
                entries[0].versions,
                vec!["1.0.0".to_owned(), "2.0.0".into()]
            );
            assert_eq!(entries[0].default.as_deref(), Some("2.0.0"));
        }
        AgentInventory::Flat(_) => panic!("expected rich shape"),
    }
}

#[test]
fn agent_inventory_satisfies_resolution_rules() {
    let flat = AgentInventory::Flat(vec!["echo".into()]);
    // Flat (v1.0) satisfies any version (runtime didn't enumerate).
    assert!(flat.satisfies(&crate::messages::AgentRef::parse("echo").expect("parse")));
    assert!(flat.satisfies(&crate::messages::AgentRef::parse("echo@1.0.0").expect("parse")));

    let rich = AgentInventory::Rich(vec![AgentInventoryEntry {
        name: "echo".into(),
        versions: vec!["1.0.0".into(), "2.0.0".into()],
        default: Some("2.0.0".into()),
    }]);
    assert!(rich.satisfies(&crate::messages::AgentRef::parse("echo").expect("parse")));
    assert!(rich.satisfies(&crate::messages::AgentRef::parse("echo@1.0.0").expect("parse")));
    assert!(!rich.satisfies(&crate::messages::AgentRef::parse("echo@9.9.9").expect("parse")));
    assert!(!rich.satisfies(&crate::messages::AgentRef::parse("other").expect("parse")));
}

#[test]
fn capabilities_round_trip_with_extra_fields() {
    let json = serde_json::json!({
        "streaming": true,
        "extensions": ["arcpx.example.v1"],
        "totally_made_up": true,
    });
    let c: Capabilities = serde_json::from_value(json).expect("deserialize");
    assert!(c.has(CapabilityName::Streaming));
    assert_eq!(c.extensions, vec!["arcpx.example.v1"]);
    assert!(c.extra.contains_key("totally_made_up"));
}

#[test]
fn unknown_type_fails_deserialize() {
    let bad = "{\"type\":\"never.heard.of.it\",\"payload\":{}}";
    let result: Result<MessageType, _> = serde_json::from_str(bad);
    assert!(result.is_err());
}

#[test]
fn handshake_messages_classified_correctly() {
    assert!(MessageType::SessionOpen(SessionOpenPayload {
        auth: Credentials {
            scheme: AuthScheme::None,
            token: None
        },
        client: ClientIdentity {
            kind: "test".into(),
            version: "0".into(),
            fingerprint: None,
            principal: None
        },
        capabilities: Capabilities::default(),
    })
    .is_handshake());
    assert!(!MessageType::Ping(PingPayload::default()).is_handshake());
}

#[test]
#[allow(clippy::too_many_lines)]
fn type_name_covers_every_variant() {
    // One instance per MessageType variant. Any future variant added
    // to MessageType but not to this list will fall through to the
    // exhaustive match in MessageType::type_name and the test will
    // surface the omission as a 0%-coverage spike on the new arm.
    let now = chrono::Utc::now();
    let cases: Vec<(MessageType, &'static str)> = vec![
        (
            MessageType::SessionOpen(SessionOpenPayload {
                auth: Credentials {
                    scheme: AuthScheme::None,
                    token: None,
                },
                client: ClientIdentity {
                    kind: "t".into(),
                    version: "0".into(),
                    fingerprint: None,
                    principal: None,
                },
                capabilities: Capabilities::default(),
            }),
            "session.open",
        ),
        (
            MessageType::SessionChallenge(SessionChallengePayload {
                challenge: "x".into(),
            }),
            "session.challenge",
        ),
        (
            MessageType::SessionAuthenticate(SessionAuthenticatePayload {
                response: "x".into(),
            }),
            "session.authenticate",
        ),
        (
            MessageType::SessionAccepted(SessionAcceptedPayload {
                session_id: crate::ids::SessionId::new(),
                runtime: crate::messages::RuntimeIdentity {
                    kind: "rt".into(),
                    version: "0".into(),
                    fingerprint: None,
                    trust_level: None,
                },
                capabilities: Capabilities::default(),
                lease: None,
                resume_token: None,
            }),
            "session.accepted",
        ),
        (
            MessageType::SessionUnauthenticated(SessionUnauthenticatedPayload {
                code: crate::error::ErrorCode::Unauthenticated,
                message: "x".into(),
            }),
            "session.unauthenticated",
        ),
        (
            MessageType::SessionRejected(SessionRejectedPayload {
                code: crate::error::ErrorCode::Unauthenticated,
                message: "x".into(),
            }),
            "session.rejected",
        ),
        (
            MessageType::SessionRefresh(SessionRefreshPayload {
                deadline: now,
                challenge: None,
            }),
            "session.refresh",
        ),
        (
            MessageType::SessionEvicted(SessionEvictedPayload {
                code: crate::error::ErrorCode::Cancelled,
                reason: "x".into(),
            }),
            "session.evicted",
        ),
        (
            MessageType::SessionClose(SessionClosePayload::default()),
            "session.close",
        ),
        (
            MessageType::SessionClosed(SessionClosedPayload::default()),
            "session.closed",
        ),
        (
            MessageType::SessionResume(SessionResumePayload {
                resume_token: "rt_x".into(),
                last_event_seq: 0,
            }),
            "session.resume",
        ),
        (
            MessageType::SessionResumed(SessionResumedPayload {
                session_id: crate::ids::SessionId::new(),
                resume_token: "rt_y".into(),
                replayed_from: 0,
                replayed: false,
            }),
            "session.resumed",
        ),
        (
            MessageType::SessionPing(SessionPingPayload {
                nonce: "n".into(),
                sent_at: now,
            }),
            "session.ping",
        ),
        (
            MessageType::SessionPong(SessionPongPayload {
                ping_nonce: "n".into(),
                received_at: now,
            }),
            "session.pong",
        ),
        (
            MessageType::SessionAck(SessionAckPayload {
                last_processed_seq: 0,
            }),
            "session.ack",
        ),
        (
            MessageType::SessionListJobs(SessionListJobsPayload::default()),
            "session.list_jobs",
        ),
        (
            MessageType::SessionJobs(SessionJobsPayload {
                request_id: "r".into(),
                jobs: vec![],
                next_cursor: None,
            }),
            "session.jobs",
        ),
        (MessageType::Ping(PingPayload::default()), "ping"),
        (MessageType::Pong(PongPayload::default()), "pong"),
        (MessageType::Ack(AckPayload { note: None }), "ack"),
        (
            MessageType::Nack(NackPayload {
                code: crate::error::ErrorCode::Unknown,
                message: "x".into(),
                details: None,
            }),
            "nack",
        ),
        (
            MessageType::Cancel(CancelPayload {
                target: CancelTargetKind::Job,
                target_id: "x".into(),
                reason: None,
                deadline_ms: None,
            }),
            "cancel",
        ),
        (
            MessageType::CancelAccepted(CancelAcceptedPayload { target_id: None }),
            "cancel.accepted",
        ),
        (
            MessageType::CancelRefused(CancelRefusedPayload {
                target_id: "x".into(),
                reason: "x".into(),
            }),
            "cancel.refused",
        ),
        (
            MessageType::Interrupt(InterruptPayload {
                target: CancelTargetKind::Job,
                target_id: "x".into(),
                prompt: "x".into(),
            }),
            "interrupt",
        ),
        (MessageType::Resume(ResumePayload::default()), "resume"),
        (
            MessageType::Backpressure(BackpressurePayload {
                desired_rate_per_second: None,
                buffer_remaining_bytes: None,
                reason: None,
            }),
            "backpressure",
        ),
        (
            MessageType::ToolInvoke(ToolInvokePayload::new("x", serde_json::json!({}))),
            "tool.invoke",
        ),
        (
            MessageType::ToolResult(ToolResultPayload {
                value: None,
                result_ref: None,
            }),
            "tool.result",
        ),
        (
            MessageType::ToolError(ToolErrorPayload {
                code: crate::error::ErrorCode::Internal,
                retryable: None,
                message: "x".into(),
                details: None,
            }),
            "tool.error",
        ),
        (
            MessageType::JobAccepted(JobAcceptedPayload {
                job_id: crate::ids::JobId::new(),
                credentials: vec![],
                lease: None,
            }),
            "job.accepted",
        ),
        (
            MessageType::JobStarted(JobStartedPayload { description: None }),
            "job.started",
        ),
        (
            MessageType::JobProgress(JobProgressPayload {
                current: 0.0,
                total: None,
                units: None,
                message: None,
            }),
            "job.progress",
        ),
        (
            MessageType::JobHeartbeat(JobHeartbeatPayload {
                sequence: 1,
                deadline_ms: None,
                state: JobState::Running,
            }),
            "job.heartbeat",
        ),
        (
            MessageType::JobCompleted(JobCompletedPayload {
                value: None,
                result_ref: None,
                result_id: None,
                result_size: None,
                summary: None,
            }),
            "job.completed",
        ),
        (
            MessageType::JobResultChunk(JobResultChunkPayload {
                result_id: "r".into(),
                chunk_seq: 0,
                data: "x".into(),
                encoding: crate::messages::ResultChunkEncoding::Utf8,
                more: false,
            }),
            "job.result_chunk",
        ),
        (
            MessageType::JobFailed(JobFailedPayload {
                code: crate::error::ErrorCode::Internal,
                retryable: None,
                message: "x".into(),
                details: None,
            }),
            "job.failed",
        ),
        (
            MessageType::JobCancelled(JobCancelledPayload { reason: None }),
            "job.cancelled",
        ),
        (
            MessageType::StreamOpen(StreamOpenPayload {
                kind: StreamKind::Text,
                content_type: None,
                encoding: None,
            }),
            "stream.open",
        ),
        (
            MessageType::StreamChunk(StreamChunkPayload {
                sequence: 0,
                data: serde_json::json!(""),
                content_type: None,
                sha256: None,
                redacted: false,
                role: None,
            }),
            "stream.chunk",
        ),
        (
            MessageType::StreamClose(StreamClosePayload::default()),
            "stream.close",
        ),
        (
            MessageType::StreamError(StreamErrorPayload {
                code: crate::error::ErrorCode::Internal,
                message: "x".into(),
            }),
            "stream.error",
        ),
        (
            MessageType::PermissionRequest(PermissionRequestPayload {
                permission: "p".into(),
                resource: "r".into(),
                operation: "o".into(),
                reason: None,
                requested_lease_seconds: None,
            }),
            "permission.request",
        ),
        (
            MessageType::PermissionGrant(PermissionGrantPayload {
                permission: "p".into(),
                resource: "r".into(),
                operation: "o".into(),
                lease_seconds: 1,
            }),
            "permission.grant",
        ),
        (
            MessageType::PermissionDeny(PermissionDenyPayload {
                permission: "p".into(),
                reason: "x".into(),
            }),
            "permission.deny",
        ),
        (
            MessageType::LeaseGranted(LeaseGrantedPayload {
                lease_id: crate::ids::LeaseId::new(),
                permission: "p".into(),
                resource: "r".into(),
                operation: "o".into(),
                expires_at: now,
            }),
            "lease.granted",
        ),
        (
            MessageType::LeaseExtended(LeaseExtendedPayload {
                lease_id: crate::ids::LeaseId::new(),
                expires_at: now,
            }),
            "lease.extended",
        ),
        (
            MessageType::LeaseRevoked(LeaseRevokedPayload {
                lease_id: crate::ids::LeaseId::new(),
                reason: "x".into(),
            }),
            "lease.revoked",
        ),
        (
            MessageType::LeaseRefresh(LeaseRefreshPayload {
                lease_id: crate::ids::LeaseId::new(),
                additional_seconds: 1,
            }),
            "lease.refresh",
        ),
        (
            MessageType::Subscribe(SubscribePayload::default()),
            "subscribe",
        ),
        (
            MessageType::SubscribeAccepted(SubscribeAcceptedPayload {
                subscription_id: crate::ids::SubscriptionId::new(),
            }),
            "subscribe.accepted",
        ),
        (
            MessageType::SubscribeEvent(SubscribeEventPayload {
                event: serde_json::json!({}),
            }),
            "subscribe.event",
        ),
        (
            MessageType::Unsubscribe(UnsubscribePayload {
                subscription_id: crate::ids::SubscriptionId::new(),
            }),
            "unsubscribe",
        ),
        (
            MessageType::SubscribeClosed(SubscribeClosedPayload {
                subscription_id: crate::ids::SubscriptionId::new(),
                code: crate::error::ErrorCode::Cancelled,
                reason: "x".into(),
            }),
            "subscribe.closed",
        ),
        (
            MessageType::JobSubscribe(JobSubscribePayload {
                job_id: crate::ids::JobId::new(),
                from_event_seq: None,
                history: false,
            }),
            "job.subscribe",
        ),
        (
            MessageType::JobSubscribed(JobSubscribedPayload {
                job_id: crate::ids::JobId::new(),
                current_status: "running".into(),
                agent: "echo".into(),
                parent_job_id: None,
                trace_id: None,
                subscribed_from: 0,
                replayed: false,
            }),
            "job.subscribed",
        ),
        (
            MessageType::JobUnsubscribe(JobUnsubscribePayload {
                job_id: crate::ids::JobId::new(),
            }),
            "job.unsubscribe",
        ),
        (
            MessageType::ArtifactPut(ArtifactPutPayload {
                media_type: "x".into(),
                data: String::new(),
                sha256: None,
                retain_seconds: None,
            }),
            "artifact.put",
        ),
        (
            MessageType::ArtifactFetch(ArtifactFetchPayload {
                artifact_id: crate::ids::ArtifactId::new(),
            }),
            "artifact.fetch",
        ),
        (
            MessageType::ArtifactRef(ArtifactRefPayload {
                artifact: ArtifactRef {
                    artifact_id: crate::ids::ArtifactId::new(),
                    uri: "arcp://x".into(),
                    media_type: "x".into(),
                    size: 0,
                    sha256: None,
                    expires_at: None,
                },
            }),
            "artifact.ref",
        ),
        (
            MessageType::ArtifactRelease(ArtifactReleasePayload {
                artifact_id: crate::ids::ArtifactId::new(),
            }),
            "artifact.release",
        ),
        (
            MessageType::EventEmit(EventEmitPayload {
                name: "x".into(),
                data: None,
            }),
            "event.emit",
        ),
        (
            MessageType::Log(LogPayload {
                level: LogLevel::Info,
                message: "x".into(),
                attributes: None,
            }),
            "log",
        ),
        (
            MessageType::Metric(MetricPayload {
                name: "x".into(),
                value: 0.0,
                unit: "u".into(),
                dims: None,
            }),
            "metric",
        ),
        (
            MessageType::TraceSpan(TraceSpanPayload {
                name: "x".into(),
                trace_id: crate::ids::TraceId::new("t").expect("non-empty"),
                span_id: crate::ids::SpanId::new("s").expect("non-empty"),
                parent_span_id: None,
                start_time: now,
                end_time: now,
                attributes: None,
            }),
            "trace.span",
        ),
    ];
    for (msg, expected) in &cases {
        assert_eq!(msg.type_name(), *expected);
    }
    // Message variants — sanity-check we built exactly that many.
    // Bump this when MessageType grows.
    assert_eq!(cases.len(), 65);
}
