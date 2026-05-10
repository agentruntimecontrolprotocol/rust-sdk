//! Integration tests for the job lifecycle (RFC §10.1–§10.2).
//!
//! Phase 3 covers tool invocation that runs to completion, tool errors
//! that surface as `job.failed`, and routing for unknown tools.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_panics_doc
)]

mod common;

use std::sync::Arc;

use arcp::auth::BearerAuthenticator;
use arcp::error::ARCPError;
use arcp::messages::{Capabilities, ClientIdentity, Credentials};
use arcp::runtime::{ARCPRuntime, ToolHandler, ToolRegistryBuilder};
use arcp::transport::paired;
use arcp::ARCPClient;
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

struct EchoTool;

#[async_trait]
impl ToolHandler for EchoTool {
    fn name(&self) -> &'static str {
        "echo"
    }

    async fn invoke(
        &self,
        arguments: serde_json::Value,
        _cancel: CancellationToken,
    ) -> Result<serde_json::Value, ARCPError> {
        Ok(arguments)
    }
}

struct FailingTool;

#[async_trait]
impl ToolHandler for FailingTool {
    fn name(&self) -> &'static str {
        "fail"
    }

    async fn invoke(
        &self,
        _arguments: serde_json::Value,
        _cancel: CancellationToken,
    ) -> Result<serde_json::Value, ARCPError> {
        Err(ARCPError::InvalidArgument {
            detail: "deliberate failure".into(),
        })
    }
}

async fn spawn_with_tools() -> ARCPClient<arcp::transport::MemoryTransport> {
    let tools = ToolRegistryBuilder::new()
        .with(Arc::new(EchoTool))
        .with(Arc::new(FailingTool))
        .build();
    let runtime = ARCPRuntime::builder()
        .with_authenticator(Box::new(BearerAuthenticator::new().with_token("t", "p")))
        .with_capabilities(Capabilities {
            streaming: Some(true),
            durable_jobs: Some(true),
            ..Default::default()
        })
        .with_tools(tools)
        .build()
        .await
        .expect("build");
    let (server_t, client_t) = paired();
    let _h = runtime.serve_connection(server_t);
    ARCPClient::new(client_t)
}

fn ident() -> ClientIdentity {
    ClientIdentity {
        kind: "test".into(),
        version: "0".into(),
        fingerprint: None,
        principal: None,
    }
}

#[tokio::test]
async fn happy_path_invoke_returns_value() {
    let client = spawn_with_tools().await;
    let session = client
        .open()
        .expect("open")
        .authenticate(
            Credentials {
                scheme: arcp::messages::AuthScheme::Bearer,
                token: Some("t".into()),
            },
            ident(),
            Capabilities::default(),
        )
        .await
        .expect("auth");

    let job = session
        .invoke("echo", serde_json::json!({"hello": "world"}))
        .await
        .expect("invoke");
    assert!(job.job_id.as_str().starts_with("job_"));

    let result = job.join().await.expect("complete");
    assert_eq!(result, serde_json::json!({"hello": "world"}));
}

#[tokio::test]
async fn failing_tool_surfaces_as_job_failed() {
    let client = spawn_with_tools().await;
    let session = client
        .open()
        .expect("open")
        .authenticate(
            Credentials {
                scheme: arcp::messages::AuthScheme::Bearer,
                token: Some("t".into()),
            },
            ident(),
            Capabilities::default(),
        )
        .await
        .expect("auth");

    let job = session
        .invoke("fail", serde_json::json!({}))
        .await
        .expect("invoke");
    let err = job.join().await.expect_err("must fail");
    assert!(err.to_string().contains("deliberate failure"), "got: {err}");
}

#[tokio::test]
async fn unknown_tool_surfaces_as_not_found() {
    let client = spawn_with_tools().await;
    let session = client
        .open()
        .expect("open")
        .authenticate(
            Credentials {
                scheme: arcp::messages::AuthScheme::Bearer,
                token: Some("t".into()),
            },
            ident(),
            Capabilities::default(),
        )
        .await
        .expect("auth");

    let job = session
        .invoke("never-registered", serde_json::json!({}))
        .await
        .expect("invoke");
    let err = job.join().await.expect_err("must fail");
    assert!(
        err.to_string().contains("not registered")
            || err.to_string().contains("NOT_FOUND")
            || err.to_string().contains("not_found"),
        "got: {err}"
    );
}
