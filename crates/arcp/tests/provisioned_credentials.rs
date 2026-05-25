//! Integration tests for provisioned credentials (ARCP v1.1 §9.8).

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_panics_doc
)]

use std::sync::Arc;
use std::time::Duration;

use arcp::auth::BearerAuthenticator;
use arcp::envelope::Envelope;
use arcp::messages::{
    AuthScheme, CancelPayload, CancelTargetKind, Capabilities, ClientIdentity, CostBudget,
    CostBudgetAmount, Credentials, JobAcceptedPayload, LeaseRequest, MessageType, ModelUse,
    SessionOpenPayload, SubscribePayload, SubscriptionFilter, ToolInvokePayload,
};
use arcp::runtime::context::ToolContext;
use arcp::runtime::credentials::InMemoryCredentialProvisioner;
use arcp::runtime::tools::{ToolHandler, ToolRegistryBuilder};
use arcp::runtime::ARCPRuntime;
use arcp::transport::{paired, MemoryTransport, Transport};
use async_trait::async_trait;

struct EchoTool;

#[async_trait]
impl ToolHandler for EchoTool {
    fn name(&self) -> &'static str {
        "echo"
    }

    async fn invoke(
        &self,
        arguments: serde_json::Value,
        _ctx: ToolContext,
    ) -> Result<serde_json::Value, arcp::error::ARCPError> {
        Ok(arguments)
    }
}

struct SlowTool;

#[async_trait]
impl ToolHandler for SlowTool {
    fn name(&self) -> &'static str {
        "slow"
    }

    async fn invoke(
        &self,
        _arguments: serde_json::Value,
        ctx: ToolContext,
    ) -> Result<serde_json::Value, arcp::error::ARCPError> {
        ctx.cancel.cancelled().await;
        Err(arcp::error::ARCPError::Cancelled {
            reason: "cancelled".into(),
        })
    }
}

fn lease() -> LeaseRequest {
    LeaseRequest {
        cost_budget: Some(CostBudget {
            amounts: vec![CostBudgetAmount {
                currency: "USD".into(),
                amount: 1.0,
            }],
        }),
        model_use: Some(ModelUse {
            patterns: vec!["tier-fast/*".into()],
        }),
        expires_at: None,
        extra: std::collections::BTreeMap::default(),
    }
}

async fn runtime_with_provisioner() -> (ARCPRuntime, Arc<InMemoryCredentialProvisioner>) {
    let provisioner = Arc::new(InMemoryCredentialProvisioner::default());
    let runtime = ARCPRuntime::builder()
        .with_authenticator(Box::new(BearerAuthenticator::new().with_token("t", "p")))
        .with_credential_provisioner(provisioner.clone())
        .with_tools(
            ToolRegistryBuilder::new()
                .with(Arc::new(EchoTool))
                .with(Arc::new(SlowTool))
                .build(),
        )
        .build()
        .await
        .expect("build");
    (runtime, provisioner)
}

async fn open(runtime: &ARCPRuntime) -> MemoryTransport {
    let (server_t, client_t) = paired();
    let _h = runtime.serve_connection(server_t);
    let mut open = Envelope::new(MessageType::SessionOpen(SessionOpenPayload {
        auth: Credentials {
            scheme: AuthScheme::Bearer,
            token: Some("t".into()),
        },
        client: ClientIdentity {
            kind: "credentials-test".into(),
            version: "0".into(),
            fingerprint: None,
            principal: None,
        },
        capabilities: Capabilities {
            model_use: Some(true),
            provisioned_credentials: Some(true),
            subscriptions: Some(true),
            ..Capabilities::default()
        },
    }));
    open.id = arcp::ids::MessageId::new();
    client_t.send(open).await.expect("send open");
    let accepted = client_t.recv().await.expect("recv").expect("accepted");
    assert!(matches!(accepted.payload, MessageType::SessionAccepted(_)));
    client_t
}

