//! Integration tests for `subscribe` / `subscribe.event` /
//! `unsubscribe` dispatch through the runtime (RFC §13).

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_panics_doc
)]

use std::sync::Arc;
use std::time::Duration;

use arcp::auth::BearerAuthenticator;
use arcp::error::ARCPError;
use arcp::messages::{
    AuthScheme, Capabilities, ClientIdentity, Credentials, MessageType, SubscriptionFilter,
};
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
        Ok(arguments)
    }
}

#[tokio::test]
async fn subscriber_receives_subsequent_invocation_events() {
    let runtime = ARCPRuntime::builder()
        .with_authenticator(Box::new(BearerAuthenticator::new().with_token("t", "p")))
        .with_capabilities(Capabilities {
            subscriptions: Some(true),
            ..Default::default()
        })
        .with_tools(ToolRegistryBuilder::new().with(Arc::new(EchoTool)).build())
        .build()
        .await
        .expect("build");

    // Two paired transports, two clients sharing the same runtime — one
    // subscribes, one invokes.
    let (sub_server_t, sub_client_t) = paired();
    let (cmd_server_t, cmd_client_t) = paired();
    let _h1 = runtime.serve_connection(sub_server_t);
    let _h2 = runtime.serve_connection(cmd_server_t);

    let auth_caps = Capabilities {
        subscriptions: Some(true),
        ..Default::default()
    };

    let subscriber = ARCPClient::new(sub_client_t)
        .open()
        .expect("open sub")
        .authenticate(
            Credentials {
                scheme: AuthScheme::Bearer,
                token: Some("t".into()),
            },
            ClientIdentity {
                kind: "subscriber".into(),
                version: "0".into(),
                fingerprint: None,
                principal: None,
            },
            auth_caps.clone(),
        )
        .await
        .expect("subscriber auth");

    let invoker = ARCPClient::new(cmd_client_t)
        .open()
        .expect("open cmd")
        .authenticate(
            Credentials {
                scheme: AuthScheme::Bearer,
                token: Some("t".into()),
            },
            ClientIdentity {
                kind: "invoker".into(),
                version: "0".into(),
                fingerprint: None,
                principal: None,
            },
            auth_caps,
        )
        .await
        .expect("invoker auth");

    // Subscribe to job.completed events only.
    let filter = SubscriptionFilter {
        types: vec!["job.completed".into()],
        ..SubscriptionFilter::default()
    };
    let sub = subscriber.subscribe(filter).await.expect("subscribe");
    assert!(sub.subscription_id.as_str().starts_with("sub_"));

    // Trigger work on the invoker side.
    let job = invoker
        .invoke("echo", serde_json::json!({"hello": "world"}))
        .await
        .expect("invoke");
    let _ = job.join().await.expect("complete");

    // The subscriber should see job.completed for the invoker's job.
    let event = tokio::time::timeout(Duration::from_millis(500), sub.next())
        .await
        .expect("timely")
        .expect("envelope");
    assert!(matches!(event.payload, MessageType::JobCompleted(_)));
}

#[tokio::test]
async fn subscription_handle_drop_silently_detaches() {
    let runtime = ARCPRuntime::builder()
        .with_authenticator(Box::new(BearerAuthenticator::new().with_token("t", "p")))
        .with_capabilities(Capabilities {
            subscriptions: Some(true),
            ..Default::default()
        })
        .build()
        .await
        .expect("build");
    let (server_t, client_t) = paired();
    let _h = runtime.serve_connection(server_t);

    let session = ARCPClient::new(client_t)
        .open()
        .expect("open")
        .authenticate(
            Credentials {
                scheme: AuthScheme::Bearer,
                token: Some("t".into()),
            },
            ClientIdentity {
                kind: "drop-test".into(),
                version: "0".into(),
                fingerprint: None,
                principal: None,
            },
            Capabilities {
                subscriptions: Some(true),
                ..Default::default()
            },
        )
        .await
        .expect("auth");

    let sub = session
        .subscribe(SubscriptionFilter::default())
        .await
        .expect("subscribe");
    let _id = sub.subscription_id.clone();
    drop(sub);
    // The runtime's broadcast keeps publishing; we just verify the
    // session is still healthy by issuing another envelope.
    let sub2 = session
        .subscribe(SubscriptionFilter::default())
        .await
        .expect("re-subscribe");
    drop(sub2);
}

#[tokio::test]
async fn explicit_unsubscribe_returns_ok() {
    let runtime = ARCPRuntime::builder()
        .with_authenticator(Box::new(BearerAuthenticator::new().with_token("t", "p")))
        .with_capabilities(Capabilities {
            subscriptions: Some(true),
            ..Default::default()
        })
        .build()
        .await
        .expect("build");
    let (server_t, client_t) = paired();
    let _h = runtime.serve_connection(server_t);

    let session = ARCPClient::new(client_t)
        .open()
        .expect("open")
        .authenticate(
            Credentials {
                scheme: AuthScheme::Bearer,
                token: Some("t".into()),
            },
            ClientIdentity {
                kind: "unsub-test".into(),
                version: "0".into(),
                fingerprint: None,
                principal: None,
            },
            Capabilities {
                subscriptions: Some(true),
                ..Default::default()
            },
        )
        .await
        .expect("auth");

    let sub = session
        .subscribe(SubscriptionFilter::default())
        .await
        .expect("subscribe");
    sub.unsubscribe().await.expect("explicit unsubscribe");
}
