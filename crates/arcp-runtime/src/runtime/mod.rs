//! ARCP runtime — the server side of the protocol.
//!
//! - [`server::ARCPRuntime`] — accepts a
//!   [`Transport`][arcp_core::transport::Transport], drives the ARCP v1.1
//!   §6.2 hello/welcome handshake (which the SDK serializes as
//!   `session.open` / `session.accepted`, optionally with an SDK-extension
//!   challenge/authenticate pair between them), and dispatches subsequent
//!   envelopes by exhaustive match on
//!   [`MessageType`][arcp_core::messages::MessageType].
//! - [`session::SessionState`] — tracked per-session bookkeeping.
//! - [`job::JobRegistry`] — §7 job lifecycle.
//! - [`subscription::SubscriptionManager`] — §7.6 cross-session subscription.
//! - [`credentials`] — §9.8 provisioned credentials.
//! - [`artifact::ArtifactStore`] — SDK extension; no v1.1 normative analog.

pub mod artifact;
pub mod context;
pub mod credentials;
pub mod job;
pub mod server;
pub mod session;
pub mod subscription;
pub mod tools;

pub use arcp_core::messages::{CredentialId, CredentialScheme, ProvisionedCredential};
pub use artifact::ArtifactStore;
pub use context::ToolContext;
pub use credentials::{
    CredentialJobContext, CredentialLedger, CredentialProvisioner, InMemoryCredentialProvisioner,
};
pub use job::{JobEntry, JobRegistry, JobState};
pub use server::{ARCPRuntime, RuntimeBuilder};
pub use session::SessionState;
pub use subscription::{FilteredReceiver, SubscriptionManager};
pub use tools::{ToolHandler, ToolRegistry, ToolRegistryBuilder};
