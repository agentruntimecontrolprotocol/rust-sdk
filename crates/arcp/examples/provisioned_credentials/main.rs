//! Provisioned credential example (ARCP v1.1 §9.8).

use std::sync::Arc;
use std::time::Duration;

use arcp::auth::BearerAuthenticator;
use arcp::envelope::Envelope;
use arcp::error::ARCPError;
use arcp::messages::{
    AuthScheme, Capabilities, ClientIdentity, CostBudget, CostBudgetAmount, Credentials,
    LeaseRequest, MessageType, ModelUse, SessionOpenPayload, ToolInvokePayload,
};
use arcp::runtime::context::ToolContext;
use arcp::runtime::credentials::{
    CredentialId, CredentialJobContext, CredentialProvisioner, CredentialScheme,
    ProvisionedCredential,
};
use arcp::runtime::tools::{ToolHandler, ToolRegistryBuilder};
use arcp::runtime::ARCPRuntime;
use arcp::transport::{paired, Transport};
use async_trait::async_trait;

struct StubLiteLlmProvisioner;

#[async_trait]
impl CredentialProvisioner for StubLiteLlmProvisioner {
    async fn issue(
        &self,
        lease: &LeaseRequest,
        _ctx: &CredentialJobContext,
    ) -> Result<Vec<ProvisionedCredential>, ARCPError> {
        let budget = lease.cost_budget.as_ref().map_or_else(
            || "none".into(),
            |b| {
                b.amounts
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(",")
            },
        );
        let models = lease
            .model_use
            .as_ref()
            .map_or_else(|| "none".into(), |m| m.patterns.join(","));
        println!("[provisioner] POST /key/generate budget={budget} models={models}");
        Ok(vec![ProvisionedCredential {
            id: CredentialId("cred_0000000000000001".into()),
            scheme: CredentialScheme::Bearer,
            value: "example-secret-do-not-print".into(),
            endpoint: "https://litellm.example.invalid".into(),
            profile: Some("tier-fast".into()),
            constraints: Some(lease.clone()),
        }])
    }

    async fn revoke(&self, id: &CredentialId) -> Result<(), ARCPError> {
        println!("[provisioner] POST /key/delete id={id}");
        Ok(())
    }
}

struct LlmTool;

#[async_trait]
impl ToolHandler for LlmTool {
    fn name(&self) -> &'static str {
        "llm.generate"
    }

    async fn invoke(
        &self,
        _arguments: serde_json::Value,
        ctx: ToolContext,
    ) -> Result<serde_json::Value, ARCPError> {
        ctx.enforce_model_use("tier-fast/small")?;
        Ok(serde_json::json!({"text": "hello from a leased model"}))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let runtime = ARCPRuntime::builder()
        .with_authenticator(Box::new(BearerAuthenticator::new().with_token("t", "p")))
        .with_credential_provisioner(Arc::new(StubLiteLlmProvisioner))
        .with_tools(ToolRegistryBuilder::new().with(Arc::new(LlmTool)).build())
        .build()
        .await?;
    let (server_t, client_t) = paired();
    let _server = runtime.serve_connection(server_t);

    let open = Envelope::new(MessageType::SessionOpen(SessionOpenPayload {
        auth: Credentials {
            scheme: AuthScheme::Bearer,
            token: Some("t".into()),
        },
        client: ClientIdentity {
            kind: "provisioned-credentials-example".into(),
            version: "0".into(),
            fingerprint: None,
            principal: None,
        },
        capabilities: Capabilities {
            model_use: Some(true),
            provisioned_credentials: Some(true),
            ..Capabilities::default()
        },
    }));
    client_t.send(open).await?;
    let accepted = client_t.recv().await?.ok_or("missing session.accepted")?;
    if !matches!(accepted.payload, MessageType::SessionAccepted(_)) {
        return Err("expected session.accepted".into());
    }

    let invoke = Envelope::new(MessageType::ToolInvoke(ToolInvokePayload {
        tool: "llm.generate".into(),
        arguments: serde_json::json!({ "prompt": "hello" }),
        cost_budget: None,
        lease_request: Some(LeaseRequest {
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
        }),
    }));
    client_t.send(invoke).await?;

    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        let Some(env) = tokio::time::timeout(Duration::from_millis(500), client_t.recv()).await??
        else {
            break;
        };
        match env.payload {
            MessageType::JobAccepted(payload) => {
                for credential in payload.credentials {
                    println!(
                        "[client] job.accepted credential id={} scheme={:?}",
                        credential.id, credential.scheme
                    );
                }
            }
            MessageType::JobCompleted(payload) => {
                println!(
                    "[client] job.completed value={}",
                    payload.value.unwrap_or(serde_json::Value::Null)
                );
                break;
            }
            MessageType::JobFailed(payload) => {
                return Err(format!("job failed: {} {}", payload.code, payload.message).into());
            }
            _ => {}
        }
    }
    tokio::time::sleep(Duration::from_millis(50)).await;
    Ok(())
}
