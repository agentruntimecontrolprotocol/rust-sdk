//! Tool invocation: register a single `echo` tool, drive it from a client,
//! and print the result.

use std::sync::Arc;

use arcp::auth::BearerAuthenticator;
use arcp::error::ARCPError;
use arcp::messages::{AuthScheme, Capabilities, ClientIdentity, Credentials};
use arcp::runtime::{ARCPRuntime, ToolContext, ToolHandler, ToolRegistryBuilder};
use arcp::transport::paired;
use arcp::ARCPClient;
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
    ) -> Result<serde_json::Value, ARCPError> {
        Ok(serde_json::json!({"echoed": arguments}))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let runtime = ARCPRuntime::builder()
        .with_authenticator(Box::new(
            BearerAuthenticator::new().with_token("t", "alice"),
        ))
        .with_capabilities(Capabilities {
            durable_jobs: Some(true),
            ..Default::default()
        })
        .with_tools(ToolRegistryBuilder::new().with(Arc::new(EchoTool)).build())
        .build()
        .await?;

    let (server_t, client_t) = paired();
    let _runtime_task = runtime.serve_connection(server_t);

    let client = ARCPClient::new(client_t);
    let session = client
        .open()?
        .authenticate(
            Credentials {
                scheme: AuthScheme::Bearer,
                token: Some("t".into()),
            },
            ClientIdentity {
                kind: "example-02".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                fingerprint: None,
                principal: Some("alice@example.com".into()),
            },
            Capabilities::default(),
        )
        .await?;

    let job = session
        .invoke("echo", serde_json::json!({"hello": "world"}))
        .await?;
    println!("job_id: {}", job.job_id);
    println!("result: {:#?}", job.join().await?);
    Ok(())
}
