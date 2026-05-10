//! stdio transport (RFC §22). Phase 6 implementation.
//!
//! Phase 2 ships only the module shell so the `transport-stdio` feature
//! flag compiles. Newline-delimited JSON over `tokio::io::stdin/stdout`
//! lands in Phase 6.

use crate::error::ARCPError;

/// Placeholder marker; real `StdioTransport` lands in Phase 6.
#[derive(Debug, Default)]
pub struct StdioTransport;

impl StdioTransport {
    /// Construct.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Begin reading/writing on the process stdio handles.
    ///
    /// # Errors
    ///
    /// Always returns [`ARCPError::Unimplemented`] in Phase 2.
    pub fn attach() -> Result<Self, ARCPError> {
        Err(ARCPError::Unimplemented {
            section: "22",
            detail: "stdio transport lands in Phase 6".into(),
        })
    }
}
