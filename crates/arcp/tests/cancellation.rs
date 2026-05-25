//! Integration tests for cooperative cancellation (RFC §10.4).

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_panics_doc
)]

mod common;

use std::sync::Arc;
use std::time::Duration;

use arcp::auth::BearerAuthenticator;
use arcp::error::{ARCPError, ErrorCode};
use arcp::messages::{Capabilities, ClientIdentity, Credentials};
use arcp::runtime::{ARCPRuntime, ToolContext, ToolHandler, ToolRegistryBuilder};
use arcp::transport::paired;
use arcp::ARCPClient;
use async_trait::async_trait;

/// Tool that loops until cancelled, then returns Ok with a sentinel.
struct SleeperTool;

#[async_trait]
impl ToolHandler for SleeperTool {
    fn name(&self) -> &'static str {
        "sleep"
    }

    async fn invoke(
        &self,
        _arguments: serde_json::Value,
        ctx: ToolContext,
    ) -> Result<serde_json::Value, ARCPError> {
        loop {
            tokio::select! {
                () = ctx.cancel.cancelled() => {
                    return Err(ARCPError::Cancelled { reason: "cooperative".into() });
                }
                () = tokio::time::sleep(Duration::from_secs(60)) => {}
            }
        }
    }
}

#[tokio::test]
async fn cancel_before_terminal_yields_cancelled_outcome() {
    let runtime = ARCPRuntime::builder()
        .with_authenticator(Box::new(BearerAuthenticator::new().with_token("t", "p")))
        .with_capabilities(Capabilities {
            durable_jobs: Some(true),
            ..Default::default()
        })
        .with_tools(
            ToolRegistryBuilder::new()
                .with(Arc::new(SleeperTool))
                .build(),
        )
        .build()
        .await
        .expect("build");
    let (server_t, client_t) = paired();
    let _h = runtime.serve_connection(server_t);
    let client = ARCPClient::new(client_t);

    let session = client
        .open()
        .expect("open")
        .authenticate(
            Credentials {
                scheme: arcp::messages::AuthScheme::Bearer,
                token: Some("t".into()),
            },
            ClientIdentity {
                kind: "test".into(),
                version: "0".into(),
                fingerprint: None,
                principal: None,
            },
            Capabilities::default(),
        )
        .await
        .expect("auth");

    let job = session
        .invoke("sleep", serde_json::json!({}))
        .await
        .expect("invoke");

    // Issue cancel after a brief moment so job.started has had time to fire.
    tokio::time::sleep(Duration::from_millis(20)).await;
    job.cancel("user requested").await.expect("cancel");

    let outcome = job.join().await;
    let err = outcome.expect_err("cancelled job must surface as Err");
    assert_eq!(err.code(), ErrorCode::Cancelled, "got: {err}");
}
