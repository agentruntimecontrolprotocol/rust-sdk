//! Integration tests for `cost.budget` capability + `BUDGET_EXHAUSTED`
//! enforcement (ARCP v1.1 §9.6).

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_panics_doc,
    clippy::similar_names
)]

use std::sync::Arc;
use std::time::Duration;

use arcp::auth::BearerAuthenticator;
use arcp::envelope::Envelope;
use arcp::error::{ARCPError, ErrorCode};
use arcp::messages::{
    AuthScheme, Capabilities, ClientIdentity, CostBudget, CostBudgetAmount, Credentials,
    MessageType, SessionOpenPayload, ToolInvokePayload,
};
use arcp::runtime::context::ToolContext;
use arcp::runtime::tools::{ToolHandler, ToolRegistryBuilder};
use arcp::runtime::ARCPRuntime;
use arcp::transport::{paired, Transport};
use async_trait::async_trait;

struct ChargingTool;

#[async_trait]
impl ToolHandler for ChargingTool {
    fn name(&self) -> &'static str {
        "charger"
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
        let mut spent = 0.0f64;
        for _ in 0..iterations {
            ctx.charge("cost.search", per, "USD").await?;
            spent += per;
        }
        Ok(serde_json::json!({"spent": spent}))
    }
}

async fn submit(
    tool: &'static str,
    budget: Option<CostBudget>,
    args: serde_json::Value,
) -> (
    arcp::transport::MemoryTransport,
    arcp::ids::SessionId,
    arcp::ids::JobId,
) {
    let runtime = ARCPRuntime::builder()
        .with_authenticator(Box::new(BearerAuthenticator::new().with_token("t", "p")))
        .with_tools(
            ToolRegistryBuilder::new()
                .with(Arc::new(ChargingTool))
                .build(),
        )
        .build()
        .await
        .expect("build");
    let (server_t, client_t) = paired();
    let _h = runtime.serve_connection(server_t);

    let mut open = Envelope::new(MessageType::SessionOpen(SessionOpenPayload {
        auth: Credentials {
            scheme: AuthScheme::Bearer,
            token: Some("t".into()),
        },
        client: ClientIdentity {
            kind: "cb-test".into(),
            version: "0".into(),
            fingerprint: None,
            principal: None,
        },
        capabilities: Capabilities::default(),
    }));
    open.id = arcp::ids::MessageId::new();
    client_t.send(open).await.expect("send");
    let accepted = client_t.recv().await.expect("recv").expect("envelope");
    let MessageType::SessionAccepted(payload) = accepted.payload else {
        panic!("expected session.accepted");
    };
    let session_id = payload.session_id;

    let mut invoke = Envelope::new(MessageType::ToolInvoke(ToolInvokePayload {
        tool: tool.into(),
        arguments: args,
        cost_budget: budget,
    }));
    invoke.session_id = Some(session_id.clone());
    client_t.send(invoke).await.expect("send invoke");
    let accepted = client_t.recv().await.expect("recv").expect("envelope");
    let MessageType::JobAccepted(p) = accepted.payload else {
        panic!("expected job.accepted");
    };
    (client_t, session_id, p.job_id)
}

#[tokio::test]
async fn budget_exhausted_surfaces_as_job_failed() {
    let budget = CostBudget {
        amounts: vec![CostBudgetAmount {
            currency: "USD".into(),
            amount: 1.0,
        }],
    };
    // 8 iterations × $0.30 = $2.40, well over the $1.00 budget.
    let (client, _sess, _job) = submit(
        "charger",
        Some(budget),
        serde_json::json!({"iterations": 8, "perCallUSD": 0.3}),
    )
    .await;

    // Drain envelopes until the terminal `job.failed` arrives.
    let mut got_failed: Option<arcp::messages::JobFailedPayload> = None;
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while std::time::Instant::now() < deadline {
        let env = tokio::time::timeout(Duration::from_millis(500), client.recv())
            .await
            .expect("timely")
            .expect("recv")
            .expect("envelope");
        if let MessageType::JobFailed(p) = env.payload {
            got_failed = Some(p);
            break;
        }
    }
    let failed = got_failed.expect("expected job.failed");
    assert_eq!(failed.code, ErrorCode::BudgetExhausted);
    assert_eq!(failed.retryable, Some(false));
}

#[tokio::test]
async fn unbudgeted_invoke_runs_to_completion() {
    let (client, _sess, _job) = submit(
        "charger",
        None,
        serde_json::json!({"iterations": 2, "perCallUSD": 0.05}),
    )
    .await;

    let mut got_completed = false;
    let deadline = std::time::Instant::now() + Duration::from_secs(1);
    while std::time::Instant::now() < deadline {
        let env = tokio::time::timeout(Duration::from_millis(500), client.recv())
            .await
            .expect("timely")
            .expect("recv")
            .expect("envelope");
        if matches!(env.payload, MessageType::JobCompleted(_)) {
            got_completed = true;
            break;
        }
    }
    assert!(got_completed, "unbudgeted job should complete");
}

#[tokio::test]
async fn budget_emits_remaining_metrics() {
    let budget = CostBudget {
        amounts: vec![CostBudgetAmount {
            currency: "USD".into(),
            amount: 1.0,
        }],
    };
    let (client, _sess, _job) = submit(
        "charger",
        Some(budget),
        serde_json::json!({"iterations": 2, "perCallUSD": 0.3}),
    )
    .await;

    let mut saw_cost = false;
    let mut saw_remaining = false;
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while std::time::Instant::now() < deadline {
        let env = tokio::time::timeout(Duration::from_millis(500), client.recv())
            .await
            .expect("timely")
            .expect("recv")
            .expect("envelope");
        if let MessageType::Metric(m) = env.payload {
            if m.name == "cost.search" && m.unit == "USD" {
                saw_cost = true;
            }
            if m.name == "cost.budget.remaining" && m.unit == "USD" {
                saw_remaining = true;
            }
        }
        if saw_cost && saw_remaining {
            break;
        }
    }
    assert!(saw_cost, "expected cost.search metric");
    assert!(saw_remaining, "expected cost.budget.remaining metric");
}

#[test]
fn cost_budget_subset_enforces_parent_envelope() {
    let parent = CostBudget {
        amounts: vec![CostBudgetAmount {
            currency: "USD".into(),
            amount: 5.0,
        }],
    };
    let child_ok = CostBudget {
        amounts: vec![CostBudgetAmount {
            currency: "USD".into(),
            amount: 2.0,
        }],
    };
    let child_too_big = CostBudget {
        amounts: vec![CostBudgetAmount {
            currency: "USD".into(),
            amount: 6.0,
        }],
    };
    let mut remaining = std::collections::HashMap::new();
    remaining.insert("USD".into(), 3.0);

    assert!(parent.subset_violation(&child_ok, &remaining).is_none());
    assert_eq!(
        parent.subset_violation(&child_too_big, &remaining),
        Some("USD")
    );
}
