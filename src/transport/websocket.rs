//! WebSocket transport (RFC §22). Phase 6 implementation.
//!
//! Phase 2 ships only the module shell so the `transport-ws` feature flag
//! compiles. The connection dialler and listener land in Phase 6.

use crate::error::ARCPError;

/// Placeholder marker; real `WebSocketTransport` lands in Phase 6.
#[derive(Debug, Default)]
pub struct WebSocketTransport;

impl WebSocketTransport {
    /// Construct.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Dial a remote runtime.
    ///
    /// # Errors
    ///
    /// Always returns [`ARCPError::Unimplemented`] in Phase 2.
    pub fn dial(_url: &str) -> Result<Self, ARCPError> {
        Err(ARCPError::Unimplemented {
            section: "22",
            detail: "WebSocket transport lands in Phase 6".into(),
        })
    }
}
