//! Shared helpers for integration tests.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_panics_doc,
    dead_code,
    unreachable_pub
)]

use arcp::auth::{BearerAuthenticator, NoneAuthenticator};
use arcp::messages::{Capabilities, ClientIdentity, Credentials};
use arcp::runtime::ARCPRuntime;
use arcp::transport::{paired, MemoryTransport};
use arcp::{ARCPClient, IMPL_KIND, IMPL_VERSION};

/// Construct a runtime with a bearer authenticator pre-loaded with one
/// `(token, principal)` pair, optional anonymous capability, and a paired
/// in-memory transport for the client.
pub async fn spawn_runtime_with_bearer(
    token: &str,
    principal: &str,
    anonymous: bool,
) -> (ARCPRuntime, ARCPClient<MemoryTransport>) {
    let mut caps = Capabilities {
        streaming: Some(true),
        human_input: Some(true),
        artifacts: Some(true),
        ..Default::default()
    };
    if anonymous {
        caps.anonymous = Some(true);
    }
    let runtime = ARCPRuntime::builder()
        .with_authenticator(Box::new(
            BearerAuthenticator::new().with_token(token, principal),
        ))
        .with_authenticator(Box::new(NoneAuthenticator::new()))
        .with_capabilities(caps)
        .build()
        .await
        .expect("build runtime");
    let (server_t, client_t) = paired();
    let _handle = runtime.serve_connection(server_t);
    let client = ARCPClient::new(client_t);
    (runtime, client)
}

/// A canonical client identity block for tests.
pub fn test_client_identity() -> ClientIdentity {
    ClientIdentity {
        kind: format!("{IMPL_KIND}-test"),
        version: IMPL_VERSION.into(),
        fingerprint: None,
        principal: Some("test-user".into()),
    }
}

/// Bearer credentials with the given token.
pub fn bearer(token: &str) -> Credentials {
    Credentials {
        scheme: arcp::messages::AuthScheme::Bearer,
        token: Some(token.into()),
    }
}

/// `none`-scheme credentials.
pub const fn none_creds() -> Credentials {
    Credentials {
        scheme: arcp::messages::AuthScheme::None,
        token: None,
    }
}
