//! # arcp — Agent Runtime Control Protocol (reference implementation)
//!
//! This crate is a Rust reference implementation of [ARCP v1.1][rfc], the
//! Agent Runtime Control Protocol. See `CONFORMANCE.md` for the per-section
//! status and `docs/` for narrative guides.
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
//! ## Status
//!
//! The public API centers on [`ARCPClient`] for consumers and [`ARCPRuntime`]
//! for runtimes.
//!
//! ## Example
//!
//! ```
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
//! # fn main() {
//! #     let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
//! #     rt.block_on(run()).unwrap();
//! # }
//! ```
//!
//! [rfc]: https://github.com/agentruntimecontrolprotocol/spec/blob/main/docs/draft-arcp-1.1.md

#![deny(unsafe_code)]
#![deny(missing_docs)]
#![warn(unreachable_pub)]

pub mod auth;
pub mod client;
pub mod envelope;
pub mod error;
pub mod extensions;
pub mod ids;
pub mod messages;
pub mod runtime;
pub mod store;
pub mod transport;

pub use client::ARCPClient;
pub use envelope::{Envelope, Priority, RawEnvelope};
pub use error::{ARCPError, ErrorCode};
pub use extensions::{ExtensionRegistry, TypeClassification};
pub use messages::{Capabilities, MessageType};
pub use runtime::ARCPRuntime;

/// Protocol version implemented by this crate, as carried in the `arcp` field
/// of every envelope (RFC §6.1).
pub const PROTOCOL_VERSION: &str = "1.1";

/// Implementation version of this crate, derived from `Cargo.toml`.
pub const IMPL_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Implementation kind reported in `runtime.kind` / `client.kind` blocks
/// (RFC §8.2, §8.3).
pub const IMPL_KIND: &str = "arcp-rs";
