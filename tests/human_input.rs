//! Integration tests for human-in-the-loop primitives (RFC §12).

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_panics_doc
)]

mod common;

use std::sync::Arc;

use arcp::auth::BearerAuthenticator;
use arcp::client::HumanInputHandler;
use arcp::error::ARCPError;
use arcp::messages::{
    AuthScheme, Capabilities, ChoiceOption, ClientIdentity, Credentials, HumanChoiceRequestPayload,
    HumanInputRequestPayload,
};
use arcp::runtime::{ARCPRuntime, ToolContext, ToolHandler, ToolRegistryBuilder};
use arcp::transport::paired;
use arcp::ARCPClient;
use async_trait::async_trait;

/// A tool that asks the human "what's your name?" and returns it as the result.
struct GreeterTool;

#[async_trait]
impl ToolHandler for GreeterTool {
    fn name(&self) -> &'static str {
        "greeter"
    }

    async fn invoke(
        &self,
        _arguments: serde_json::Value,
        ctx: ToolContext,
    ) -> Result<serde_json::Value, ARCPError> {
        let value = ctx
            .request_human_input(HumanInputRequestPayload {
                prompt: "What's your name?".into(),
                response_schema: serde_json::json!({"type": "object", "properties": {"name": {"type": "string"}}}),
                default: None,
                expires_at: chrono::Utc::now() + chrono::Duration::seconds(30),
            })
            .await?;
        Ok(
            serde_json::json!({"greeting": format!("Hello, {}", value["name"].as_str().unwrap_or("?"))}),
        )
    }
}

/// A tool that asks "fix or skip?" via human.choice and returns the picked option.
struct ChooserTool;

#[async_trait]
impl ToolHandler for ChooserTool {
    fn name(&self) -> &'static str {
        "chooser"
    }

    async fn invoke(
        &self,
        _arguments: serde_json::Value,
        ctx: ToolContext,
    ) -> Result<serde_json::Value, ARCPError> {
        let choice = ctx
            .request_human_choice(HumanChoiceRequestPayload {
                prompt: "Fix or skip?".into(),
                options: vec![
                    ChoiceOption {
                        id: "fix".into(),
                        label: "Fix it".into(),
                    },
                    ChoiceOption {
                        id: "skip".into(),
                        label: "Skip".into(),
                    },
                ],
                expires_at: chrono::Utc::now() + chrono::Duration::seconds(30),
            })
            .await?;
        Ok(serde_json::json!({"chose": choice}))
    }
}

/// A handler that always returns the canned name "alice".
struct AliceHandler;

#[async_trait]
impl HumanInputHandler for AliceHandler {
    async fn input(&self, _req: HumanInputRequestPayload) -> serde_json::Value {
        serde_json::json!({"name": "alice"})
    }

    async fn choice(&self, req: HumanChoiceRequestPayload) -> String {
        // Pick the second option to verify routing.
        req.options.get(1).map(|o| o.id.clone()).unwrap_or_default()
    }
}

#[tokio::test]
async fn human_input_round_trip() {
    let runtime = ARCPRuntime::builder()
        .with_authenticator(Box::new(BearerAuthenticator::new().with_token("t", "p")))
        .with_capabilities(Capabilities {
            human_input: Some(true),
            ..Default::default()
        })
        .with_tools(
            ToolRegistryBuilder::new()
                .with(Arc::new(GreeterTool))
                .build(),
        )
        .build()
        .await
        .expect("build");
    let (server_t, client_t) = paired();
    let _h = runtime.serve_connection(server_t);
    let client = ARCPClient::new(client_t).with_human_input_handler(Arc::new(AliceHandler));

    let session = client
        .open()
        .expect("open")
        .authenticate(
            Credentials {
                scheme: AuthScheme::Bearer,
                token: Some("t".into()),
            },
            ClientIdentity {
                kind: "test".into(),
                version: "0".into(),
                fingerprint: None,
                principal: None,
            },
            Capabilities {
                human_input: Some(true),
                ..Default::default()
            },
        )
        .await
        .expect("auth");

    let job = session
        .invoke("greeter", serde_json::json!({}))
        .await
        .expect("invoke");
    let result = job.join().await.expect("complete");
    assert_eq!(result, serde_json::json!({"greeting": "Hello, alice"}));
}

#[tokio::test]
async fn human_choice_round_trip() {
    let runtime = ARCPRuntime::builder()
        .with_authenticator(Box::new(BearerAuthenticator::new().with_token("t", "p")))
        .with_capabilities(Capabilities {
            human_input: Some(true),
            ..Default::default()
        })
        .with_tools(
            ToolRegistryBuilder::new()
                .with(Arc::new(ChooserTool))
                .build(),
        )
        .build()
        .await
        .expect("build");
    let (server_t, client_t) = paired();
    let _h = runtime.serve_connection(server_t);
    let client = ARCPClient::new(client_t).with_human_input_handler(Arc::new(AliceHandler));

    let session = client
        .open()
        .expect("open")
        .authenticate(
            Credentials {
                scheme: AuthScheme::Bearer,
                token: Some("t".into()),
            },
            ClientIdentity {
                kind: "test".into(),
                version: "0".into(),
                fingerprint: None,
                principal: None,
            },
            Capabilities {
                human_input: Some(true),
                ..Default::default()
            },
        )
        .await
        .expect("auth");

    let job = session
        .invoke("chooser", serde_json::json!({}))
        .await
        .expect("invoke");
    let result = job.join().await.expect("complete");
    assert_eq!(result, serde_json::json!({"chose": "skip"}));
}
