//! Integration tests for `model.use` lease enforcement.

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
use arcp::error::{ARCPError, ErrorCode};
use arcp::messages::{
    AuthScheme, Capabilities, ClientIdentity, Credentials, LeaseRequest, MessageType, ModelUse,
    SessionOpenPayload, ToolInvokePayload,
};
use arcp::runtime::context::ToolContext;
use arcp::runtime::tools::{ToolHandler, ToolRegistryBuilder};
use arcp::runtime::ARCPRuntime;
use arcp::transport::{paired, MemoryTransport, Transport};
use async_trait::async_trait;

struct ModelTool;

#[async_trait]
impl ToolHandler for ModelTool {
    fn name(&self) -> &'static str {
        "model-tool"
    }

    async fn invoke(
        &self,
        arguments: serde_json::Value,
        ctx: ToolContext,
    ) -> Result<serde_json::Value, ARCPError> {
        let model = arguments
            .get("model")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("anthropic/claude-3-haiku-20240307");
        ctx.enforce_model_use(model)?;
        Ok(serde_json::json!({"model": model}))
    }
}

async fn submit(model: &str) -> MemoryTransport {
    let runtime = ARCPRuntime::builder()
        .with_authenticator(Box::new(BearerAuthenticator::new().with_token("t", "p")))
        .with_tools(ToolRegistryBuilder::new().with(Arc::new(ModelTool)).build())
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
            kind: "model-use-test".into(),
            version: "0".into(),
            fingerprint: None,
            principal: None,
        },
        capabilities: Capabilities::default(),
    }));
    open.id = arcp::ids::MessageId::new();
    client_t.send(open).await.expect("send open");
    let accepted = client_t.recv().await.expect("recv").expect("accepted");
    assert!(matches!(accepted.payload, MessageType::SessionAccepted(_)));

    let mut invoke = Envelope::new(MessageType::ToolInvoke(ToolInvokePayload {
        tool: "model-tool".into(),
        arguments: serde_json::json!({ "model": model }),
        cost_budget: None,
        lease_request: Some(LeaseRequest {
            model_use: Some(ModelUse {
                patterns: vec!["anthropic/claude-3-haiku-*".into()],
            }),
            ..LeaseRequest::default()
        }),
    }));
    invoke.session_id = accepted.session_id;
    client_t.send(invoke).await.expect("send invoke");
    client_t
}

#[tokio::test]
async fn matching_model_completes() {
    let client = submit("anthropic/claude-3-haiku-20240307").await;
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
}

#[tokio::test]
async fn mismatching_model_fails_permission_denied() {
    let client = submit("anthropic/claude-3-opus-20240229").await;
    let mut failed = None;
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while std::time::Instant::now() < deadline {
        let env = tokio::time::timeout(Duration::from_millis(500), client.recv())
            .await
            .expect("timely")
            .expect("recv")
            .expect("env");
        if let MessageType::JobFailed(payload) = env.payload {
            failed = Some(payload);
            break;
        }
    }
    let failed = failed.expect("job.failed");
    assert_eq!(failed.code, ErrorCode::PermissionDenied);
}
