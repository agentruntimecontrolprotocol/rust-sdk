//! Transport abstraction (RFC §22).
//!
//! Each transport delivers and receives [`Envelope`]-shaped messages. The
//! trait is async, object-safe via `#[async_trait]`, and intentionally
//! minimal: send one envelope, receive the next, close when done. Higher-
//! level concerns (idempotency, ordering, backpressure) live above this
//! layer.
//!
//! ## Examples
//!
//! ```rust
//! use arcp::messages::{MessageType, PingPayload};
//! use arcp::transport::{paired, Transport};
//! use arcp::Envelope;
//!
//! # async fn demo() -> Result<(), Box<dyn std::error::Error>> {
//! let (sender, receiver) = paired();
//! sender.send(Envelope::new(MessageType::Ping(PingPayload::default()))).await?;
//! let env = receiver.recv().await?.expect("envelope");
//! assert_eq!(env.payload.type_name(), "ping");
//! # Ok(())
//! # }
//! ```

use async_trait::async_trait;

use crate::envelope::Envelope;
use crate::error::ARCPError;

pub mod memory;
#[cfg(feature = "transport-stdio")]
pub mod stdio;
#[cfg(feature = "transport-ws")]
pub mod websocket;

pub use memory::{paired, MemoryTransport};

/// Object-safe transport interface.
#[async_trait]
pub trait Transport: Send + Sync {
    /// Send one envelope. Blocks until the underlying transport has accepted
    /// the bytes (which may be sooner than the peer has received them).
    ///
    /// # Errors
    ///
    /// Returns [`ARCPError::Unavailable`] if the transport is closed,
    /// [`ARCPError::Serialization`] for an encoding failure, or
    /// [`ARCPError::Internal`] for transport-specific failures.
    async fn send(&self, envelope: Envelope) -> Result<(), ARCPError>;

    /// Receive the next envelope. Returns `Ok(None)` when the transport
    /// has been closed cleanly.
    ///
    /// # Errors
    ///
    /// Returns [`ARCPError::Serialization`] for a decoding failure or
    /// [`ARCPError::Internal`] for transport-specific failures.
    async fn recv(&self) -> Result<Option<Envelope>, ARCPError>;

    /// Close the transport.
    ///
    /// # Errors
    ///
    /// Returns [`ARCPError::Internal`] if the underlying close fails.
    async fn close(&self) -> Result<(), ARCPError>;
}
