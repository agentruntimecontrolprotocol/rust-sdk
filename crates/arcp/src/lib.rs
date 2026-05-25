//! # arcp — Agent Runtime Control Protocol (reference implementation)
//!
//! Umbrella crate that re-exports the three primary ARCP crates:
//!
//! - [`arcp_core`][arcp_core] — wire-format envelopes, message payloads,
//!   error taxonomy, IDs, transport trait + in-memory transport,
//!   authenticator trait.
//! - [`arcp_client`][arcp_client] — [`ARCPClient`] / type-state `Session`.
//! - [`arcp_runtime`][arcp_runtime] — [`ARCPRuntime`], job machinery,
//!   `SQLite` store, bearer / JWT / none auth validators.
//!
//! See `CONFORMANCE.md` for per-section RFC status and `docs/` for
//! narrative guides.
//!
//! ## Scope
//!
//! The crate implements the protocol fundamentals: envelopes, sessions and
//! authentication (`bearer`, `signed_jwt`, `none`), capability negotiation,
//! jobs, streams, permissions, leases, subscriptions, artifacts, the canonical
//! error taxonomy, observability primitives, and the `WebSocket`, stdio, and
//! in-memory transports.
//!
//! Out-of-scope items (`HTTP/2`, `QUIC`, `mTLS`, `OAuth2`, sidecar binary
//! frames, scheduled jobs, multi-agent delegation, workflows, trust
//! elevation, checkpoint-based resume) return `ARCPError::Unimplemented`
//! when invoked.
//!
//! ## Cargo features
//!
//! - `client` (default) — pulls in [`arcp-client`][arcp_client].
//! - `runtime` (default) — pulls in [`arcp-runtime`][arcp_runtime].
//! - `transport-ws`, `transport-stdio` (default) — transport implementations.
//!
//! To slim builds, opt out of the side you don't need:
//!
//! ```toml
//! arcp = { version = "2", default-features = false, features = ["client", "transport-ws"] }
//! ```
//!
//! ## Example
//!
//! ```
//! # #[cfg(all(feature = "client", feature = "runtime"))]
//! # mod demo {
//! use std::sync::Arc;
//!
//! use arcp::auth::BearerAuthenticator;
//! use arcp::messages::{AuthScheme, Capabilities, ClientIdentity, Credentials};
//! use arcp::runtime::{ARCPRuntime, ToolContext, ToolHandler, ToolRegistryBuilder};
//! use arcp::transport::paired;
//! use arcp::ARCPClient;
//! use async_trait::async_trait;
//!
//! struct Echo;
//!
//! #[async_trait]
//! impl ToolHandler for Echo {
//!     fn name(&self) -> &'static str { "echo" }
//!     async fn invoke(
//!         &self,
//!         input: serde_json::Value,
//!         _ctx: ToolContext,
//!     ) -> Result<serde_json::Value, arcp::ARCPError> {
//!         Ok(input)
//!     }
//! }
//!
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! let tools = ToolRegistryBuilder::new().with(Arc::new(Echo)).build();
//! let runtime = ARCPRuntime::builder()
//!     .with_authenticator(Box::new(BearerAuthenticator::new().with_token("tok", "alice")))
//!     .with_tools(tools)
//!     .build()
//!     .await?;
//! let (server_t, client_t) = paired();
//! let _server = runtime.serve_connection(server_t);
//! let session = ARCPClient::new(client_t)
//!     .open()?
//!     .authenticate(
//!         Credentials { scheme: AuthScheme::Bearer, token: Some("tok".into()) },
//!         ClientIdentity {
//!             kind: "demo".into(), version: "1.0".into(),
//!             fingerprint: None, principal: None,
//!         },
//!         Capabilities::default(),
//!     )
//!     .await?;
//! let result = session.invoke("echo", serde_json::json!({"hi": "arcp"})).await?.join().await?;
//! assert_eq!(result["hi"], "arcp");
//! # Ok(())
//! # }
//! # }
//! ```
//!
//! [rfc]: https://github.com/agentruntimecontrolprotocol/spec/blob/main/docs/draft-arcp-1.1.md

#![deny(unsafe_code)]
#![deny(missing_docs)]
#![warn(unreachable_pub)]

// Re-export modules from arcp-core at their canonical paths.
pub use arcp_core::{envelope, error, extensions, ids, messages, transport};

/// Authentication scheme adapters (RFC §8.2).
///
/// The [`Authenticator`][arcp_core::auth::Authenticator] trait,
/// [`AuthOutcome`][arcp_core::auth::AuthOutcome], and
/// [`AuthRegistry`][arcp_core::auth::AuthRegistry] live in `arcp-core`.
/// Concrete validators ([`BearerAuthenticator`][arcp_runtime::auth::BearerAuthenticator],
/// [`SignedJwtAuthenticator`][arcp_runtime::auth::SignedJwtAuthenticator],
/// [`NoneAuthenticator`][arcp_runtime::auth::NoneAuthenticator]) live in
/// `arcp-runtime` and are re-exported here so existing import paths
/// continue to work.
pub mod auth {
    pub use arcp_core::auth::*;
    #[cfg(feature = "runtime")]
    pub use arcp_runtime::auth::*;
}

/// Reference client (consumer side). Re-export of
/// [`arcp_client::api`][arcp_client::api].
#[cfg(feature = "client")]
pub use arcp_client::api as client;

/// Reference runtime (server side). Re-export of
/// [`arcp_runtime::runtime`][arcp_runtime::runtime].
#[cfg(feature = "runtime")]
pub use arcp_runtime::runtime;

/// SQLite-backed event log and credential ledger. Re-export of
/// [`arcp_runtime::store`].
#[cfg(feature = "runtime")]
pub use arcp_runtime::store;

// Convenience top-level re-exports — match v1.x public surface.
pub use arcp_core::{
    ARCPError, Capabilities, Envelope, ErrorCode, ExtensionRegistry, MessageType, Priority,
    RawEnvelope, TypeClassification, IMPL_KIND, IMPL_VERSION, PROTOCOL_VERSION,
};

#[cfg(feature = "client")]
pub use arcp_client::ARCPClient;

#[cfg(feature = "runtime")]
pub use arcp_runtime::ARCPRuntime;
