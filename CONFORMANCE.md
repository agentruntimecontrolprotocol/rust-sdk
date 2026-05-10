# ARCP v1.0 Conformance Matrix — `arcp` crate

This document tracks the implemented vs. deferred status of every section of
[`RFC-0001-v2.md`](./RFC-0001-v2.md). It is updated at every phase gate.

Legend: ✅ implemented · 🟡 partial · ⏳ deferred to v0.2 · ➖ not applicable

| §    | Section                            | Status |
| ---- | ---------------------------------- | ------ |
| 1    | Goals                              | ➖ |
| 2    | Non-Goals                          | ➖ |
| 3    | Terminology                        | ➖ |
| 4    | Design Principles                  | ⏳ |
| 5    | Architecture                       | ⏳ |
| 6.1  | Envelope                           | ✅ |
| 6.2  | Message Types                      | 🟡 (skeleton: ping/pong/event.emit/log/metric; rest in Phase 2) |
| 6.3  | Command/Result/Event Flow          | ⏳ |
| 6.4  | Delivery Semantics                 | 🟡 (event log idempotency by `(session_id, id)`; logical idempotency table reserved) |
| 6.5  | Priority & QoS                     | 🟡 (priority field round-trips; scheduling/shedding in later phases) |
| 7    | Capability Negotiation             | ⏳ |
| 8.1  | Session Establishment              | ⏳ |
| 8.2  | Credentials (`bearer`,`signed_jwt`,`none`) | ⏳ |
| 8.2  | Credentials (`mtls`,`oauth2`)      | ⏳ v0.2 |
| 8.3  | Runtime Identity                   | ⏳ |
| 8.4  | Re-authentication                  | ⏳ |
| 8.5  | Eviction                           | ⏳ |
| 9    | Sessions (stateless, stateful)     | ⏳ |
| 9    | Sessions (durable across reconnect)| ⏳ v0.2 |
| 10.1 | Durable Jobs                       | ⏳ |
| 10.2 | Job States                         | ⏳ |
| 10.3 | Heartbeats                         | ⏳ |
| 10.4 | Cancellation                       | ⏳ |
| 10.5 | Interrupts                         | ⏳ |
| 10.6 | Scheduled Jobs                     | ⏳ v0.2 |
| 11.1 | Stream Kinds                       | ⏳ |
| 11.2 | Backpressure                       | ⏳ |
| 11.3 | Binary Encoding (base64 in-envelope) | ⏳ |
| 11.3 | Binary Encoding (sidecar frames)   | ⏳ v0.2 |
| 11.4 | Reasoning Streams                  | ⏳ |
| 12.1 | Human Input Requests               | ⏳ |
| 12.2 | Choice Requests                    | ⏳ |
| 12.3 | Provenance / multi-channel (first-wins) | ⏳ |
| 12.3 | Provenance / multi-channel (quorum) | ⏳ v0.2 |
| 12.4 | Expiration with default fallback   | ⏳ |
| 13.1 | Subscribe                          | ⏳ |
| 13.2 | Filtering                          | ⏳ |
| 13.3 | Backfill                           | ⏳ |
| 13.4 | Termination                        | ⏳ |
| 14   | Multi-Agent Coordination           | ⏳ v0.2 |
| 15.1 | Permission Model                   | ⏳ |
| 15.2 | Sandboxing                         | ➖ (runtime concern) |
| 15.3 | Trust Levels                       | ⏳ |
| 15.4 | Permission Challenge Flow          | ⏳ |
| 15.5 | Lease Lifecycle                    | ⏳ |
| 15.6 | Trust Elevation                    | ⏳ v0.2 |
| 16.1 | Artifact References                | ⏳ |
| 16.2 | Storage & Retrieval (inline base64)| ⏳ |
| 16.3 | Lifecycle / retention sweep        | ⏳ |
| 17.1 | Tracing (`tracing` crate)          | ⏳ |
| 17.2 | Structured Logs                    | ⏳ |
| 17.3 | Metrics + standard names           | ⏳ |
| 18.1 | Error Envelope                     | 🟡 (`ARCPError` and `ErrorCode` taxonomy in place; envelope wrapper in Phase 2) |
| 18.2 | Canonical Codes                    | ✅ |
| 18.3 | Retryability & Backoff             | ✅ (`ErrorCode::retryable`) |
| 19   | Resumability (after_message_id)    | 🟡 (event log `list(session, after_rowid, limit)` ready; runtime wiring in Phase 5) |
| 19   | Resumability (checkpoint)          | ⏳ v0.2 |
| 20   | MCP Compatibility                  | ➖ (advisory) |
| 21   | Extensions                         | ✅ (namespace validation + classifier) |
| 22   | Reference Transports (WS, stdio)   | ⏳ |
| 22   | Reference Transports (HTTP/2,QUIC) | ⏳ v0.2 |
