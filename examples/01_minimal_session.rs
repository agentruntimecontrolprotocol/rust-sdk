//! Minimal session example: spawn an in-process runtime, connect a client
//! over the in-memory transport, complete the four-step handshake, and
//! print the negotiated session id.

use arcp::auth::BearerAuthenticator;
use arcp::messages::{AuthScheme, Capabilities, ClientIdentity, Credentials};
use arcp::runtime::ARCPRuntime;
use arcp::transport::paired;
use arcp::ARCPClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let runtime = ARCPRuntime::builder()
        .with_authenticator(Box::new(
            BearerAuthenticator::new().with_token("hunter2", "alice"),
        ))
        .with_capabilities(Capabilities {
            streaming: Some(true),
            human_input: Some(true),
            ..Default::default()
        })
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
                token: Some("hunter2".into()),
            },
            ClientIdentity {
                kind: "example-01".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                fingerprint: None,
                principal: Some("alice@example.com".into()),
            },
            Capabilities {
                streaming: Some(true),
                human_input: Some(true),
                ..Default::default()
            },
        )
        .await?;

    println!("session opened: {}", session.id().await?);
    println!(
        "negotiated capabilities: {:#?}",
        session.capabilities().await
    );
    Ok(())
}
