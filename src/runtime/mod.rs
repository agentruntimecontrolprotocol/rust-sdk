//! ARCP runtime — the server side of the protocol.
//!
//! Phase 2 ships:
//!
//! - [`server::ARCPRuntime`] — accepts a [`Transport`][crate::transport::Transport],
//!   drives the four-step handshake (RFC §8.1), and dispatches subsequent
//!   envelopes by exhaustive match on [`MessageType`][crate::messages::MessageType].
//! - [`session::SessionState`] — tracked per-session bookkeeping.
//!
//! Job state machines, streams, subscriptions, leases, and artifacts land
//! in Phases 3–5.

pub mod artifact;
pub mod context;
pub mod credentials;
pub mod job;
pub mod server;
pub mod session;
pub mod subscription;
pub mod tools;

pub use artifact::ArtifactStore;
pub use context::{HumanResponse, ToolContext};
pub use credentials::{
    CredentialId, CredentialJobContext, CredentialLedger, CredentialProvisioner, CredentialScheme,
    InMemoryCredentialProvisioner, ProvisionedCredential,
};
pub use job::{JobEntry, JobRegistry, JobState};
pub use server::{ARCPRuntime, RuntimeBuilder};
pub use session::SessionState;
pub use subscription::{FilteredReceiver, SubscriptionManager};
pub use tools::{ToolHandler, ToolRegistry, ToolRegistryBuilder};
