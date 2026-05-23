<h3 align="center">ARCP Rust SDK</h3>

<p align="center"><strong>Rust SDK for the Agent Runtime Control Protocol (ARCP) — submit, observe, and control long-running agent jobs from Rust.</strong></p>

<p align="center">
  <a href="https://crates.io/crates/arcp"><img alt="crates.io" src="https://img.shields.io/crates/v/arcp.svg"></a>
  <a href="https://docs.rs/arcp"><img alt="docs.rs" src="https://docs.rs/arcp/badge.svg"></a>
  <a href="https://github.com/agentruntimecontrolprotocol/rust-sdk/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/agentruntimecontrolprotocol/rust-sdk/actions/workflows/ci.yml/badge.svg"></a>
  <a href="https://github.com/agentruntimecontrolprotocol/spec/blob/main/docs/draft-arcp-1.1.md"><img alt="ARCP" src="https://img.shields.io/badge/ARCP-v1.1%20draft-blue"></a>
  <a href="LICENSE"><img alt="License" src="https://img.shields.io/badge/license-Apache--2.0-lightgrey"></a>
  <a href="https://coderabbit.ai"><img alt="CodeRabbit" src="https://img.shields.io/coderabbit/prs/github/agentruntimecontrolprotocol/rust-sdk?utm_source=oss&utm_medium=github&utm_campaign=agentruntimecontrolprotocol/rust-sdk&labelColor=171717&color=FF570A&label=CodeRabbit+Reviews"></a>
</p>

<p align="center">
  <a href="https://github.com/agentruntimecontrolprotocol/spec/blob/main/docs/draft-arcp-1.1.md">Specification</a> ·
  <a href="#concepts">Concepts</a> ·
  <a href="#installation">Install</a> ·
  <a href="#quick-start">Quick start</a> ·
  <a href="docs/">Guides</a> ·
  <a href="https://docs.rs/arcp">API reference</a>
</p>

---