async fn invoke(client: &MemoryTransport, tool: &str) -> JobAcceptedPayload {
    let invoke = Envelope::new(MessageType::ToolInvoke(ToolInvokePayload {
        tool: tool.into(),
        arguments: serde_json::json!({"ok": true}),
        cost_budget: None,
        lease_request: Some(lease()),
    }));
    client.send(invoke).await.expect("send invoke");
    loop {
        let env = tokio::time::timeout(Duration::from_secs(2), client.recv())
            .await
            .expect("timely")
            .expect("recv")
            .expect("env");
        if let MessageType::JobAccepted(payload) = env.payload {
            return payload;
        }
    }
}

#[tokio::test]
async fn credentials_are_attached_and_revoked_on_success() {
    let (runtime, provisioner) = runtime_with_provisioner().await;
    let client = open(&runtime).await;
    let accepted = invoke(&client, "echo").await;
    assert_eq!(accepted.credentials.len(), 1);
    assert_eq!(accepted.credentials[0].value, "test-token-1");
    assert_eq!(accepted.lease, Some(lease()));

    let mut completed = false;
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while std::time::Instant::now() < deadline {
        let env = tokio::time::timeout(Duration::from_millis(500), client.recv())
            .await
            .expect("timely")
            .expect("recv")
            .expect("env");
        if matches!(env.payload, MessageType::JobCompleted(_)) {
            completed = true;
            break;
        }
    }
    assert!(completed);
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(
        provisioner.revoked_ids(),
        vec![accepted.credentials[0].id.clone()]
    );
}

#[tokio::test]
async fn credentials_are_revoked_on_cancel() {
    let (runtime, provisioner) = runtime_with_provisioner().await;
    let client = open(&runtime).await;
    let accepted = invoke(&client, "slow").await;

    let cancel = Envelope::new(MessageType::Cancel(CancelPayload {
        target: CancelTargetKind::Job,
        target_id: accepted.job_id.to_string(),
        reason: Some("test".into()),
        deadline_ms: None,
    }));
    client.send(cancel).await.expect("send cancel");

    let mut cancelled = false;
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while std::time::Instant::now() < deadline {
        let env = tokio::time::timeout(Duration::from_millis(500), client.recv())
            .await
            .expect("timely")
            .expect("recv")
            .expect("env");
        if matches!(env.payload, MessageType::JobCancelled(_)) {
            cancelled = true;
            break;
        }
    }
    assert!(cancelled);
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(
        provisioner.revoked_ids(),
        vec![accepted.credentials[0].id.clone()]
    );
}

#[tokio::test]
async fn subscriber_fanout_redacts_credential_values() {
    let (runtime, _provisioner) = runtime_with_provisioner().await;
    let subscriber = open(&runtime).await;
    let commander = open(&runtime).await;

    subscriber
        .send(Envelope::new(MessageType::Subscribe(SubscribePayload {
            filter: SubscriptionFilter::default(),
            since: None,
        })))
        .await
        .expect("subscribe");
    let _ack = subscriber.recv().await.expect("recv").expect("ack");

    let accepted = invoke(&commander, "echo").await;
    assert_eq!(accepted.credentials.len(), 1);

    let mut saw_redacted = false;
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while std::time::Instant::now() < deadline {
        let env = tokio::time::timeout(Duration::from_millis(500), subscriber.recv())
            .await
            .expect("timely")
            .expect("recv")
            .expect("env");
        if let MessageType::SubscribeEvent(payload) = env.payload {
            let inner: Envelope = serde_json::from_value(payload.event).expect("inner env");
            if let MessageType::JobAccepted(job) = inner.payload {
                saw_redacted = true;
                assert!(job.credentials.is_empty());
                let json = serde_json::to_value(job).expect("json");
                assert!(json.get("credentials").is_none());
                assert!(!json.to_string().contains("test-token"));
                break;
            }
        }
    }
    assert!(saw_redacted);
}
