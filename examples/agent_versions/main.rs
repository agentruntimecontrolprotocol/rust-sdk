//! ARCP v1.1 §7.5 — agent versioning (`name@version`) demo.
//!
//! Configures a runtime that advertises `echo` at versions
//! `1.0.0` and `2.0.0` (with `2.0.0` as default), then:
//!   1. Submits a job pinning the existing version (`echo@1.0.0`)
//!      and verifies it runs to completion.
//!   2. Submits a job pinning a missing version (`echo@9.9.9`) and
//!      verifies the runtime surfaces `AGENT_VERSION_NOT_AVAILABLE`.
//!
//! Run with:
//!     `cargo run --example agent_versions`

#![allow(clippy::similar_names, clippy::expect_used)]

use std::sync::Arc;
use std::time::Duration;

use arcp::auth::BearerAuthenticator;
use arcp::envelope::Envelope;
use arcp::error::{ARCPError, ErrorCode};
use arcp::messages::{
    AgentInventory, AgentInventoryEntry, AuthScheme, Capabilities, ClientIdentity, Credentials,
    MessageType, SessionOpenPayload, ToolInvokePayload,
};
use arcp::runtime::context::ToolContext;
use arcp::runtime::tools::{ToolHandler, ToolRegistryBuilder};
use arcp::runtime::ARCPRuntime;
use arcp::transport::{paired, Transport};
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
        Ok(arguments)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Runtime advertises echo at v1.0.0 and v2.0.0, default v2.0.0.
    let caps = Capabilities {
        agents: Some(AgentInventory::Rich(vec![AgentInventoryEntry {
            name: "echo".into(),
            versions: vec!["1.0.0".into(), "2.0.0".into()],
            default: Some("2.0.0".into()),
        }])),
        ..Capabilities::default()
    };

    let tools = ToolRegistryBuilder::new().with(Arc::new(EchoTool)).build();
    let runtime = ARCPRuntime::builder()
        .with_authenticator(Box::new(BearerAuthenticator::new().with_token("t", "p")))
        .with_tools(tools)
        .with_capabilities(caps)
        .build()
        .await?;
    let (server_t, client_t) = paired();
    let _h = runtime.serve_connection(server_t);

    let mut open = Envelope::new(MessageType::SessionOpen(SessionOpenPayload {
        auth: Credentials {
            scheme: AuthScheme::Bearer,
            token: Some("t".into()),
        },
        client: ClientIdentity {
            kind: "agent-versions-demo".into(),
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
    let session_id = payload.session_id.clone();
    println!(
        "runtime advertised agents: {:?}",
        payload.capabilities.agents
    );

    // 1. Pin to an existing version → should complete.
    {
        let mut invoke = Envelope::new(MessageType::ToolInvoke(ToolInvokePayload::new(
            "echo@1.0.0",
            serde_json::json!({"msg": "hello"}),
        )));
        invoke.session_id = Some(session_id.clone());
        client_t.send(invoke).await?;
        loop {
            let env = tokio::time::timeout(Duration::from_secs(1), client_t.recv())
                .await??
                .ok_or("transport closed")?;
            match env.payload {
                MessageType::JobCompleted(p) => {
                    println!("echo@1.0.0 → JobCompleted, value={:?}", p.value);
                    break;
                }
                MessageType::JobFailed(p) => {
                    return Err(format!("unexpected failure: {} {}", p.code, p.message).into());
                }
                _ => {}
            }
        }
    }

    // 2. Pin to a missing version → should surface AGENT_VERSION_NOT_AVAILABLE.
    {
        let mut invoke = Envelope::new(MessageType::ToolInvoke(ToolInvokePayload::new(
            "echo@9.9.9",
            serde_json::json!({"msg": "hello"}),
        )));
        invoke.session_id = Some(session_id);
        client_t.send(invoke).await?;
        loop {
            let env = tokio::time::timeout(Duration::from_secs(1), client_t.recv())
                .await??
                .ok_or("transport closed")?;
            match env.payload {
                MessageType::JobFailed(p) => {
                    assert_eq!(p.code, ErrorCode::AgentVersionNotAvailable);
                    println!("echo@9.9.9 → {} ({})", p.code, p.message);
                    break;
                }
                MessageType::JobCompleted(_) => {
                    return Err("expected failure for missing version".into());
                }
                _ => {}
            }
        }
    }

    println!("done");
    Ok(())
}