`arcp` is the Rust reference implementation of [ARCP](https://github.com/agentruntimecontrolprotocol/spec/blob/main/docs/draft-arcp-1.1.md), the Agent Runtime Control Protocol. It covers both sides of the wire — `ARCPClient` for submitting and observing jobs, `ARCPRuntime` for hosting agents and tools — so either side can talk to any conformant peer in any language without hand-rolling the envelope, sequencing, or lease enforcement.

ARCP itself is a transport-agnostic wire protocol for long-running AI agent jobs. It owns the parts of agent infrastructure that don't change between products — sessions, durable event streams, capability leases, budgets, resume — and stays out of the parts that do. ARCP wraps the agent function; it does not define how agents are built, how tools are exposed (that's MCP), or how telemetry is exported (that's OpenTelemetry).

## Installation

Requires Rust 1.88 or later (the MSRV declared in `Cargo.toml`). The crate is on [crates.io](https://crates.io/crates/arcp); default features ship the WebSocket and stdio transports, and the in-memory transport is always available for tests.

```sh
cargo add arcp
```

To drop the WebSocket dependency and keep only stdio plus the in-memory transport:

```toml
[dependencies]
arcp = { version = "1.1", default-features = false, features = ["transport-stdio"] }
```

## Quick start

Connect to a runtime, submit a job, stream its events to completion:

```rust
use std::time::Duration;

use arcp::error::ARCPError;
use arcp::messages::{
    AuthScheme, Capabilities, ClientIdentity, Credentials, MessageType, SessionOpenPayload,
    ToolInvokePayload,
};
use arcp::transport::{Transport, WebSocketTransport};
use arcp::Envelope;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let transport = WebSocketTransport::dial("wss://runtime.example.com/arcp").await?;

    let mut open = Envelope::new(MessageType::SessionOpen(SessionOpenPayload {
        auth: Credentials { scheme: AuthScheme::Bearer, token: Some(std::env::var("ARCP_TOKEN")?) },
        client: ClientIdentity {
            kind: "quickstart".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            fingerprint: None,
            principal: None,
        },
        capabilities: Capabilities::default(),
    }));
    transport.send(open).await?;
    let MessageType::SessionAccepted(welcome) = transport.recv().await?.ok_or("eof")?.payload
    else {
        return Err("expected session.accepted".into());
    };

    let mut invoke = Envelope::new(MessageType::ToolInvoke(ToolInvokePayload::new(
        "data-analyzer",
        serde_json::json!({ "dataset": "s3://example/sales.csv" }),
    )));
    invoke.session_id = Some(welcome.session_id);
    transport.send(invoke).await?;

    while let Some(env) = tokio::time::timeout(Duration::from_secs(30), transport.recv()).await?? {
        match env.payload {
            MessageType::JobAccepted(p) => println!("accepted: {}", p.job_id),
            MessageType::JobCompleted(p) => { println!("done: {:?}", p.value); break; }
            MessageType::JobFailed(p) => return Err(format!("{}: {}", p.code, p.message).into()),
            other => println!("[seq={:?}] {}", env.event_seq, other.type_name()),
        }
    }
    transport.close().await?;
    Ok(())
}
```

This is the whole shape of the SDK: open a session, submit work, consume an ordered event stream, get a terminal result or error. Everything below is detail on those four moves.

## Concepts

ARCP organizes everything around four concerns — **identity**, **durability**, **authority**, and **observability** — expressed through five core objects:

- **Session** — a connection between a client and a runtime. A session carries identity (a bearer token), negotiates a feature set in a `hello`/`welcome` handshake, and is *resumable*: if the transport drops, you reconnect with a resume token and the runtime replays buffered events. Jobs outlive the session that started them. See [§6](https://github.com/agentruntimecontrolprotocol/spec/blob/main/docs/draft-arcp-1.1.md).
- **Job** — one unit of agent work submitted into a session. A job has an identity, an optional idempotency key, a resolved agent version, and a lifecycle that ends in exactly one terminal state: `success`, `error`, `cancelled`, or `timed_out`. See [§7](https://github.com/agentruntimecontrolprotocol/spec/blob/main/docs/draft-arcp-1.1.md).
- **Event** — the ordered, session-scoped stream a job emits: logs, thoughts, tool calls and results, status, metrics, artifact references, progress, and streamed result chunks. Events carry strictly monotonic sequence numbers so the stream survives reconnects gap-free. See [§8](https://github.com/agentruntimecontrolprotocol/spec/blob/main/docs/draft-arcp-1.1.md).
- **Lease** — the authority a job runs under, expressed as capability grants (`fs.read`, `fs.write`, `net.fetch`, `tool.call`, `agent.delegate`, `cost.budget`, `model.use`). The runtime enforces the lease at every operation boundary; a job can never act outside it. Leases may carry a budget and an expiry, and may be subset and handed to sub-agents via delegation. See [§9](https://github.com/agentruntimecontrolprotocol/spec/blob/main/docs/draft-arcp-1.1.md).
- **Subscription** — read-only attachment to a job started elsewhere (e.g. a dashboard watching a job a CLI submitted). A subscriber observes the live event stream but cannot cancel or mutate the job. Distinct from *resume*, which continues the original session and carries cancel authority. See [§7.6](https://github.com/agentruntimecontrolprotocol/spec/blob/main/docs/draft-arcp-1.1.md).

The SDK models each of these as first-class objects; the rest of this README shows how.

## Guides

### Sessions and resume

Open a session, negotiate features, and reconnect transparently after a transport drop using the resume token — jobs keep running server-side while you're gone.

```rust
use arcp::messages::{
    AuthScheme, Capabilities, ClientIdentity, Credentials, MessageType, SessionOpenPayload,
};
use arcp::transport::{Transport, WebSocketTransport};
use arcp::Envelope;

let transport = WebSocketTransport::dial("wss://runtime.example.com/arcp").await?;
let mut open = Envelope::new(MessageType::SessionOpen(SessionOpenPayload {
    auth: Credentials { scheme: AuthScheme::Bearer, token: Some(token.clone()) },
    client: ClientIdentity {
        kind: "resumable".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        fingerprint: None,
        principal: None,
    },
    capabilities: Capabilities::default(),
}));
transport.send(open).await?;
let MessageType::SessionAccepted(welcome) = transport.recv().await?.ok_or("eof")?.payload else {
    return Err("expected session.accepted".into());
};
let session_id = welcome.session_id.clone();
let mut last_seq: u64 = 0;

// ... read envelopes, tracking the highest env.event_seq in `last_seq` ...
// ... transport drops ...

// Reconnect on a fresh transport and resume from `last_seq`:
let transport = WebSocketTransport::dial("wss://runtime.example.com/arcp").await?;
// Re-open the session, then send `session.ack { last_processed_seq: last_seq }`
// so the runtime trims its buffer; buffered envelopes with seq > last_seq
// will be replayed before live streaming resumes.
```

### Submitting jobs

Submit a job with an agent (optionally version-pinned as `name@version`), an input, and an optional lease request, idempotency key, and runtime limit.

```rust
use chrono::{Duration, Utc};

use arcp::messages::{
    CostBudget, CostBudgetAmount, LeaseRequest, MessageType, ToolInvokePayload,
};
use arcp::Envelope;

let lease = LeaseRequest {
    cost_budget: Some(CostBudget {
        amounts: vec![CostBudgetAmount { currency: "USD".into(), amount: 1.00 }],
    }),
    expires_at: Some(Utc::now() + Duration::seconds(60)),
    ..LeaseRequest::default()
};

let mut invoke = Envelope::new(MessageType::ToolInvoke(ToolInvokePayload {
    tool: "weekly-report@2.1.0".into(),
    arguments: serde_json::json!({ "week": "2026-W19" }),
    cost_budget: None,
    lease_request: Some(lease),
}));
invoke.session_id = Some(session_id.clone());
// Idempotency keys ride on the envelope (§6.4); set `invoke.idempotency_key`
// before sending if you need replay-safety.
transport.send(invoke).await?;

if let Some(env) = transport.recv().await? {
    if let MessageType::JobAccepted(accepted) = env.payload {
        println!("job_id = {}", accepted.job_id);
        println!("effective lease = {:?}", accepted.lease);
    }
}
```

### Consuming events

Iterate the ordered event stream — `log`, `thought`, `tool_call`, `tool_result`, `status`, `metric`, `artifact_ref`, `progress`, `result_chunk` — and optionally acknowledge progress so the runtime can release buffered events early.

```rust
use arcp::messages::{MessageType, SessionAckPayload};
use arcp::Envelope;

let mut last_seq: u64 = 0;
while let Some(env) = transport.recv().await? {
    if let Some(seq) = env.event_seq {
        last_seq = seq;
    }
    match env.payload {
        MessageType::Log(p) => println!("[log {:?}] {}", p.level, p.message),
        MessageType::Metric(m) => println!("metric[{}] = {} {}", m.name, m.value, m.unit),
        MessageType::JobProgress(p) => println!("progress {:?}%", p.percent),
        MessageType::JobResultChunk(c) => println!("chunk seq={} more={}", c.chunk_seq, c.more),
        MessageType::JobCompleted(_) | MessageType::JobFailed(_) | MessageType::JobCancelled(_) => break,
        _ => {}
    }

    // Coalesced flow-control ack so the runtime can free buffered events.
    if last_seq.is_multiple_of(32) {
        let mut ack = Envelope::new(MessageType::SessionAck(SessionAckPayload {
            last_processed_seq: last_seq,
        }));
        ack.session_id = Some(session_id.clone());
        transport.send(ack).await?;
    }
}
```

### Leases and budgets

Request capabilities, a budget, and an expiry; read budget-remaining metrics as they arrive; handle the runtime's enforcement decisions.

```rust
use chrono::{Duration, Utc};

use arcp::error::ErrorCode;
use arcp::messages::{
    CostBudget, CostBudgetAmount, LeaseRequest, MessageType, ToolInvokePayload,
};
use arcp::Envelope;

let lease = LeaseRequest {
    cost_budget: Some(CostBudget {
        amounts: vec![CostBudgetAmount { currency: "USD".into(), amount: 1.00 }],
    }),
    expires_at: Some(Utc::now() + Duration::seconds(600)),
    ..LeaseRequest::default()
};

let mut invoke = Envelope::new(MessageType::ToolInvoke(ToolInvokePayload {
    tool: "web-research".into(),
    arguments: serde_json::json!({ "iterations": 8, "perCallUSD": 0.30 }),
    cost_budget: None,
    lease_request: Some(lease),
}));
invoke.session_id = Some(session_id.clone());
transport.send(invoke).await?;

while let Some(env) = transport.recv().await? {
    match env.payload {
        MessageType::Metric(m) if m.name == "cost.budget.remaining" => {
            println!("budget remaining: {:.2} {}", m.value, m.unit);
        }
        MessageType::JobFailed(p) if p.code == ErrorCode::BudgetExhausted
            || p.code == ErrorCode::LeaseExpired =>
        {
            // BUDGET_EXHAUSTED and LEASE_EXPIRED are never retryable —
            // a naive retry fails identically.
            return Err(format!("{}: {}", p.code, p.message).into());
        }
        MessageType::JobCompleted(_) => break,
        _ => {}
    }
}
```

### Subscribing to jobs

Attach read-only to a job submitted elsewhere and observe its live stream (with optional history replay) without cancel authority.

```rust
use arcp::ids::JobId;
use arcp::messages::{JobSubscribePayload, JobUnsubscribePayload, MessageType};
use arcp::Envelope;

let job_id: JobId = /* discovered via session.list_jobs */;

let mut subscribe = Envelope::new(MessageType::JobSubscribe(JobSubscribePayload {
    job_id: job_id.clone(),
    from_event_seq: None,
    history: true, // replay buffered events before live tail
}));
subscribe.session_id = Some(session_id.clone());
transport.send(subscribe).await?;

while let Some(env) = transport.recv().await? {
    match env.payload {
        MessageType::JobSubscribed(ack) => {
            println!(
                "subscribed_from={} replayed={} status={}",
                ack.subscribed_from, ack.replayed, ack.current_status,
            );
        }
        MessageType::JobCompleted(_) | MessageType::JobFailed(_) | MessageType::JobCancelled(_) => break,
        _ => {}
    }
}

let mut unsubscribe = Envelope::new(MessageType::JobUnsubscribe(JobUnsubscribePayload {
    job_id,
}));
unsubscribe.session_id = Some(session_id.clone());
transport.send(unsubscribe).await?;
```

### Error handling

Catch the typed error taxonomy and respect the `retryable` flag — `LEASE_EXPIRED` and `BUDGET_EXHAUSTED` are never retryable; a naive retry fails identically.

```rust
use arcp::error::{ARCPError, ErrorCode};

match run_job(&transport, session_id.clone()).await {
    Ok(value) => println!("ok: {value}"),
    Err(err) => match err {
        ARCPError::LeaseExpired { .. }
        | ARCPError::LeaseRevoked { .. }
        | ARCPError::BudgetExhausted { .. } => {
            // Non-retryable: resubmit with a fresh lease / budget instead.
            return Err(err.into());
        }
        e if e.code().retryable() => {
            // Safe to retry with backoff: INTERNAL, UNAVAILABLE, ABORTED,
            // DEADLINE_EXCEEDED, RESOURCE_EXHAUSTED.
            backoff_and_retry(e).await?;
        }
        e => return Err(e.into()),
    },
}
```

## Feature support

`Capabilities` (RFC §7) is the negotiated feature set, exchanged on
`session.open` / `session.accepted`. The Rust SDK implements the following
capability fields:

| `Capabilities` field | Status |
|---|---|
| `streaming` | Supported |
| `durable_jobs` | Supported |
| `checkpoints` | Not implemented (deferred) |
| `binary_streams` | Not implemented (deferred) |
| `agent_handoff` | Supported |
| `model_use` | Supported |
| `provisioned_credentials` | Supported |
| `artifacts` | Supported |
| `subscriptions` | Supported |
| `scheduled_jobs` | Not implemented (deferred) |
| `interrupt` | Supported |
| `anonymous` | Supported |
| `heartbeat_recovery` | Supported (`"fail"` / `"block"`) |
| `binary_encoding` | Advertised; payloads remain JSON |
| `extensions` | Supported |
| `artifact_retention` | Supported |
| `agents` | Supported (v1.0 flat list and v1.1 rich form) |

The SDK also implements the ARCP v1.1 protocol-level surfaces that are not
themselves capability flags: `session.ack` flow control (§6.5),
`session.ping`/`pong` heartbeats (§6.4), `session.list_jobs` (§6.6),
`job.subscribe`/`job.unsubscribe` (§7.6), `job.progress`,
`job.result_chunk` (§8.4), and `agent@version` resolution (§7.5).

## Transport

ARCP is transport-agnostic. This SDK ships a WebSocket transport (default), a stdio transport for in-process child runtimes, and an in-memory transport for tests. WebSocket is the default for networked runtimes; stdio is used for in-process child runtimes. Select one by constructing the corresponding type (`WebSocketTransport::dial(url)`, `StdioTransport::process()`, or `arcp::transport::paired()` for the in-memory pair) and passing it to `ARCPClient::new(transport)`; WebSocket and stdio sit behind the `transport-ws` and `transport-stdio` Cargo features (both on by default), and the in-memory transport is always compiled in.

## API reference

Full API reference — every type, method, and event payload — is in [`docs/`](docs/) and at <https://docs.rs/arcp>.

## Versioning and compatibility

This SDK speaks **ARCP v1.1 (draft)**. The SDK follows semantic versioning independently of the protocol; the protocol version it negotiates is shown above and in `session.hello`. A runtime advertising a different ARCP MAJOR is not guaranteed compatible. Feature mismatches degrade gracefully: the effective feature set is the intersection of what the client and runtime advertise, and the SDK will not use a feature outside it.

## Contributing

See [`CONTRIBUTING.md`](CONTRIBUTING.md). Protocol questions and proposed changes belong in the [spec repository](https://github.com/agentruntimecontrolprotocol/spec); SDK bugs and feature requests belong here.

## License

Apache-2.0 — see [`LICENSE`](LICENSE).
