//! ARCP v1.1 §9.6 — `cost.budget` capability + `BUDGET_EXHAUSTED`.
//!
//! Hosts a `web-research` agent that charges $0.30 per iteration. The
//! client submits with a `cost.budget: ["USD:1.00"]` lease, so the
//! fourth iteration's pre-call charge fails with `BUDGET_EXHAUSTED`
//! and the runtime emits a terminal `job.failed`. Along the way the
//! runtime emits `cost.search` (the agent's cost report) and
//! `cost.budget.remaining` (the running counter) metric events.
//!
//! Run with:
//!     `cargo run --example cost_budget`

#![allow(clippy::similar_names, clippy::expect_used, clippy::print_stdout)]

use std::sync::Arc;
use std::time::Duration;

use arcp::auth::BearerAuthenticator;
use arcp::envelope::Envelope;
use arcp::error::ARCPError;
use arcp::messages::{
    AuthScheme, Capabilities, ClientIdentity, CostBudget, CostBudgetAmount, Credentials,
    MessageType, SessionOpenPayload, ToolInvokePayload,
};
use arcp::runtime::context::ToolContext;
use arcp::runtime::tools::{ToolHandler, ToolRegistryBuilder};
use arcp::runtime::ARCPRuntime;
use arcp::transport::{paired, Transport};
use async_trait::async_trait;

struct WebResearchTool;

#[async_trait]
impl ToolHandler for WebResearchTool {
    fn name(&self) -> &'static str {
        "web-research"
    }

    async fn invoke(
        &self,
        arguments: serde_json::Value,
        ctx: ToolContext,
    ) -> Result<serde_json::Value, ARCPError> {
        let iterations = arguments
            .get("iterations")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(8);
        let per = arguments
            .get("perCallUSD")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.3);
        for i in 1..=iterations {
            println!(
                "[agent] iteration {i}: charging {per:.2} USD (remaining={})",
                ctx.budget().remaining("USD").unwrap_or(f64::INFINITY)
            );
            ctx.charge("cost.search", per, "USD").await?;
        }
        Ok(serde_json::json!({"iterations": iterations}))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let runtime = ARCPRuntime::builder()
        .with_authenticator(Box::new(
            BearerAuthenticator::new().with_token("demo-token", "demo"),
        ))
        .with_tools(
            ToolRegistryBuilder::new()
                .with(Arc::new(WebResearchTool))
                .build(),
        )
        .build()
        .await?;

    let (server_t, client_t) = paired();
    let _h = runtime.serve_connection(server_t);

    let mut open = Envelope::new(MessageType::SessionOpen(SessionOpenPayload {
        auth: Credentials {
            scheme: AuthScheme::Bearer,
            token: Some("demo-token".into()),
        },
        client: ClientIdentity {
            kind: "cost-budget-demo".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            fingerprint: None,
            principal: None,
        },
        capabilities: Capabilities::default(),
    }));
    open.id = arcp::ids::MessageId::new();
    client_t.send(open).await?;
    let accepted = client_t.recv().await?.ok_or("no session.accepted")?;
    let MessageType::SessionAccepted(payload) = accepted.payload else {
        return Err("expected session.accepted".into());
    };
    let session_id = payload.session_id;
    println!("connected; session_id={session_id}");

    let mut invoke = Envelope::new(MessageType::ToolInvoke(ToolInvokePayload {
        tool: "web-research".into(),
        arguments: serde_json::json!({"iterations": 8, "perCallUSD": 0.3}),
        cost_budget: Some(CostBudget {
            amounts: vec![CostBudgetAmount {
                currency: "USD".into(),
                amount: 1.0,
            }],
        }),
        lease_request: None,
    }));
    invoke.session_id = Some(session_id);
    client_t.send(invoke).await?;

    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        let env = tokio::time::timeout(Duration::from_millis(500), client_t.recv())
            .await??
            .ok_or("transport closed")?;
        match env.payload {
            MessageType::JobAccepted(p) => println!("job_id={}", p.job_id),
            MessageType::Metric(m) => {
                println!("metric[{}]={:.2} {}", m.name, m.value, m.unit);
            }
            MessageType::JobFailed(p) => {
                println!(
                    "job.failed code={} retryable={:?} message={:?}",
                    p.code, p.retryable, p.message
                );
                break;
            }
            MessageType::JobCompleted(p) => {
                println!("job.completed value={:?}", p.value);
                break;
            }
            _ => {}
        }
    }

    Ok(())
}
